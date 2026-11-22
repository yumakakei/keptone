#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of the portamento for keptone-engine's tests.

    python3 scripts/portamento_references.py

Computes the laws of docs/MATH.md §7.3 exactly at binary64 inputs, with fractions
and mpmath at 400 bits: the portamento's speed kappa from its TIME, the pitch
envelope's speed of the rate 99 - TIME (§5, whose tables scripts/peg_references.py
reads from MATH) and kappa(1)/TIME below 1; and it runs glides as keptone-engine
runs them, in scenarios of an engine rate, a unit, octaves or keys, an offset from
the target, a TIME and events at whole samples drawn from SplitMix64 with a fixed
seed: changes of TIME and of the target, and, in keys, the glissando turned on and
off. A glide's distance falls by dd/dt = -kappa(1 + 4d), in closed form from its
anchor, to its arrival; a glissando's glide runs at 36 kappa keys a second, and the
key heard steps a whole key toward it each time it lies a whole key from the key
heard, and takes the target when the glide reaches it; a change keeps the
glissando's glide and its key heard. It records the instants of arrivals and of
the steps of the key heard, the offsets at whole samples between them and the
glissando's glide, none nearer an instant than ten times the bound of its error,
drawing a scenario again where an event lies that near one or a decision of the
glissando that near its other side; and beside each value a bound on
keptone-engine's error, first order in u = 2^-53, which it follows from the errors
that keptone-engine's functions document through every piece of law. It writes
crates/keptone-engine/tests/data/portamento.txt, every number as the 16
hexadecimal digits of its binary64, so that the file is the same every time.
"""

import itertools
import sys
from collections.abc import Callable, Iterator
from fractions import Fraction
from pathlib import Path

import mpmath

from binary64 import binades, neighbours, splitmix64, to_bits, uniform
from peg_references import OCTAVES_PER_RATE, RATES, between
from peg_references import speed_error as rate_speed_error

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "portamento.txt"
SEED = 0x706F7274616D656E
SCENARIOS = 160
RANDOM = 500
QUARTERS = (Fraction(1, 4), Fraction(3))
LARGEST = Fraction(sys.float_info.max)

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/portamento_references.py with mpmath; do not edit.
# Every number the binary64 bits in hexadecimal:
#   speed TIME, then kappa as a double-double and the bound of its error |
#   nospeed TIME
#   scenario rate unit glissando offset TIME (unit 0 octaves, 1 keys), then its
#     lines in the order of its time: time sample TIME | target sample shift |
#     switch sample (the glissando turned on or off) | end, the instant in
#     samples as a double-double and the bound of its error | sample n, the
#     offset as a double-double and its bound | glide n, the glissando's glide
#     as a double-double and its bound
"""

type Line = list[str | float]
type Exact = Fraction | mpmath.mpf
type Value = tuple[mpmath.mpf, mpmath.mpf]


def ulps() -> mpmath.mpf:
    """u = 2^-53."""
    return mpmath.mpf(2) ** -53


def exact(value: Exact) -> mpmath.mpf:
    """A rational or an mpf as an mpf."""
    if isinstance(value, mpmath.mpf):
        return value
    value = Fraction(value)
    return mpmath.mpf(value.numerator) / value.denominator


def double_double(value: Exact) -> list[float]:
    """value as hi + lo, hi the nearest binary64 and lo the nearest to the rest."""
    hi = float(value)
    rest = value - (Fraction(hi) if isinstance(value, Fraction) else mpmath.mpf(hi))
    return [hi, float(rest)]


def rounding(value: mpmath.mpf, error: mpmath.mpf) -> mpmath.mpf:
    """The bound of the rounding of a sum or a product whose exact value is this: none where
    its operands are exact and it is a binary64, u|value| otherwise."""
    if error == 0 and mpmath.mpf(float(value)) == value:
        return mpmath.mpf(0)
    return ulps() * abs(value)


def rate_speed(rate: Fraction) -> mpmath.mpf:
    """The pitch envelope's speed of MATH §5 at an exact rate: pr(R) F/8192, linear between
    the steps, along the last step above 99 and 2^R below 0."""
    if rate < 0:
        return mpmath.power(2, exact(rate)) * exact(OCTAVES_PER_RATE)
    steps = 255 + (rate - 99) if rate > 99 else Fraction(between(rate, lambda step: RATES[step]))
    return exact(steps * OCTAVES_PER_RATE)


def kappa(time: float) -> mpmath.mpf | None:
    """kappa of MATH §7.3 at a TIME, or none at or below 0; binary64's largest finite number
    where it lies beyond it."""
    t = Fraction(time)
    if t <= 0:
        return None
    if t < 1:
        return min(rate_speed(Fraction(98)) / exact(t), exact(LARGEST))
    return rate_speed(99 - t)


def kappa_error(time: float) -> mpmath.mpf:
    """The bound of keptone-engine's error of kappa that speed documents: at 99 - TIME as it
    rounds, within the pitch envelope's speed's bound there; below 1, 4u."""
    if time < 1:
        return 4 * ulps() * kappa(time)
    rounded = 99.0 - time
    taken = rate_speed(Fraction(rounded))
    return rate_speed_error(rounded) * taken + abs(taken - kappa(time))


def draw(randoms: Iterator[int], low: float, high: float) -> float:
    """An input: a real of [low, high] or one of any binade up to 2^60."""
    return uniform(randoms, low, high) if next(randoms) % 4 else binades(randoms, -30, 60)


def whole(randoms: Iterator[int], low: int, high: int) -> float:
    """A whole input of [low, high]."""
    return float(low + next(randoms) % (high - low + 1))


def pick(randoms: Iterator[int], choices: list[Callable[[], float]]) -> float:
    """One of the ways of drawing an input, each as likely."""
    return choices[next(randoms) % len(choices)]()


def logarithmic(randoms: Iterator[int], octaves: int) -> int:
    """A whole count of samples from 1 up to 2^octaves, of each octave as likely."""
    return int(2 ** (next(randoms) % octaves) * (1 + uniform(randoms, 0.0, 1.0)))


def speeds(randoms: Iterator[int]) -> Iterator[Line]:
    """kappa at the steps of TIME, between them, from 0 to 1, beyond the scale and drawn."""
    times = [x for t in range(-2, 103) for x in (float(t), t + 0.5)] + [*neighbours(0.0, 2), *neighbours(1.0, 2)]
    times += [0.25, 1.3, 49.5, 198.0, 198.5, 1e3, 1100.0, *(draw(randoms, -5.0, 120.0) for _ in range(RANDOM))]
    for time in times:
        value = kappa(time)
        if value is None:
            yield ["nospeed", time]
        else:
            yield ["speed", time, *double_double(value), float(kappa_error(time))]


class Glide:
    """keptone-engine's glide without glissando in exact arithmetic, with the bounds of its
    errors: its anchor, its offset there and its rate a sample, 4 kappa/fe."""

    def __init__(self, at: int, offset: Value, speed: Value, quarter: Fraction, rate: float) -> None:
        """A glide from a sample, from an offset, at a speed kappa with its error."""
        self.quarter, self.sample_rate = quarter, rate
        self.set_rate(speed)
        self.anchor(at, offset)

    def set_rate(self, speed: Value) -> None:
        """4 kappa/fe with its relative error."""
        self.rate = 4 * speed[0] / exact(Fraction(self.sample_rate))
        self.rate_error = speed[1] / speed[0] + ulps()

    def anchor(self, at: int, offset: Value) -> None:
        """Anchors the law at a sample from an offset."""
        self.start, self.offset, self.arrived = at, offset, offset[0] == 0

    def distance(self, at: int) -> Value:
        """The distance at a sample with the bound of its error."""
        u = ulps()
        d0, e0 = abs(self.offset[0]), self.offset[1]
        q = exact(self.quarter)
        x = self.rate * (at - self.start)
        m = mpmath.exp(-x) - 1
        d = d0 + (d0 + q) * m
        decay = mpmath.exp(-x)
        error = e0 * decay + (d0 + q) * x * decay * self.rate_error
        error += u * ((d0 + q) * (mpmath.mpf("2.24") * abs(m) + 3 * x * decay) + abs(d))
        return max(d, mpmath.mpf(0)), error

    def value(self, at: int) -> Value:
        """The offset at a sample, 0 from the arrival on."""
        if self.arrived:
            return mpmath.mpf(0), mpmath.mpf(0)
        d, error = self.distance(at)
        return (d if self.offset[0] > 0 else -d), error

    def end(self) -> Value | None:
        """The arrival, in samples, with its error."""
        if self.arrived:
            return None
        u = ulps()
        d0, e0 = abs(self.offset[0]), self.offset[1]
        q = exact(self.quarter)
        samples = mpmath.log(1 + d0 / q) / self.rate
        relative = self.rate_error + mpmath.mpf("4.62") * u + (u if self.quarter == 3 else 0)
        error = samples * relative + e0 / ((d0 + q) * self.rate) + u * samples
        return self.start + samples, error

    def next(self) -> None:
        """Reaches the target."""
        self.arrived, self.offset = True, (mpmath.mpf(0), mpmath.mpf(0))

    def retarget(self, at: int, shift: float) -> None:
        """A change of target by a shift."""
        value, error = self.value(at)
        moved = value + exact(Fraction(shift))
        self.anchor(at, (moved, error + ulps() * abs(moved)))

    def set_speed(self, at: int, speed: Value) -> None:
        """A change of speed."""
        offset = self.value(at)
        self.set_rate(speed)
        self.anchor(at, offset)


class Glissando:
    """keptone-engine's glissando in exact arithmetic, with the bounds of its errors: its
    anchor, its glide and its key heard there, the whole keys stepped since, and its speed
    in keys a sample, 36 kappa/fe."""

    def __init__(self, at: int, glide: Value, heard: Value, speed: Value, rate: float) -> None:
        """A glissando from a sample, its glide and its key heard given."""
        self.sample_rate = rate
        self.set_step(speed)
        self.anchor(at, glide, heard)

    def set_step(self, speed: Value) -> None:
        """36 kappa/fe with its relative error."""
        self.step = 36 * speed[0] / exact(Fraction(self.sample_rate))
        self.step_error = speed[1] / speed[0] + 2 * ulps()

    def anchor(self, at: int, glide: Value, heard: Value) -> None:
        """Anchors the glissando at a sample."""
        self.start, self.glide0, self.heard0, self.steps = at, glide, heard, 0
        self.arrived = glide[0] == 0 and heard[0] == 0

    def direction(self) -> int:
        """Up where the glide lies below the target."""
        return 1 if self.glide0[0] < 0 else -1

    def steps_first(self) -> bool:
        """Whether the key heard steps before the glide reaches the target."""
        return self.steps + 1 < -self.direction() * self.heard0[0]

    def near(self) -> bool:
        """Whether a decision of keptone-engine's lies within ten times its error of its other
        side: the glide's direction, or whether the key heard steps first."""
        if self.arrived:
            return False
        glide, heard = self.glide0, self.heard0
        if glide[1] > 0 and abs(glide[0]) <= 10 * glide[1]:
            return True
        margin = abs(self.steps + 1 + self.direction() * heard[0])
        return heard[1] > 0 and margin <= 10 * heard[1]

    def heard(self) -> Value:
        """The key heard's offset with its error."""
        value = self.heard0[0] + self.direction() * self.steps
        return value, self.heard0[1] + rounding(value, self.heard0[1])

    def glide(self, at: int) -> Value:
        """The glide at a sample with its error, 0 from where it reaches the target on."""
        g0, e0 = self.glide0
        moved = self.step * (at - self.start)
        value = g0 + self.direction() * moved
        error = e0 + moved * self.step_error + ulps() * abs(value)
        if self.direction() * value >= 0:
            return mpmath.mpf(0), error
        return value, error

    def end(self) -> Value | None:
        """The next instant at which the key heard changes, with its error."""
        if self.arrived:
            return None
        u = ulps()
        g0, eg = self.glide0
        h0, eh = self.heard0
        if g0 == 0:
            return mpmath.mpf(self.start), mpmath.mpf(0)
        if self.steps_first():
            distance = self.direction() * (h0 - g0) + self.steps + 1
            samples = distance / self.step
            error = (eh + eg + u * abs(h0 - g0) + u * abs(distance)) / self.step
        else:
            samples = abs(g0) / self.step
            error = eg / self.step
        error += samples * (self.step_error + u) + u * samples
        return self.start + samples, error

    def next(self) -> None:
        """Steps the key heard, or reaches the target."""
        if self.glide0[0] != 0 and self.steps_first():
            self.steps += 1
        else:
            self.arrived = True
            self.glide0 = self.heard0 = (mpmath.mpf(0), mpmath.mpf(0))
            self.steps = 0

    def retarget(self, at: int, shift: float) -> None:
        """A change of target by a shift, the glide and the key heard kept."""
        u = ulps()
        glide, glide_error = self.glide(at)
        heard, heard_error = self.heard()
        moved = exact(Fraction(shift))
        glide, heard = glide + moved, heard + moved
        self.anchor(at, (glide, glide_error + u * abs(glide)), (heard, heard_error + rounding(heard, heard_error)))

    def set_speed(self, at: int, speed: Value) -> None:
        """A change of speed, the glide and the key heard kept."""
        glide, heard = self.glide(at), self.heard()
        self.set_step(speed)
        self.anchor(at, glide, heard)


def speed_value(time: float) -> Value:
    """kappa at a TIME with the bound of its error."""
    return kappa(time), kappa_error(time)


def make(head: list[float]) -> Glide | Glissando:
    """A scenario's glide from its head."""
    rate, unit, glissando, offset, time = head
    start = (exact(Fraction(offset)), mpmath.mpf(0))
    if glissando:
        return Glissando(0, start, start, speed_value(time), rate)
    return Glide(0, start, speed_value(time), QUARTERS[int(unit)], rate)


def times(randoms: Iterator[int]) -> float:
    """A TIME above 0: whole, between the steps, below 1 or beyond the scale."""
    return pick(
        randoms,
        [
            lambda: whole(randoms, 1, 99),
            lambda: uniform(randoms, 1.0, 99.0),
            lambda: uniform(randoms, 0.05, 1.0),
            lambda: uniform(randoms, 99.0, 115.0),
        ],
    )


def drawn(randoms: Iterator[int]) -> tuple[list[float], list[Line]]:
    """A scenario's head and its events in their order."""
    rate = pick(randoms, [lambda: 44100.0, lambda: 48000.0, lambda: 96000.0, lambda: 192000.0, lambda: 8000.0])
    unit = float(next(randoms) % 2)
    glissando = float(unit == 1 and next(randoms) % 2 == 0)
    if unit == 0:
        offset = pick(randoms, [lambda: uniform(randoms, -2.0, 2.0), lambda: whole(randoms, -2, 2) or 0.5])
    else:
        offset = pick(randoms, [lambda: whole(randoms, -24, 24) or 1.0, lambda: uniform(randoms, -24.0, 24.0)])
    head = [rate, unit, glissando, offset, times(randoms)]
    events: list[Line] = [["time", float(logarithmic(randoms, 19)), times(randoms)] for _ in range(next(randoms) % 3)]
    for _ in range(next(randoms) % 3):
        shift = whole(randoms, -12, 12) if unit else uniform(randoms, -1.0, 1.0)
        events.append(["target", float(logarithmic(randoms, 19)), shift or 0.5])
    if unit:
        events += [["switch", float(logarithmic(randoms, 19))] for _ in range(next(randoms) % 3)]
    events.sort(key=lambda event: event[1])
    return head, events


def designed() -> list[tuple[list[float], list[Line]]]:
    """Scenarios that pass through what random ones seldom reach: a glissando of whole keys
    and one from between them, a reversal and a change of speed in a glissando, the glissando
    turned on and off during a glide, a glide in octaves retargeted after it arrives, and
    TIMEs below 1 and beyond the scale."""
    return [
        ([48000.0, 1.0, 1.0, -7.0, 60.0], []),
        ([48000.0, 1.0, 1.0, 3.7, 70.0], [["time", 3000.0, 40.0]]),
        ([48000.0, 1.0, 1.0, -12.0, 80.0], [["target", 20000.0, 9.0], ["time", 30000.0, 50.0]]),
        ([44100.0, 1.0, 0.0, -12.0, 70.0], [["switch", 5000.0], ["switch", 30000.0]]),
        ([96000.0, 0.0, 0.0, 1.0, 50.0], [["target", 400000.0, -0.75]]),
        ([48000.0, 0.0, 0.0, -0.5, 0.3], [["time", 100.0, 105.0]]),
    ]


def run(scenario: tuple[list[float], list[Line]], evaluations: list[int]) -> tuple[list[Line], list[Value], bool]:
    """A scenario's lines, its ends, and whether a decision lies near its other side."""
    head, events = scenario
    glide = make(head)
    lines: list[Line] = [["scenario", *head]]
    ends: list[Value] = []
    near = False
    rate, time = head[0], head[4]

    def end_up_to(sample: int) -> None:
        nonlocal near
        while (end := glide.end()) is not None and end[0] <= sample:
            near |= isinstance(glide, Glissando) and glide.near()
            lines.append(["end", *double_double(end[0]), float(end[1])])
            ends.append(end)
            glide.next()

    for sample in sorted({int(event[1]) for event in events} | set(evaluations)):
        end_up_to(sample)
        for event in (event for event in events if int(event[1]) == sample):
            lines.append(event)
            if event[0] == "time":
                time = event[2]
                glide.set_speed(sample, speed_value(time))
            elif event[0] == "target":
                glide.retarget(sample, event[2])
            elif isinstance(glide, Glissando):
                glide = Glide(sample, glide.heard(), speed_value(time), QUARTERS[1], rate)
            else:
                offset = glide.value(sample)
                glide = Glissando(sample, offset, offset, speed_value(time), rate)
            if isinstance(glide, Glissando):
                near |= glide.near()
        end_up_to(sample)
        if sample in evaluations:
            value, error = glide.value(sample) if isinstance(glide, Glide) else glide.heard()
            lines.append(["sample", float(sample), *double_double(value), float(error)])
            if isinstance(glide, Glissando):
                value, error = glide.glide(sample)
                lines.append(["glide", float(sample), *double_double(value), float(error)])
    return lines, ends, near


def scenarios(randoms: Iterator[int]) -> Iterator[Line]:
    """The scenarios' lines, the designed ones first: each run once for its ends, its
    evaluations chosen between them, and run again; a random one drawn again where an end
    lies near an event or a decision near its other side."""
    queue = designed()
    count = len(queue)
    made = 0
    while made < SCENARIOS + count:
        scenario = queue.pop(0) if queue else drawn(randoms)
        samples = sorted({int(event[1]) for event in scenario[1]})
        horizon = (samples[-1] if samples else 0) + 2**21
        _, ends, near = run(scenario, [horizon])
        margins = [(instant, max(mpmath.mpf(10) ** -6, 10 * error)) for instant, error in ends]
        if near or any(abs(instant - sample) < margin for instant, margin in margins for sample in samples):
            if made < count:
                raise ValueError(f"a designed scenario's end lies near an event: {scenario[0]}")
            continue
        points = sorted([mpmath.mpf(0), *(mpmath.mpf(sample) for sample in samples), *(i for i, _ in margins)])
        points.append(mpmath.mpf(horizon))
        evaluations = set(samples)
        for low, high in itertools.pairwise(points):
            inside = [int(mpmath.floor(low)) + 1, int(mpmath.floor((low + high) / 2)), int(mpmath.ceil(high)) - 1]
            evaluations |= {n for n in inside if low < n < high}
        evaluations = {n for n in evaluations if all(abs(instant - n) >= margin for instant, margin in margins)}
        lines, _, _ = run(scenario, sorted(evaluations))
        yield from lines
        made += 1


def main() -> None:
    """Writes the file."""
    mpmath.mp.prec = 400
    randoms = splitmix64(SEED)
    out = [HEADER.rstrip("\n")]
    for name, *numbers in itertools.chain(speeds(randoms), scenarios(randoms)):
        out.append(" ".join([str(name), *(f"{to_bits(x):016x}" for x in numbers)]))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
