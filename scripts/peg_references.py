#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of the pitch envelope for keptone-engine's tests.

    python3 scripts/peg_references.py

Computes the laws of docs/MATH.md §5 exactly at binary64 inputs, with fractions
and, for the rates below 0, with mpmath at 400 bits: the level pt(L)/32 of an EG
level, linear between the steps and along the table's end steps beyond them;
and the speed pr(R) f_tick/8192, linear between the steps, along the last step
above 99 and 2^R below 0, f_tick = 9.4265 MHz/(8 3165). And it runs the pitch
envelope as keptone-engine runs it, in scenarios of a voice's parameters, an
engine rate and events at whole samples drawn from SplitMix64 with a fixed
seed: key-ons, which start it at L4, key-offs and changes of its parameters,
and the holds at L3 and L4. It records the instants at which segments end and
the levels at whole samples between them, none nearer an end than 10^-6 of a
sample or ten times the bound of the end's error, drawing a scenario again
where an end lies that near an event; and beside each value a bound on
keptone-engine's error, first order in u = 2^-53, which it follows from the
errors that keptone-engine's functions document through every piece of law.
It writes crates/keptone-engine/tests/data/peg.txt, every number as the 16
hexadecimal digits of its binary64, so that the file is the same every time.
"""

import itertools
import math
import re
from collections.abc import Callable, Iterator
from fractions import Fraction
from pathlib import Path

import mpmath

from binary64 import binades, neighbours, splitmix64, to_bits, uniform

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "peg.txt"
MATH = ROOT / "docs" / "MATH.md"
SEED = 0x7069746368
SCENARIOS = 120
RANDOM = 600

# The rate of the firmware's timer over 8192, in octaves a second for a step of pr.
OCTAVES_PER_RATE = Fraction(9426500, 8 * 3165 * 8192)

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/peg_references.py with mpmath; do not edit.
# Every number the binary64 bits in hexadecimal:
#   level L, then the level as a double-double | speed R, then the speed
#   scenario rate R1 R2 R3 R4 L1 L2 L3 L4, then its lines in the order of its
#     time: key_on sample | key_off sample | change sample R1..R4 L1..L4 |
#     end segment, the instant in samples as a double-double and the bound of
#     its error | sample n, the level there as a double-double and its bound
"""

type Line = list[str | float]
type Exact = Fraction | mpmath.mpf
type Value = tuple[mpmath.mpf, mpmath.mpf]


def tables() -> tuple[list[int], list[int]]:
    """pt and pr as MATH §5 lists them."""
    line = next(line for line in MATH.read_text(encoding="utf-8").split("\n") if line.startswith("- pt and pr are"))
    levels = [int(x.replace("\N{MINUS SIGN}", "-")) for x in re.search(r"pt = \(([^)]*)\)", line).group(1).split(", ")]
    rates = [int(x) for x in re.search(r"pr = \(([^)]*)\)", line).group(1).split(", ")]
    return levels, rates


LEVELS, RATES = tables()


def between(x: Fraction, at: Callable[[int], Exact]) -> Exact:
    """A law at whole inputs, linear between them."""
    whole = math.floor(x)
    part = x - whole
    return at(whole) if part == 0 else at(whole) + part * (at(whole + 1) - at(whole))


def level(value: float) -> Fraction:
    """pt(L)/32: the table, linear between its steps and along its end steps beyond them."""
    x = Fraction(value)
    if x < 0:
        return (-128 + 12 * x) / 32
    if x > 99:
        return (127 + 12 * (x - 99)) / 32
    return Fraction(between(x, lambda step: LEVELS[step])) / 32


def speed(value: float) -> Exact:
    """pr(R) f_tick/8192: the table, linear between its steps, along its last step above 99
    and 2^R below 0."""
    x = Fraction(value)
    if x < 0:
        return (
            mpmath.power(2, mpmath.mpf(value)) * mpmath.mpf(OCTAVES_PER_RATE.numerator) / OCTAVES_PER_RATE.denominator
        )
    rate = 255 + (x - 99) if x > 99 else Fraction(between(x, lambda step: RATES[step]))
    return rate * OCTAVES_PER_RATE


def exact(value: Exact) -> mpmath.mpf:
    """A rational or an mpf as an mpf."""
    if isinstance(value, mpmath.mpf):
        return value
    value = Fraction(value)
    return mpmath.mpf(value.numerator) / value.denominator


def ulps() -> mpmath.mpf:
    """u = 2^-53."""
    return mpmath.mpf(2) ** -53


def level_error(value: float) -> mpmath.mpf:
    """The error that keptone-engine's level documents: u|pt|/32, and 12u|L - 99|/32 more above 99."""
    u = ulps()
    error = u * abs(exact(level(value)))
    return error + (12 * u * abs(exact(Fraction(value) - 99)) / 32 if value > 99 else 0)


def speed_error(value: float) -> mpmath.mpf:
    """The relative error that keptone-engine's speed documents, and the absolute one above 99."""
    u = ulps()
    rho = exact(speed(value))
    if value < 0:
        return mpmath.mpf("3.86") * u
    relative = 3 * u
    if value > 99 and rho > 0:
        relative += u * abs(exact(Fraction(value) - 99)) * exact(OCTAVES_PER_RATE) / rho
    return relative


class Envelope:
    """keptone-engine's pitch envelope, in exact arithmetic, with the bounds of the errors of
    keptone-engine's: its stage, its piece from an instant at a step a sample, from a level
    toward a target, and the progress at which its segment ends."""

    def __init__(self, parameters: dict, rate: float) -> None:
        """A pitch envelope at L4's level from sample 0."""
        self.parameters, self.rate = parameters, rate
        self.stage, self.held, self.resting = 3, False, True
        rest = exact(level(parameters["levels"][3]))
        self.start = (mpmath.mpf(0), mpmath.mpf(0))
        self.step = (mpmath.mpf(0), mpmath.mpf(0))
        error = level_error(parameters["levels"][3])
        self.source = (rest, error)
        self.to = (rest, error)
        self.end_progress = None

    def progress(self, at: mpmath.mpf) -> Value:
        """The progress at an instant, with its error."""
        u = ulps()
        samples = max(at - self.start[0], 0)
        step, step_error = self.step
        value = step * samples
        return value, value * (step_error + 2 * u) + step * (u / 2 + self.start[1])

    def level(self, at: mpmath.mpf) -> Value:
        """The level at an instant, with its error."""
        progress, progress_error = self.progress(at)
        source, to = self.source, self.to
        rising = to[0] >= source[0]
        moved = source[0] + progress if rising else source[0] - progress
        error = source[1] + progress_error + ulps() * abs(moved)
        if (rising and moved >= to[0]) or (not rising and moved <= to[0]):
            return to[0], max(error, to[1])
        return moved, error

    def end(self) -> Value | None:
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

    def next(self) -> None:
        """Ends the segment at its end and starts what follows."""
        end = self.end()
        if end is None:
            return
        if self.stage < 2:
            self.begin(self.stage + 1, end, self.to)
        elif self.stage == 2:
            self.held, self.end_progress = True, None
        else:
            self.resting, self.end_progress = True, None

    def key_on(self, at: mpmath.mpf) -> None:
        """A key-on: segment 0 from L4's level."""
        levels = self.parameters["levels"]
        self.begin(0, (at, mpmath.mpf(0)), (exact(level(levels[3])), level_error(levels[3])))

    def key_off(self, at: mpmath.mpf) -> None:
        """A key-off: segment 3 from the level there, but where segment 3 has begun."""
        if self.stage == 3:
            return
        self.begin(3, (at, mpmath.mpf(0)), self.level(at))

    def change(self, at: mpmath.mpf, parameters: dict) -> None:
        """A change of the parameters: the law re-anchored from the level there."""
        source = self.level(at)
        self.parameters = parameters
        self.anchor((at, mpmath.mpf(0)), source)

    def begin(self, segment: int, at: Value, source: Value) -> None:
        """Starts a segment at an instant from a level."""
        self.stage, self.held, self.resting = segment, False, False
        self.anchor(at, source)

    def anchor(self, at: Value, source: Value) -> None:
        """Anchors the law at an instant from a level, and the segment's end."""
        u = ulps()
        index = 2 if self.held else self.stage
        target = self.parameters["levels"][index]
        rate = self.parameters["rates"][index]
        self.to = (exact(level(target)), level_error(target))
        self.step = (exact(speed(rate)) / exact(Fraction(self.rate)), speed_error(rate) + u)
        self.source, self.start = source, at
        if self.held or self.resting:
            self.end_progress = None
            return
        distance = abs(self.to[0] - source[0])
        self.end_progress = (distance, source[1] + self.to[1] + u * distance)


def draw(randoms: Iterator[int], low: float, high: float) -> float:
    """An input: a real of [low, high] or one of any binade up to binary64's largest."""
    return uniform(randoms, low, high) if next(randoms) % 3 else binades(randoms, -60, 1023)


def whole(randoms: Iterator[int], low: int, high: int) -> float:
    """A whole input of [low, high]."""
    return float(low + next(randoms) % (high - low + 1))


def pick(randoms: Iterator[int], choices: list[Callable[[], float]]) -> float:
    """One of the ways of drawing an input, each as likely."""
    return choices[next(randoms) % len(choices)]()


def logarithmic(randoms: Iterator[int], octaves: int) -> int:
    """A whole count of samples from 1 up to 2^octaves, of each octave as likely."""
    return int(2 ** (next(randoms) % octaves) * (1 + uniform(randoms, 0.0, 1.0)))


def laws(randoms: Iterator[int]) -> Iterator[Line]:
    """The level and the speed at the steps, their halves and neighbours, and at draws on the
    scales and beyond them."""
    inputs = [x for step in range(-3, 103) for x in (*neighbours(float(step), 1), step + 0.5)]
    inputs += [-1e300, 1e300, *(draw(randoms, -20.0, 120.0) for _ in range(RANDOM))]
    for x in inputs:
        yield ["level", x, *double_double(level(x))]
    for x in [*inputs, -1074.0, -1100.0]:
        yield ["speed", x, *double_double(speed(x))]


def double_double(value: Exact) -> list[float]:
    """value as hi + lo, hi the nearest binary64 and lo the nearest to the rest."""
    hi = float(value)
    rest = value - (Fraction(hi) if isinstance(value, Fraction) else mpmath.mpf(hi))
    return [hi, float(rest)]


def parameters_of(randoms: Iterator[int]) -> dict:
    """A voice's pitch envelope parameters: whole, between the steps, and beyond the scales."""

    def one() -> float:
        return pick(
            randoms,
            [
                lambda: whole(randoms, 0, 99),
                lambda: whole(randoms, 0, 99),
                lambda: uniform(randoms, 0.0, 99.0),
                lambda: uniform(randoms, -10.0, 110.0),
            ],
        )

    return {"rates": [one() for _ in range(4)], "levels": [one() for _ in range(4)]}


def drawn(randoms: Iterator[int]) -> tuple[list[float], dict, float, list[Line]]:
    """A scenario's head, parameters and engine rate, and its events in their order."""
    rate = pick(randoms, [lambda: 44100.0, lambda: 48000.0, lambda: 96000.0, lambda: 192000.0, lambda: 8000.0])
    parameters = parameters_of(randoms)
    events: list[Line] = [["key_on", 0.0]]
    released = logarithmic(randoms, 20)
    events.append(["key_off", float(released)])
    if next(randoms) % 3 == 0:
        events.append(["key_on", float(released + logarithmic(randoms, 19))])
    if next(randoms) % 2 == 0:
        changed = parameters_of(randoms)
        merged = {name: changed[name] if next(randoms) % 2 else parameters[name] for name in parameters}
        events.append(["change", float(logarithmic(randoms, 20)), *merged["rates"], *merged["levels"]])
    events.sort(key=lambda event: event[1])
    return [rate, *parameters["rates"], *parameters["levels"]], parameters, rate, events


def designed() -> list[tuple[list[float], dict, float, list[Line]]]:
    """Scenarios that pass through what random ones seldom reach: segments that do not move,
    which end at once, a key-off and a second one in the release, a retrigger mid-segment, a
    change in the hold, and rates beyond the scale."""
    made = []
    for rates, levels, events in [
        ([50.0] * 4, [50.0] * 4, [["key_on", 0.0], ["key_off", 1000.0]]),
        (
            [99.0, 70.0, 40.0, 30.0],
            [99.0, 0.0, 70.0, 50.0],
            [["key_on", 0.0], ["key_off", 30000.0], ["key_off", 30500.0]],
        ),
        (
            [60.0, 50.0, 40.0, 50.0],
            [80.0, 20.0, 60.0, 30.0],
            [["key_on", 0.0], ["key_on", 2000.0], ["key_off", 90000.0]],
        ),
        (
            [99.0, 99.0, 99.0, 60.0],
            [70.0, 60.0, 55.0, 50.0],
            [
                ["key_on", 0.0],
                ["change", 20000.0, 99.0, 99.0, 30.0, 60.0, 70.0, 60.0, 90.0, 50.0],
                ["key_off", 120000.0],
            ],
        ),
        ([-3.5, 120.0, -20.0, 99.5], [10.0, 90.0, 40.0, 60.0], [["key_on", 0.0], ["key_off", 400000.0]]),
    ]:
        parameters = {"rates": rates, "levels": levels}
        made.append(([48000.0, *rates, *levels], parameters, 48000.0, events))
    return made


def run(
    scenario: tuple[list[float], dict, float, list[Line]], evaluations: list[int]
) -> tuple[list[Line], list[tuple]]:
    """A scenario's lines, and its ends as (instant, error, at its start)."""
    head, parameters, rate, events = scenario
    envelope = Envelope(parameters, rate)
    lines: list[Line] = [["scenario", *head]]
    ends: list[tuple] = []
    points = sorted({int(event[1]) for event in events} | set(evaluations))

    def end_up_to(sample: int) -> None:
        while (end := envelope.end()) is not None and end[0] <= sample:
            lines.append(["end", float(envelope.stage), *double_double(end[0]), float(end[1])])
            ends.append((end[0], end[1], envelope.end_progress[0] <= 0))
            envelope.next()

    for sample in points:
        end_up_to(sample)
        for event in (event for event in events if int(event[1]) == sample):
            lines.append(event)
            at = mpmath.mpf(sample)
            if event[0] == "key_on":
                envelope.key_on(at)
            elif event[0] == "key_off":
                envelope.key_off(at)
            else:
                envelope.change(at, {"rates": event[2:6], "levels": event[6:10]})
        end_up_to(sample)
        if sample in evaluations:
            value, error = envelope.level(mpmath.mpf(sample))
            lines.append(["sample", float(sample), *double_double(value), float(error)])
    return lines, ends


def scenarios(randoms: Iterator[int]) -> Iterator[Line]:
    """The scenarios' lines, the designed ones first: each run once for its ends, its
    evaluations chosen between them, and run again; a random one drawn again where an end lies
    near an event."""
    queue = designed()
    count = len(queue)
    made = 0
    while made < SCENARIOS + count:
        scenario = queue.pop(0) if queue else drawn(randoms)
        samples = sorted({int(event[1]) for event in scenario[3]})
        horizon = samples[-1] + 2**20
        _, ends = run(scenario, [horizon])
        margins = [
            (instant, max(mpmath.mpf(10) ** -6, 10 * error)) for instant, error, at_start in ends if not at_start
        ]
        if any(abs(instant - sample) < margin for instant, margin in margins for sample in samples):
            if made < count:
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
    randoms = splitmix64(SEED)
    out = [HEADER.rstrip("\n")]
    for name, *numbers in itertools.chain(laws(randoms), scenarios(randoms)):
        out.append(" ".join([str(name), *(f"{to_bits(x):016x}" for x in numbers)]))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
