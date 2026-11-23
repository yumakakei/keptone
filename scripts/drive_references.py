#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of an operator envelope driven by a moving rates' key for keptone-engine's tests.

    python3 scripts/drive_references.py

Runs, in exact arithmetic at binary64 inputs, an operator envelope of docs/MATH.md §4 whose rates' key k_R moves with
the pitch envelope under scaling.rate_key = frequency (§2.6, §4.7): the pitch envelope as scripts/peg_references.py
runs it, k_R = k' + 12 P_EG on the pieces of each sample between its corners; on each piece the segment's progress,
the integral of rho over the key's law split where rho has a corner, as scripts/progress_references.py computes it;
the instant at which a segment's progress reaches its end, where the next segment starts; and the envelope's laws as
scripts/envelope_references.py has them. Each scenario draws an envelope, a pitch envelope, a key, the settings and
an engine rate from SplitMix64 with a fixed seed, keys both on at sample 0 and off later, and records the level at
every sample and the instant at which each segment ends.

Beside each value it gives a bound on keptone-engine's error, first order in u = 2^-53: the errors that
keptone-engine's functions document, of the pitch envelope's level and slope and its corners' instants, which move
the key that the engine takes, of each stretch's progress at that key, of the instants at which segments end, and of
the envelope's laws, followed through every sample. It writes crates/keptone-engine/tests/data/drive.txt, every
number as the 16 hexadecimal digits of its binary64, so that the file is the same every time.
"""

from collections.abc import Callable, Iterator
from pathlib import Path

import mpmath

import peg_references
from binary64 import splitmix64, to_bits, uniform
from envelope_references import KNEE, WAIT, Law, attack_progress, exact, jump, own_law, shown, target, ulps
from progress_references import HIGHEST, LOWEST, Key, Rates, reach, stretch_error, stretch_progress, stretches

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "drive.txt"
SEED = 0x6472697665
SCENARIOS = 60

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/drive_references.py with mpmath; do not edit.
# A scenario's lines, every number the binary64 bits in hexadecimal:
#   scenario rate clock law jump waits key offset samples release, then the
#     envelope's rates, levels and rate scaling, and the pitch envelope's rates
#     and levels
#   level sample value bound (value 0 and bound -1 where it is silent)
#   end value bound, the instant at which a segment ends in samples
"""

type Level = tuple[mpmath.mpf | None, mpmath.mpf]


class Driven:
    """keptone-engine's envelope (MATH §4) driven by a moving rates' key, in exact arithmetic, with the bounds of the
    errors of keptone-engine's: its stage, its law, the progress at which its segment ends, what its wait has counted
    and its piece's progress so far."""

    def __init__(self, parameters: dict, settings: dict, offset: float) -> None:
        """An envelope resting at L4 at an offset."""
        self.parameters, self.settings = parameters, settings
        self.offset = exact(offset)
        self.stage, self.resting, self.held = 3, True, False
        to = target(parameters["levels"][3], self.offset)
        self.law = Law("still", to, (None, mpmath.mpf(0)))
        self.end_progress: tuple[mpmath.mpf, mpmath.mpf] | None = None
        self.counted = (mpmath.mpf(0), mpmath.mpf(0))
        self.progress = (mpmath.mpf(0), mpmath.mpf(0))

    def rates(self) -> Rates:
        """The rates of its stage."""
        index = 2 if self.held else self.stage
        return Rates(
            self.parameters["rates"][index],
            self.parameters["scaling"],
            self.settings["clock"],
            self.settings["law"],
            self.settings["rate"],
        )

    def level(self) -> Level:
        """T at its piece's progress, with its error."""
        value, error = self.progress
        return self.law.level(value, error + ulps() * abs(value))

    def key_on(self) -> None:
        """A key-on: segment 0 from the level there."""
        self.begin(0, self.level())

    def key_off(self) -> None:
        """A key-off: segment 3 from the level there, but where segment 3 has begun."""
        if self.stage == 3:
            return
        self.begin(3, self.level())

    def begin(self, segment: int, start: Level) -> None:
        """Starts a segment from a level."""
        self.stage, self.held, self.resting = segment, False, False
        self.counted = (mpmath.mpf(0), mpmath.mpf(0))
        self.anchor(start)

    def anchor(self, start: Level) -> None:
        """Anchors the law from a level, and the segment's end, as scripts/envelope_references.py does."""
        u = ulps()
        index = 2 if self.held else self.stage
        to = target(self.parameters["levels"][index], self.offset)
        shown_start, shown_to = shown(start), shown(to)
        if shown_to[0] > shown_start[0]:
            first = jump(shown_start, shown_to) if self.settings["jump"] else shown_start
            moved_value = attack_progress(first[0], shown_to[0])
            beyond = u * abs(shown_to[0] - KNEE) if shown_to[0] > KNEE else 0
            moved = (moved_value, 6.62 * u * moved_value + beyond + first[1] + shown_to[1])
            self.law = Law("rise", first, shown_to)
        else:
            moved_value = shown_start[0] - shown_to[0]
            moved = (moved_value, shown_start[1] + shown_to[1] + u * moved_value)
            self.law = own_law(start, to, jumps=bool(self.settings["jump"]))
        distance = abs(shown_to[0] - shown_start[0])
        if self.settings["waits"] and distance < 1:
            waited_value = WAIT * (1 - distance) / (20 if index == 0 else 1)
            distance_error = shown_start[1] + shown_to[1] + u * distance
            waited = (waited_value, WAIT * distance_error + 3 * u * waited_value)
        else:
            waited = (mpmath.mpf(0), mpmath.mpf(0))
        self.progress = (mpmath.mpf(0), mpmath.mpf(0))
        if self.held or self.resting:
            self.end_progress = None
            return
        rest = waited[0] - self.counted[0]
        rest_error = waited[1] + self.counted[1] + u * abs(rest)
        self.end_progress = (max(moved[0], rest), max(moved[1], rest_error))

    def finish(self) -> None:
        """Ends the segment where its progress reaches its end, and starts what follows."""
        if self.stage < 2:
            level = self.law.level(*self.end_progress)
            self.begin(self.stage + 1, level)
        elif self.stage == 2:
            self.held, self.end_progress = True, None
        else:
            self.resting, self.end_progress = True, None

    def drive(self, pieces: list[tuple], start: mpmath.mpf, ends: list[tuple[mpmath.mpf, mpmath.mpf]]) -> None:
        """Runs it over a sample's pieces of the key from the sample's start, recording each segment's end."""
        u = ulps()
        for offset, length, key, key_error in pieces:
            done = mpmath.mpf(0)
            while True:
                rates = self.rates()
                here = Key(key.key + key.slope * done, key.slope, 0.0, 0.0)
                parts = stretches(rates, here, length - done)
                progresses = [stretch_progress(rates, here, part) for part in parts]
                errors = [
                    stretch_error(rates, here, part, progress) + progress * slope_of(rates, part) * key_error / 4
                    for part, progress in zip(parts, progresses, strict=True)
                ]
                total = sum(progresses, mpmath.mpf(0))
                value, error = self.progress
                if self.end_progress is not None and value + total >= self.end_progress[0]:
                    left = self.end_progress[0] - value
                    t, index = reach(rates, here, parts, left)
                    rho = rates.step(here.at(t))
                    before = error + self.end_progress[1] + sum(errors[: index + 1], mpmath.mpf(0)) + u * left
                    instant = start + offset + done + t
                    instant_error = before / rho + 8 * u * abs(t) + u * abs(instant)
                    ends.append((instant, instant_error))
                    self.finish()
                    self.progress = (mpmath.mpf(0), self.rates().step(here.at(t)) * instant_error)
                    if self.held or self.resting:
                        self.progress = (value + left, error + before)
                    done += t
                    continue
                self.progress = (value + total, error + sum(errors, mpmath.mpf(0)))
                break


def slope_of(rates: Rates, part: tuple) -> mpmath.mpf:
    """q's slope in the key on a stretch, 0 where q holds still."""
    middle = (part[2] + part[3]) / 2
    return abs(rates.slope(middle)) if LOWEST < middle < HIGHEST else mpmath.mpf(0)


def key_pieces(
    pitch: peg_references.Envelope, sample: int, key: float
) -> list[tuple[mpmath.mpf, mpmath.mpf, Key, mpmath.mpf]]:
    """The pieces of k_R = k' + 12 P_EG over the sample from sample - 1 to sample, between the pitch envelope's
    corners, with the bound of the error of the key that keptone-engine takes on each: twelve times the pitch
    envelope's level's and slope's errors, its corners' instants' times its slope, and the key's rounding."""
    u = ulps()
    start, end = mpmath.mpf(sample - 1), mpmath.mpf(sample)
    t = start
    out = []
    while t < end:
        while (corner := pitch.end()) is not None and corner[0] <= t:
            pitch.next()
        level, level_error = pitch.level(t)
        corner = pitch.end()
        stop = min(end, corner[0]) if corner is not None and corner[0] > t else end
        moving = corner is not None and corner[0] > t
        step, step_error = pitch.step
        slope = (step if pitch.to[0] >= pitch.source[0] else -step) if moving else mpmath.mpf(0)
        value = exact(key) + 12 * level
        corner_error = corner[1] if corner is not None else mpmath.mpf(0)
        error = 12 * level_error + u * abs(value) + 12 * abs(slope) * (step_error + corner_error + u)
        out.append((t - start, stop - t, Key(value, 12 * slope, 0.0, 0.0), error))
        t = stop
    while (corner := pitch.end()) is not None and corner[0] <= end:
        pitch.next()
    return out


def draw(randoms: Iterator[int], low: float, high: float) -> float:
    """A uniform draw from [low, high)."""
    return uniform(randoms, low, high)


def pick(randoms: Iterator[int], choices: list[Callable[[], float]]) -> float:
    """One of the choices, each as likely."""
    return choices[next(randoms) % len(choices)]()


def whole(randoms: Iterator[int], low: int, high: int) -> float:
    """A whole number from low to high."""
    return float(low + next(randoms) % (high - low + 1))


def scenario(randoms: Iterator[int]) -> tuple[dict, dict, dict, float, float, int, int]:
    """A scenario: the settings; an envelope whose rate scaling makes its speed follow the key, its rates fast enough
    to end segments within it; a pitch envelope that moves the key across ramps of q, some of its segments far faster
    than their scale, so that it holds within the scenario and the closed form takes over; a key, an offset, a count
    of samples and the sample of the key-off."""
    settings = {
        "rate": pick(randoms, [lambda: 8000.0, lambda: 44100.0, lambda: 48000.0, lambda: 96000.0, lambda: 192000.0]),
        "clock": pick(randoms, [lambda: 49096.0, lambda: 44100.0]),
        "law": next(randoms) % 2,
        "jump": next(randoms) % 4 != 0,
        "waits": next(randoms) % 4 != 0,
    }

    def rate() -> float:
        return pick(randoms, [lambda: whole(randoms, 75, 99), lambda: draw(randoms, 70.0, 99.0)])

    def level() -> float:
        return pick(randoms, [lambda: whole(randoms, 0, 99), lambda: draw(randoms, 0.0, 99.0)])

    envelope = {
        "rates": [rate() for _ in range(4)],
        "levels": [level() for _ in range(4)],
        "scaling": pick(randoms, [lambda: whole(randoms, 3, 7), lambda: draw(randoms, 3.0, 9.0)]),
    }
    pitch = {
        "rates": [
            pick(randoms, [lambda: whole(randoms, 80, 99), lambda: draw(randoms, 80.0, 99.0), lambda: 1e7])
            for _ in range(4)
        ],
        "levels": [pick(randoms, [lambda: whole(randoms, 0, 99), lambda: draw(randoms, 0.0, 99.0)]) for _ in range(4)],
    }
    key = pick(randoms, [lambda: whole(randoms, 40, 90), lambda: draw(randoms, 30.0, 100.0)])
    offset = pick(randoms, [lambda: 4064.0, lambda: draw(randoms, 3000.0, 4200.0)])
    samples = int(pick(randoms, [lambda: whole(randoms, 200, 600), lambda: whole(randoms, 600, 2000)]))
    release = samples // 3 + next(randoms) % (samples // 3)
    return settings, envelope, pitch, key, offset, samples, release


def run(randoms: Iterator[int]) -> list[list[str | float]]:
    """A scenario's lines."""
    settings, envelope, pitch_parameters, key, offset, samples, release = scenario(randoms)
    pitch = peg_references.Envelope(pitch_parameters, settings["rate"])
    pitch.key_on(mpmath.mpf(0))
    driven = Driven(envelope, settings, offset)
    driven.key_on()
    head = [settings["rate"], settings["clock"], float(settings["law"]), float(settings["jump"])]
    head += [float(settings["waits"]), key, offset, float(samples), float(release)]
    head += [*envelope["rates"], *envelope["levels"], envelope["scaling"]]
    head += [*pitch_parameters["rates"], *pitch_parameters["levels"]]
    lines: list[list[str | float]] = [["scenario", *head]]
    for sample in range(1, samples + 1):
        pieces = key_pieces(pitch, sample, key)
        ends: list[tuple[mpmath.mpf, mpmath.mpf]] = []
        driven.drive(pieces, mpmath.mpf(sample - 1), ends)
        lines.extend(["end", float(instant), float(error * (1 + mpmath.mpf(10) ** -6))] for instant, error in ends)
        level = driven.level()
        if level[0] is None:
            lines.append(["level", float(sample), 0.0, -1.0])
        else:
            lines.append(["level", float(sample), float(level[0]), float(level[1] * (1 + mpmath.mpf(10) ** -6))])
        if sample == release:
            pitch.key_off(mpmath.mpf(sample))
            driven.key_off()
    return lines


def main() -> None:
    """Writes the file."""
    mpmath.mp.dps = 40
    randoms = splitmix64(SEED)
    out = [HEADER.rstrip("\n")]
    for _ in range(SCENARIOS):
        for kind, *numbers in run(randoms):
            out.append(" ".join([str(kind), *(f"{to_bits(float(x)):016x}" for x in numbers)]))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
