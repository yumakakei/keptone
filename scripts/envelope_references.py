#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of the operator envelope for keptone-engine's tests.

    python3 scripts/envelope_references.py

Runs the envelope of docs/MATH.md §4 as keptone-engine runs it, in exact
arithmetic, with fractions and with mpmath at 400 bits, at binary64 inputs: an
operator's segments toward L1 to L4, timed by the DX7's levels, max(T, 16),
while the level itself goes on below the floor (§4.6); the attack, with its
jump, and the decay (§4.2, §4.3); the waits of segments that move by less than
a step (§4.4); key-ons, key-offs and the holds after segments 2 and 3 (§4.5);
and changes of the rates, the levels, the rate scaling, the key of the rates
and the offset during a segment, the waits keeping what they have counted
(§4.7). Each scenario draws an operator's parameters, the settings, a key, an
offset and an engine rate, and events at whole samples, from SplitMix64 with a
fixed seed, on the scales, between their steps and beyond them. It records
the instants at which segments end, and the levels at whole samples between
them, none nearer an end than 10^-6 of a sample or ten times the bound of the
end's error, and draws a scenario again where an end lies that near an event.

Beside each value it gives a bound on keptone-engine's error, first order in
u = 2^-53: it follows the errors that keptone-engine's functions document, of
a target, a rate, a speed, the progress, each law, the jump, the wait, a
segment's end and its instant, through every piece of law of the scenario, as
keptone-engine computes each from the one before. It writes
crates/keptone-engine/tests/data/envelope.txt, every number as the 16
hexadecimal digits of its binary64, so that the file is the same every time.
"""

import itertools
import math
from collections.abc import Callable, Iterator
from fractions import Fraction
from pathlib import Path

import mpmath

from binary64 import splitmix64, to_bits, uniform
from eg_references import ASYMPTOTE, KNEE, below_knee, rate_law, to_knee
from level_references import envelope as level_envelope

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "envelope.txt"
SEED = 0x7365676D656E7473
SCENARIOS = 120

FLOOR = 16
JUMP = 1716
WAIT = 480

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/envelope_references.py with mpmath; do not edit.
# Scenarios of the operator envelope, every number the binary64 bits in
# hexadecimal, a scenario's lines in the order of its time:
#   scenario rate clock law(0 dx7, 1 exponential) jump(0, 1) waits(0, 1)
#     R1 R2 R3 R4 L1 L2 L3 L4 KRS key offset
#   key_on sample | key_off sample | offset sample offset
#   change sample R1 R2 R3 R4 L1 L2 L3 L4 KRS key
#   end segment, then the instant in samples as a double-double and the bound
#     of its error
#   level sample, then T as a double-double and the bound of its error
#   silent sample
"""

type Line = list[str | float]
type Level = tuple[mpmath.mpf | None, mpmath.mpf]


def exact(value: float | Fraction) -> mpmath.mpf:
    """A binary64 or a rational as an mpf, exactly where its bits fit."""
    value = Fraction(value)
    return mpmath.mpf(value.numerator) / value.denominator


def double_double(value: mpmath.mpf) -> list[float]:
    """value as hi + lo, hi the nearest binary64 and lo the nearest to the rest."""
    hi = float(value)
    return [hi, float(value - mpmath.mpf(hi))]


def ulps() -> mpmath.mpf:
    """u = 2^-53."""
    return mpmath.mpf(2) ** -53


def target(level: float, offset: mpmath.mpf) -> Level:
    """T(L) = E(L) + O - 4256 with the bound of its error, (None, 0) for L at or below 0: E(L)
    within u of itself, and 415u|log2 L| more below 1, and the two sums within u each."""
    part = level_envelope(level)
    if part is None:
        return None, mpmath.mpf(0)
    part = exact(part) if isinstance(part, Fraction) else part
    u = ulps()
    error = u * abs(part) + (415 * u * abs(mpmath.log(exact(level), 2)) if level < 1 else 0)
    total = part + offset - 4256
    return total, error + u * abs(part + offset) + u * abs(total)


def speed(rate: Fraction, clock: float, law: int) -> mpmath.mpf:
    """rho(q) of MATH §4.1 at an exact rate."""
    base = exact(clock) / 4096
    if law == 0:
        quarter = math.floor(rate / 4)
        return base * mpmath.mpf(2) ** quarter * exact(1 + (rate - 4 * quarter) / 4)
    return base * mpmath.power(2, exact(rate) / 4)


def attack(start: mpmath.mpf, progress: mpmath.mpf) -> mpmath.mpf:
    """The level of an attack from a start after a progress (MATH §4.3)."""
    if start >= KNEE:
        return start + progress
    knee = to_knee(start)
    if progress >= knee:
        return KNEE + (progress - knee)
    return below_knee(start, progress)


def attack_progress(start: mpmath.mpf, target_level: mpmath.mpf) -> mpmath.mpf:
    """The progress at which an attack from a start reaches a target."""
    if target_level <= start:
        return mpmath.mpf(0)
    if start >= KNEE:
        return target_level - start
    if target_level <= KNEE:
        return 256 * mpmath.log((ASYMPTOTE - start) / (ASYMPTOTE - target_level))
    return to_knee(start) + (target_level - KNEE)


def jump(start: Level, to: Level) -> Level:
    """The level from which a rising segment starts, with its error, that of the level it is."""
    if start[0] < JUMP and start[0] < to[0]:
        return (mpmath.mpf(JUMP), mpmath.mpf(0)) if to[0] > JUMP else to
    return start


def shown(level: Level) -> Level:
    """A level as the DX7 takes it, no lower than its floor, with its error."""
    if level[0] is None or level[0] < FLOOR:
        return mpmath.mpf(FLOOR), (mpmath.mpf(0) if level[0] is None else level[1])
    return level


class Law:
    """A piece's law, its level a function of its progress: still, a fall or a rise."""

    def __init__(self, kind: str, start: Level, to: Level) -> None:
        """The law of its kind from a level to a target, with their errors."""
        self.kind, self.start, self.to = kind, start, to

    def level(self, progress: mpmath.mpf, error: mpmath.mpf) -> Level:
        """The level after a progress, with the bound of the error that keptone-engine's makes
        from the progress's error: a fall within u of its level beside its start's and the
        progress's errors, an attack within u|T| + 4.62u(|T - T0| + Phi) beside its start's
        and the progress's times its slope at its start, each stopping at its target, within
        the larger error there."""
        u = ulps()
        if self.kind == "still":
            return self.start
        value, start_error = self.start
        if self.kind == "fall":
            fallen = value - progress
            moved = (fallen, start_error + error + u * abs(fallen))
            if self.to[0] is not None and fallen <= self.to[0]:
                return self.to[0], max(moved[1], self.to[1])
            return moved
        risen = attack(value, progress)
        slope = max(1, (ASYMPTOTE - value) / 256)
        moved_error = start_error + slope * error + u * abs(risen) + 4.62 * u * (abs(risen - value) + progress)
        if risen >= self.to[0]:
            return self.to[0], max(moved_error, self.to[1])
        return risen, moved_error


def own_law(start: Level, to: Level, *, jumps: bool) -> Law:
    """The law of a segment that the DX7 does not see rise: its own level's, toward its own target."""
    none = (None, mpmath.mpf(0))
    if start[0] is None:
        return Law("still", to if jumps and to[0] is not None else none, none)
    if to[0] is None or to[0] < start[0]:
        return Law("fall", start, to)
    if to[0] > start[0]:
        return Law("rise", jump(start, to) if jumps else start, to)
    return Law("still", start, none)


class Envelope:
    """keptone-engine's envelope (MATH §4), in exact arithmetic, with the bounds of the errors
    of keptone-engine's: its stage, its piece of law from an instant at a step a sample, the
    progress at which its segment ends, its segment's offset and what its wait has counted."""

    def __init__(self, parameters: dict, settings: dict, key: float, offset: float) -> None:
        """An envelope resting at L4 at an offset, from sample 0."""
        self.parameters, self.settings, self.key = parameters, settings, key
        self.offset = exact(offset)
        self.stage = 3
        self.resting = True
        self.held = False
        to = target(parameters["levels"][3], self.offset)
        self.law = Law("still", to, (None, mpmath.mpf(0)))
        self.start = (mpmath.mpf(0), mpmath.mpf(0))
        self.step = (mpmath.mpf(0), mpmath.mpf(0))
        self.end_progress = None
        self.counted = (mpmath.mpf(0), mpmath.mpf(0))

    def progress(self, at: mpmath.mpf) -> tuple[mpmath.mpf, mpmath.mpf]:
        """The progress at an instant, with its error: the step's relative error and 2u more
        of the samples and the product, u/2 of a step, and a step for each sample of the
        start's error."""
        u = ulps()
        samples = max(at - self.start[0], 0)
        step, step_error = self.step
        value = step * samples
        return value, value * (step_error + 2 * u) + step * (u / 2 + self.start[1])

    def level(self, at: mpmath.mpf, offset: mpmath.mpf) -> Level:
        """T at an instant at an offset, with its error."""
        value, error = self.law.level(*self.progress(at))
        if value is None:
            return None, mpmath.mpf(0)
        if offset == self.offset:
            return value, error
        moved = value + (offset - self.offset)
        return moved, error + ulps() * abs(moved)

    def end(self) -> tuple[mpmath.mpf, mpmath.mpf] | None:
        """The instant at which the segment ends, with its error, or None."""
        if self.end_progress is None:
            return None
        progress, progress_error = self.end_progress
        if progress <= 0:
            return self.start
        step, step_error = self.step
        if step == 0:
            return None
        instant = self.start[0] + progress / step
        error = self.start[1] + (progress_error + progress * (step_error + 2 * ulps())) / step
        return instant, error + ulps() * (instant - mpmath.floor(self.start[0]) + 1)

    def next(self, offset: mpmath.mpf) -> None:
        """Ends the segment at its end and starts what follows."""
        end = self.end()
        if end is None:
            return
        if self.stage < 2:
            level = self.law.level(*self.end_progress)
            if level[0] is not None and offset != self.offset:
                moved = level[0] + (offset - self.offset)
                level = (moved, level[1] + ulps() * abs(moved))
            self.begin(self.stage + 1, end, level, offset)
        elif self.stage == 2:
            self.held, self.end_progress = True, None
        else:
            self.resting, self.end_progress = True, None

    def key_on(self, at: mpmath.mpf, offset: mpmath.mpf) -> None:
        """A key-on: segment 0 from the level there."""
        self.begin(0, (at, mpmath.mpf(0)), self.level(at, offset), offset)

    def key_off(self, at: mpmath.mpf, offset: mpmath.mpf) -> None:
        """A key-off: segment 3 from the level there, but where segment 3 has begun."""
        if self.stage == 3:
            return
        self.begin(3, (at, mpmath.mpf(0)), self.level(at, offset), offset)

    def change(self, at: mpmath.mpf, parameters: dict, key: float) -> None:
        """A change of the parameters or the key: the law re-anchored, the wait keeping its count."""
        progress = self.progress(at)
        level = self.law.level(*progress)
        if not (self.held or self.resting):
            self.counted = (self.counted[0] + progress[0], self.counted[1] + progress[1] + ulps() * self.counted[0])
        self.parameters, self.key = parameters, key
        self.anchor((at, mpmath.mpf(0)), level)

    def begin(self, segment: int, at: tuple[mpmath.mpf, mpmath.mpf], start: Level, offset: mpmath.mpf) -> None:
        """Starts a segment at an instant from a level, at an offset."""
        self.stage, self.held, self.resting = segment, False, False
        self.offset = offset
        self.counted = (mpmath.mpf(0), mpmath.mpf(0))
        self.anchor(at, start)

    def anchor(self, at: tuple[mpmath.mpf, mpmath.mpf], start: Level) -> None:
        """Anchors the law at an instant from a level, and the segment's end."""
        u = ulps()
        index = 2 if self.held else self.stage
        to = target(self.parameters["levels"][index], self.offset)
        rate = self.parameters["rates"][index]
        scaling = self.parameters["scaling"]
        q, largest, slack = rate_law(rate, scaling, self.key)
        on_scales = 0 <= rate <= 99 and 0 <= scaling <= 7
        rate_error = (12 if on_scales else 41) * u * largest + exact(slack)
        rho = speed(q, self.settings["clock"], self.settings["law"])
        speed_error = (mpmath.mpf("2.86") if self.settings["law"] else mpmath.mpf("2.5")) * u + rate_error / 4
        self.step = (rho / exact(self.settings["rate"]), speed_error + u)
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
        self.start = at
        if self.held or self.resting:
            self.end_progress = None
            return
        rest = waited[0] - self.counted[0]
        rest_error = waited[1] + self.counted[1] + u * abs(rest)
        self.end_progress = (max(moved[0], rest), max(moved[1], rest_error))


def logarithmic(randoms: Iterator[int], octaves: int) -> int:
    """A whole count of samples from 1 up to 2^octaves, of each octave as likely, uniform within it."""
    octave = next(randoms) % octaves
    return int(2**octave * (1 + uniform(randoms, 0.0, 1.0)))


def pick(randoms: Iterator[int], choices: list[Callable[[], float]]) -> float:
    """One of the ways of drawing an input, each as likely."""
    return choices[next(randoms) % len(choices)]()


def whole(randoms: Iterator[int], low: int, high: int) -> float:
    """A whole input of [low, high]."""
    return float(low + next(randoms) % (high - low + 1))


def parameters_of(randoms: Iterator[int]) -> dict:
    """An operator's envelope parameters: whole, between the steps, and beyond the scales."""
    rates = [
        pick(
            randoms,
            [
                lambda: whole(randoms, 0, 99),
                lambda: whole(randoms, 0, 99),
                lambda: uniform(randoms, 0.0, 99.0),
                lambda: uniform(randoms, -10.0, 120.0),
            ],
        )
        for _ in range(4)
    ]
    levels = [
        pick(
            randoms,
            [
                lambda: whole(randoms, 0, 99),
                lambda: whole(randoms, 0, 99),
                lambda: uniform(randoms, 0.0, 99.0),
                lambda: uniform(randoms, 0.0, 1.0),
                lambda: uniform(randoms, -5.0, 110.0),
            ],
        )
        for _ in range(4)
    ]
    scaling = pick(
        randoms, [lambda: whole(randoms, 0, 7), lambda: whole(randoms, 0, 7), lambda: uniform(randoms, 0.0, 9.0)]
    )
    return {"rates": rates, "levels": levels, "scaling": scaling}


def key_of(randoms: Iterator[int]) -> float:
    """A key of the rates: whole, or between keys and beyond the keyboard."""
    return pick(
        randoms,
        [lambda: whole(randoms, 0, 127), lambda: whole(randoms, 0, 127), lambda: uniform(randoms, -10.0, 140.0)],
    )


def offset_of(randoms: Iterator[int]) -> float:
    """An offset: a whole one of the DX7's, a multiple of 16, or any between and beyond them."""
    return pick(randoms, [lambda: 16.0 * whole(randoms, 0, 265), lambda: uniform(randoms, -1500.0, 4500.0)])


def drawn(randoms: Iterator[int]) -> tuple[list[float], dict, dict, list[Line]]:
    """A scenario's head, parameters and settings, and its events in their order."""
    settings = {
        "rate": pick(
            randoms,
            [lambda: 44100.0, lambda: 48000.0, lambda: 96000.0, lambda: 192000.0, lambda: 8000.0, lambda: 88200.0],
        ),
        "clock": pick(randoms, [lambda: 49096.0, lambda: 49096.0, lambda: 44100.0]),
        "law": int(next(randoms) % 4 == 0),
        "jump": int(next(randoms) % 4 != 0),
        "waits": int(next(randoms) % 4 != 0),
    }
    parameters = parameters_of(randoms)
    key, offset = key_of(randoms), offset_of(randoms)
    events: list[Line] = [["key_on", 0.0]]
    released = logarithmic(randoms, 21)
    events.append(["key_off", float(released)])
    if next(randoms) % 3 == 0:
        events.append(["key_on", float(released + logarithmic(randoms, 20))])
    if next(randoms) % 2 == 0:
        changed = parameters_of(randoms)
        merged = {name: changed[name] if next(randoms) % 2 else parameters[name] for name in parameters}
        events.append(
            [
                "change",
                float(logarithmic(randoms, 21)),
                *merged["rates"],
                *merged["levels"],
                merged["scaling"],
                key_of(randoms),
            ]
        )
    if next(randoms) % 3 == 0:
        events.append(["offset", float(logarithmic(randoms, 21)), offset_of(randoms)])
    events.sort(key=lambda event: event[1])
    head = [
        settings["rate"],
        settings["clock"],
        float(settings["law"]),
        float(settings["jump"]),
        float(settings["waits"]),
    ]
    head += [*parameters["rates"], *parameters["levels"], parameters["scaling"], key, offset]
    return head, parameters, settings, events


def run(scenario: tuple[list[float], dict, dict, list[Line]], evaluations: list[int]) -> tuple[list[Line], list[tuple]]:
    """A scenario's lines, and its ends as (instant, error, instantaneous), its levels taken at
    the evaluations' samples after the ends and the events there."""
    head, parameters, settings, events = scenario
    key, offset = head[-2], exact(head[-1])
    envelope = Envelope(parameters, settings, key, head[-1])
    lines: list[Line] = [["scenario", *head]]
    ends: list[tuple] = []
    points = sorted({int(event[1]) for event in events} | set(evaluations))

    def end_up_to(sample: int) -> None:
        while (end := envelope.end()) is not None and end[0] <= sample:
            lines.append(["end", float(envelope.stage), *double_double(end[0]), float(end[1])])
            ends.append((end[0], end[1], envelope.end_progress[0] <= 0))
            envelope.next(offset)

    for sample in points:
        end_up_to(sample)
        for event in (event for event in events if int(event[1]) == sample):
            lines.append(event)
            at = mpmath.mpf(sample)
            if event[0] == "key_on":
                envelope.key_on(at, offset)
            elif event[0] == "key_off":
                envelope.key_off(at, offset)
            elif event[0] == "offset":
                offset = exact(event[2])
            else:
                changed = {"rates": event[2:6], "levels": event[6:10], "scaling": event[10]}
                envelope.change(at, changed, event[11])
        end_up_to(sample)
        if sample in evaluations:
            value, error = envelope.level(mpmath.mpf(sample), offset)
            lines.append(
                ["silent", float(sample)]
                if value is None
                else ["level", float(sample), *double_double(value), float(error)]
            )
    return lines, ends


def designed() -> list[tuple[list[float], dict, dict, list[Line]]]:
    """Scenarios that each pass through a part of the laws that random ones seldom reach: silence,
    segments that do not move and their waits, moves of less than a step, a change during a wait,
    a key-off during one, a retrigger in the release, a change of the offset, levels between 0 and
    1, a rise below the floor, and each setting's other choice."""
    made = []

    def add(settings: tuple, rates: list, levels: list, rest: tuple, events: list[Line]) -> None:
        rate, clock, law, jumps, waits = settings
        scaling, key, offset = rest
        head = [rate, clock, float(law), float(jumps), float(waits), *rates, *levels, scaling, key, offset]
        parameters = {"rates": rates, "levels": levels, "scaling": scaling}
        named = {"rate": rate, "clock": clock, "law": law, "jump": jumps, "waits": waits}
        made.append((head, parameters, named, events))

    dx7 = (48000.0, 49096.0, 0, 1, 1)
    full = (0.0, 60.0, 4064.0)
    still = [99.0, 40.0, 40.0, 60.0]
    add(dx7, [50.0] * 4, [0.0] * 4, full, [["key_on", 0.0], ["key_off", 3000.0]])
    add(dx7, still, [99.0, 99.0, 99.0, 0.0], full, [["key_on", 0.0], ["key_off", 120000.0]])
    add(dx7, still, [98.0, 97.99, 97.98, 0.0], full, [["key_on", 0.0], ["key_off", 90000.0]])
    add(
        dx7,
        still,
        [99.0, 99.0, 99.0, 0.0],
        full,
        [
            ["key_on", 0.0],
            ["change", 9000.0, 99.0, 70.0, 20.0, 60.0, 99.0, 99.0, 99.0, 0.0, 0.0, 60.0],
            ["key_off", 200000.0],
        ],
    )
    add(dx7, still, [99.0, 99.0, 99.0, 0.0], full, [["key_on", 0.0], ["key_off", 7000.0], ["key_off", 8000.0]])
    add(dx7, still, [99.0] * 4, full, [["key_on", 0.0], ["key_off", 60000.0], ["key_off", 61000.0]])
    add(
        dx7,
        [80.0, 50.0, 40.0, 30.0],
        [99.0, 80.0, 70.0, 0.0],
        full,
        [["key_on", 0.0], ["key_off", 30000.0], ["key_on", 60000.0], ["key_off", 200000.0]],
    )
    add(
        dx7,
        [70.0, 50.0, 40.0, 60.0],
        [99.0, 80.0, 70.0, 10.0],
        full,
        [["key_on", 0.0], ["offset", 20000.0, 3200.5], ["key_off", 80000.0]],
    )
    add(dx7, [70.0, 60.0, 50.0, 60.0], [0.5, 0.25, 0.75, 0.125], full, [["key_on", 0.0], ["key_off", 50000.0]])
    add(
        dx7,
        [99.0, 99.0, 99.0, 99.0],
        [99.0, 99.0, 99.0, 0.0],
        (0.0, 60.0, 3600.0),
        [
            ["key_on", 0.0],
            ["key_off", 200.0],
            ["change", 30000.0, 99.0, 99.0, 99.0, 99.0, 5.0, 5.0, 5.0, 0.0, 0.0, 60.0],
            ["key_on", 40000.0],
            ["key_on", 50000.0],
            ["change", 50000.0, 99.0, 99.0, 99.0, 99.0, 5.0, 5.0, 5.0, 0.0, 0.0, 60.0],
            ["key_off", 90000.0],
        ],
    )
    add(
        (96000.0, 44100.0, 1, 0, 0),
        [60.0, 40.0, 30.0, 50.0],
        [99.0, 70.0, 60.0, 0.0],
        (3.0, 90.5, 3000.0),
        [["key_on", 0.0], ["key_off", 150000.0]],
    )
    add(
        (44100.0, 49096.0, 0, 0, 1),
        [75.0, 99.0, 30.0, 45.0],
        [60.0, 99.0, 99.0, 40.0],
        (7.0, 100.0, 2500.0),
        [["key_on", 0.0], ["key_off", 100000.0]],
    )
    return made


def scenarios(randoms: Iterator[int]) -> Iterator[Line]:
    """The scenarios' lines, the designed ones first: each run once for its ends, its evaluations
    chosen between them, and run again; a random one drawn again where an end lies near an event."""
    made = 0
    queue = designed()
    while made < SCENARIOS + len(designed()):
        scenario = queue.pop(0) if queue else drawn(randoms)
        samples = sorted({int(event[1]) for event in scenario[3]})
        horizon = samples[-1] + 2**20
        _, ends = run(scenario, [horizon])
        margins = [
            (instant, max(mpmath.mpf(10) ** -6, 10 * error)) for instant, error, instant_end in ends if not instant_end
        ]
        if any(abs(instant - sample) < margin for instant, margin in margins for sample in samples):
            if made < len(designed()):
                raise ValueError(f"a designed scenario's end lies near an event: {scenario[0]}")
            continue
        points = sorted(
            [*(mpmath.mpf(sample) for sample in samples), *(instant for instant, _ in margins), mpmath.mpf(horizon)]
        )
        evaluations = set(samples)
        for low, high in itertools.pairwise(points):
            inside = [int(mpmath.floor(low)) + 1, int(mpmath.floor((low + high) / 2)), int(mpmath.ceil(high)) - 1]
            evaluations |= {n for n in inside if low < n < high}
        evaluations = {n for n in evaluations if all(abs(instant - n) >= margin for instant, margin in margins)}
        lines, _ = run(scenario, sorted(evaluations))
        yield from lines
        made += 1


def main() -> None:
    """Writes the file."""
    mpmath.mp.prec = 400
    out = [HEADER.rstrip("\n")]
    for name, *numbers in scenarios(splitmix64(SEED)):
        out.append(" ".join([str(name), *(f"{to_bits(x):016x}" for x in numbers)]))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
