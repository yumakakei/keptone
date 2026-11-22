#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of the LFO and the random source for keptone-engine's tests.

    python3 scripts/lfo_references.py

Computes the laws of docs/MATH.md §6 exactly at binary64 inputs, with integers,
fractions and mpmath at 400 bits: the random source of §6.3, SplitMix64 read at
counters, a part's three streams and the sample-and-hold values and phases
that its outputs give; the LFO's frequency of §6.1 at whole speeds, between
them in log f, below 0 and far beyond 99, taken at binary64's largest finite
number beyond it; its waves of §6.2; and the stages of its delay of §6.4, t1
taken at that number at the steps where it lies beyond it (§0.5). And it runs
the LFO as keptone-engine runs it, in scenarios of an engine rate, a speed, a
delay, a stream's seed and an index before its values, and events at whole
samples drawn from SplitMix64 with a fixed seed: key-ons with LFO SYNC or
without it, and changes of the speed and of the delay. It records at whole
samples the phase, the count of values drawn, the value that the sample and
hold holds and the delay's fade, none where the phase lies within ten times
the bound of its error of a wrap or the fade that near a boundary of its
stages, drawing a scenario again where an event lies that near one; and beside
the phase and the fade a bound on keptone-engine's error, first order in
u = 2^-53, which it follows from the errors that keptone-engine's functions
document through every piece of law. It writes
crates/keptone-engine/tests/data/lfo.txt, every number as the 16 hexadecimal
digits of its binary64, or of its u64 where the header says so, so that the
file is the same every time.
"""

import itertools
import math
import sys
from collections.abc import Callable, Iterator
from fractions import Fraction
from pathlib import Path

import mpmath

from binary64 import binades, neighbours, splitmix64, to_bits, uniform

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "lfo.txt"
SEED = 0x6C666F
SCENARIOS = 120
RANDOM = 500
MASK = (1 << 64) - 1
GAMMA = 0x9E3779B97F4A7C15

# The rate of the firmware's timer, and binary64's largest finite number.
TICK = Fraction(9426500, 8 * 3165)
LARGEST = Fraction(sys.float_info.max)

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/lfo_references.py with mpmath; do not edit.
# Every number the binary64 bits in hexadecimal, the u64 ones marked so:
#   value seed index output, all u64 | streams seed part, then part voices
#     phases, all u64 | unit output (u64), then its S&H value and its phase
#   frequency S, then f as a double-double and the bound of its relative
#     error | shape wave phase held, then l as a double-double | stages DELAY,
#     then t1 and t2 as double-doubles | nodelay DELAY
#   scenario rate speed seed base (the last two u64), then its lines in the
#     order of its time: key_on sample sync delay | speed sample speed |
#     delay sample delay | sample n, the phase in turns as a double-double and
#     the bound of its error, the count of values drawn (u64), the S&H value
#     held, and the fade as a double-double and the bound of its error
"""

type Line = list[str | float | int]
type Exact = Fraction | mpmath.mpf
type Value = tuple[Exact, mpmath.mpf]
type Scenario = tuple[list[float | int], list[Line]]


def ulps() -> mpmath.mpf:
    """u = 2^-53."""
    return mpmath.mpf(2) ** -53


def exact(value: Exact) -> mpmath.mpf:
    """A rational or an mpf as an mpf."""
    if isinstance(value, mpmath.mpf):
        return value
    value = Fraction(value)
    return mpmath.mpf(value.numerator) / value.denominator


def saturated(value: Fraction) -> Fraction:
    """A value beyond binary64's largest finite number taken at it (MATH §0.5)."""
    return min(value, LARGEST)


def double_double(value: Exact) -> list[float]:
    """value as hi + lo, hi the nearest binary64 and lo the nearest to the rest."""
    hi = float(value)
    rest = value - (Fraction(hi) if isinstance(value, Fraction) else mpmath.mpf(hi))
    return [hi, float(rest)]


def mix(state: int) -> int:
    """SplitMix64's finaliser."""
    z = ((state ^ (state >> 30)) * 0xBF58476D1CE4E5B9) & MASK
    z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
    return z ^ (z >> 31)


def value(seed: int, index: int) -> int:
    """The value of index i of the stream seeded with x: mix(x + i gamma) modulo 2^64."""
    return mix((seed + index * GAMMA) & MASK)


def streams(seed: int, part: int) -> list[int]:
    """The seeds of part j's three streams, the outputs 3j + 1 to 3j + 3."""
    return [value(seed, (3 * part + k) & MASK) for k in (1, 2, 3)]


def sample_hold(output: int) -> Fraction:
    """2 floor(x/2^11) 2^-53 - 1."""
    return Fraction(2 * (output >> 11), 2**53) - 1


def phase_of(output: int) -> Fraction:
    """floor(x/2^11) 2^-53."""
    return Fraction(output >> 11, 2**53)


def increment(speed: int) -> int:
    """The increment of the 16-bit phase a tick at a whole speed of 0 or more."""
    a = (165 * speed) // 64
    return 11 * max(1, a) if a < 160 else a * (11 + (a - 160) // 4)


def frequency(speed: float) -> Exact:
    """f of MATH §6.1: F/2^16 times the increment at whole speeds, in log f between them,
    f(0) 2^S below 0, taken at binary64's largest finite number beyond it."""
    if speed < 0:
        return exact(TICK * 11 / 65536) * mpmath.power(2, mpmath.mpf(speed))
    x = Fraction(speed)
    whole = math.floor(x)
    part = x - whole
    if part == 0:
        return saturated(TICK * increment(whole) / 65536)
    below, above = increment(whole), increment(whole + 1)
    return exact(TICK * below / 65536) * mpmath.power(mpmath.mpf(above) / below, exact(part))


def advance_error(speed: int) -> mpmath.mpf:
    """The relative error that keptone-engine's advance documents at a whole speed."""
    if increment(speed) < 2**53:
        return mpmath.mpf(0)
    return ulps() if (165 * speed) // 64 < 2**53 else 11 * ulps()


def frequency_error(speed: float) -> mpmath.mpf:
    """The relative error that keptone-engine's frequency documents; 2^-1074 more absolutely,
    which the tests add, where it is subnormal."""
    u = ulps()
    if speed < 0:
        return mpmath.mpf("4.86") * u
    x = Fraction(speed)
    whole = math.floor(x)
    t = exact(x - whole)
    if t == 0:
        return 2 * u + advance_error(whole)
    e = max(advance_error(whole), advance_error(whole + 1))
    log = abs(mpmath.log(mpmath.mpf(increment(whole + 1)) / increment(whole), 2))
    return u * (mpmath.mpf("4.86") + t + mpmath.mpf("1.82") * t * log) + e * (1 + 2 * t)


def shape(wave: int, turns: float, held: float) -> Exact:
    """l of MATH §6.2 of a wave at a phase."""
    phi = Fraction(turns)
    if wave == 0:
        return abs(4 * phi - 2) - 1
    if wave == 1:
        return 1 - 2 * phi
    if wave == 2:
        return 2 * phi - 1
    if wave == 3:
        return Fraction(-1 if phi < Fraction(1, 2) else 1)
    if wave == 4:
        return -mpmath.sinpi(2 * mpmath.mpf(turns))
    return Fraction(held)


def stages_at(delay: int) -> tuple[Fraction, Fraction]:
    """t1 and t2 at a whole DELAY of 1 or more, a = 99 - DELAY by Euclid's division, t1 taken
    at binary64's largest finite number where it lies beyond it."""
    group, rest = divmod(99 - delay, 16)
    rise = 256 / TICK if group < 2 else 256 / ((16 + rest) * 2 ** (2 + group) // 256 * TICK)
    # Below this group t1, above 2^(-group)/2, (16 + rest)F lying below 2^14, is far beyond it.
    if group < -1100:
        return LARGEST, rise
    return saturated(Fraction(2) ** (14 - group) / ((16 + rest) * TICK)), rise


def stages(delay: float) -> tuple[tuple[Fraction, Fraction], mpmath.mpf] | None:
    """t1 and t2 at a DELAY, none at or below 0, linear between whole ones, DELAY 0 taken as
    none; and the relative error that keptone-engine's stages documents."""
    if delay <= 0:
        return None
    x = Fraction(delay)
    whole = math.floor(x)
    part = x - whole
    below = (Fraction(0), Fraction(0)) if whole <= 0 else stages_at(whole)
    if part == 0:
        return below, 3 * ulps()
    above = stages_at(whole + 1)
    return (below[0] + part * (above[0] - below[0]), below[1] + part * (above[1] - below[1])), 5 * ulps()


class Lfo:
    """keptone-engine's LFO in exact arithmetic, with the bounds of the errors of
    keptone-engine's: its phase from its last start at its step a sample, the values that
    it has drawn there, and its delay's fade."""

    def __init__(self, rate: float, speed: float, seed: int, base: int) -> None:
        """An LFO from sample 0 at phase 0, its first value drawn, the full depth."""
        self.rate = Fraction(rate)
        self.seed, self.base = seed, base
        self.start, self.turns, self.draws = 0, (mpmath.mpf(0), mpmath.mpf(0)), 0
        self.set_step(speed)
        self.fade_full(0)

    def set_step(self, speed: float) -> None:
        """The step a sample, f/fe, with its relative error, the quotient's u included."""
        self.step = exact(frequency(speed)) / exact(self.rate)
        self.step_error = frequency_error(speed) + ulps()

    def phase(self, sample: int) -> tuple[mpmath.mpf, int, mpmath.mpf]:
        """The turns, the wraps since the last start and the bound of the turns' error, at a
        sample."""
        u = ulps()
        advance = self.step * (sample - self.start)
        total = self.turns[0] + advance
        wraps = int(mpmath.floor(total))
        return total - wraps, wraps, self.turns[1] + advance * self.step_error + 3 * u + 2 * u * u * advance

    def near_wrap(self, sample: int) -> bool:
        """Whether the phase lies within ten times its error of a wrap, where its count of wraps
        may be the neighbouring one; at its start it has none."""
        turns, _, error = self.phase(sample)
        return sample != self.start and min(turns, 1 - turns) <= 10 * error

    def count(self, sample: int) -> int:
        """The values drawn by a sample."""
        return self.draws + self.phase(sample)[1]

    def held(self, sample: int) -> Fraction:
        """The value that the sample and hold holds at a sample."""
        return sample_hold(value(self.seed, (self.base + self.count(sample) + 1) & MASK))

    def key_on(self, sample: int, sync: bool, delay: float) -> None:  # noqa: FBT001
        """A key-on: the delay restarted, and with LFO SYNC the phase at 0 with a new value."""
        self.set_fade(sample, delay, "waiting", (Fraction(0), mpmath.mpf(0)))
        if sync:
            self.draws += self.phase(sample)[1] + 1
            self.start, self.turns = sample, (mpmath.mpf(0), mpmath.mpf(0))

    def set_speed(self, sample: int, speed: float) -> None:
        """A change of speed, the phase carried over."""
        turns, wraps, error = self.phase(sample)
        self.draws += wraps
        self.start, self.turns = sample, (turns, error)
        self.set_step(speed)

    def set_delay(self, sample: int, delay: float) -> None:
        """A change of DELAY, the share of the stage that has passed carried over."""
        stage, passed, _ = self.fade_at(sample)
        self.set_fade(sample, delay, stage, passed)

    def fade_full(self, sample: int) -> None:
        """The full depth from a sample on."""
        self.fade_stage, self.fade_start = "full", sample
        self.passed = (Fraction(0), mpmath.mpf(0))
        self.wait = self.rise = (Fraction(0), mpmath.mpf(0))

    def set_fade(self, sample: int, delay: float, stage: str, passed: Value) -> None:
        """A fade from a sample, its stage and the share passed of it given: the stages' lengths
        in samples within the errors of the stages and the product by the rate."""
        found = stages(delay)
        if found is None or stage == "full":
            self.fade_full(sample)
            return
        (t1, t2), error = found
        self.fade_stage, self.fade_start, self.passed = stage, sample, passed
        self.wait = (saturated(t1 * self.rate), error + ulps())
        self.rise = (saturated(t2 * self.rate), error + ulps())

    def fade_at(self, sample: int) -> tuple[str, Value, bool]:
        """The stage at a sample, the share of it that has passed with its error, and whether
        the sample lies within ten times the errors of a boundary of the stages."""
        u = ulps()
        samples = Fraction(sample - self.fade_start)
        (p, p_error), (wait, wait_error), (rise, rise_error) = self.passed, self.wait, self.rise
        if self.fade_stage == "full":
            return "full", (Fraction(1), mpmath.mpf(0)), False
        if self.fade_stage == "waiting":
            left = (1 - p) * wait
            left_error = p_error * exact(wait) + exact(left) * (wait_error + 2 * u)
            near = abs(exact(samples - left)) <= 10 * left_error
            if samples < left:
                share = samples / wait
                return "waiting", (p + share, p_error + exact(share) * (wait_error + u) + u), near
            share = (samples - left) / rise
            stage, passed, near_one = self.rising(share, left_error / exact(rise) + exact(share) * (rise_error + 2 * u))
            return stage, passed, near or near_one
        share = samples / rise
        return self.rising(p + share, p_error + exact(share) * (rise_error + u) + u)

    @staticmethod
    def rising(share: Fraction, error: mpmath.mpf) -> tuple[str, Value, bool]:
        """The rise with a share passed, or the full depth from 1 on, and whether the share lies
        within ten times its error of 1."""
        near = abs(exact(share - 1)) <= 10 * error
        if share < 1:
            return "rising", (share, error), near
        return "full", (Fraction(1), mpmath.mpf(0)), near

    def fade(self, sample: int) -> tuple[Value, bool]:
        """w at a sample with its error, and whether it lies near a boundary of the stages."""
        stage, passed, near = self.fade_at(sample)
        if stage == "waiting":
            return (Fraction(0), mpmath.mpf(0)), near
        return passed, near


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


def speeds(randoms: Iterator[int]) -> list[float]:
    """Speeds at the steps, their halves and quarters, about the regimes of the advance's
    error, where the floor of 165S/64 of a rounded 165S would fall short (S = 19 modulo 64
    with 165S from 2^53 to 2^54), about binary64's largest frequency, and drawn."""
    out = [x for s in range(-3, 103) for x in (float(s), s + 0.5, s + 0.25)]
    out += [*neighbours(0.0, 2), -1018.5, -1050.0, -1080.0, -1e300, 1e6, 1e300, sys.float_info.max]
    for edge in (73624278, 3493701529111658):
        out += [float(edge - 1), float(edge), edge - 0.5, edge + 0.5]
    first = -(-(2**53) // 165)
    out += [float(first + (19 - first) % 64 + 64 * k) for k in (0, 1, 1000, 10**9)]
    for edge in (1.38e155, 1.3800001e155):
        out += neighbours(edge, 1)
    out += [draw(randoms, -20.0, 120.0) for _ in range(RANDOM)]
    return out


def checked() -> None:
    """Checks the values against SplitMix64's own sequence: the value of index i is its i-th
    output from the seed."""
    for seed, index in itertools.product((0, 7, MASK), (1, 2, 50)):
        if value(seed, index) != next(itertools.islice(splitmix64(seed), index - 1, None)):
            raise ValueError(f"value({seed}, {index}) is not SplitMix64's")


def laws(randoms: Iterator[int]) -> Iterator[Line]:
    """The random source, the frequency, the waves and the delay's stages."""
    checked()
    pairs = [(0, 1), (0, 2), (MASK, 1), (1, MASK), (GAMMA, 3), (0x123456789ABCDEF0, 1 << 40)]
    pairs += [(next(randoms), next(randoms) % 4096 + 1) for _ in range(200)]
    for seed, index in pairs:
        yield ["value", seed, index, value(seed, index)]
    for seed, part in [(0, 0), (0, 1), (MASK, 2), *((next(randoms), next(randoms) % 64) for _ in range(40))]:
        yield ["streams", seed, part, *streams(seed, part)]
    for output in [0, MASK, 1 << 63, (1 << 11) - 1, 1 << 11, *(next(randoms) for _ in range(100))]:
        yield ["unit", output, float(sample_hold(output)), float(phase_of(output))]
    for speed in speeds(randoms):
        yield ["frequency", speed, *double_double(frequency(speed)), float(frequency_error(speed))]
    phases = [0.0, 0.25, 0.5, 0.75, 2.0**-30, 2.0**-1000, 1 - 2.0**-53, *neighbours(0.5, 2), *neighbours(0.25, 1)]
    phases += [*neighbours(0.375, 1), *neighbours(0.625, 1), *(uniform(randoms, 0.0, 1.0) for _ in range(RANDOM))]
    for wave in range(6):
        for turns in phases if wave in {0, 4} else phases[::3]:
            held = uniform(randoms, -1.0, 1.0)
            yield ["shape", float(wave), turns, held, *double_double(shape(wave, turns, held))]
    delays = [x for d in range(-2, 103) for x in (float(d), d + 0.5)] + [*neighbours(0.0, 1), 1e-310, 150.0, 1e3]
    delays += [16460.0, 16461.0, 16460.5, 2.0**53, 1e300, *(draw(randoms, -10.0, 130.0) for _ in range(RANDOM // 2))]
    for delay in delays:
        found = stages(delay)
        if found is None:
            yield ["nodelay", delay]
        else:
            (t1, t2), _ = found
            yield ["stages", delay, *double_double(t1), *double_double(t2)]


def drawn(randoms: Iterator[int]) -> Scenario:
    """A scenario's head, an engine rate, a speed, a seed and a base, and its events in their
    order."""
    rate = pick(randoms, [lambda: 44100.0, lambda: 48000.0, lambda: 96000.0, lambda: 192000.0, lambda: 8000.0])

    def speed() -> float:
        return pick(
            randoms,
            [lambda: whole(randoms, 0, 99), lambda: uniform(randoms, 0.0, 99.0), lambda: uniform(randoms, -5.0, 120.0)],
        )

    def delay() -> float:
        return pick(
            randoms,
            [lambda: whole(randoms, 0, 99), lambda: uniform(randoms, 0.0, 99.0), lambda: uniform(randoms, -2.0, 110.0)],
        )

    head = [rate, speed(), next(randoms), (next(randoms) % 64) << 32 if next(randoms) % 2 else 0]
    events: list[Line] = [
        ["key_on", float(logarithmic(randoms, 21)), float(next(randoms) % 2), delay()]
        for _ in range(1 + next(randoms) % 4)
    ]
    events += [["speed", float(logarithmic(randoms, 21)), speed()] for _ in range(next(randoms) % 3)]
    events += [["delay", float(logarithmic(randoms, 21)), delay()] for _ in range(next(randoms) % 3)]
    events.sort(key=lambda event: event[1])
    return head, events


def designed() -> list[Scenario]:
    """Scenarios that pass through what random ones seldom reach: changes of DELAY in each
    stage, two in one wait, and to 0, a retrigger, speeds beyond the scale, a DELAY between 0
    and 1, and the LFO of a voice's note."""
    return [
        (
            [48000.0, 35.0, 1, 0],
            [
                ["key_on", 0.0, 1.0, 99.0],
                ["delay", 60000.0, 50.0],
                ["delay", 61000.0, 60.0],
                ["key_on", 200000.0, 0.0, 99.0],
                ["delay", 330000.0, 0.0],
            ],
        ),
        (
            [48000.0, 99.0, 2, 3 << 32],
            [
                ["key_on", 10.0, 1.0, 40.0],
                ["key_on", 5000.0, 1.0, 40.0],
                ["speed", 7000.0, 120.0],
                ["speed", 9000.0, -3.5],
            ],
        ),
        ([44100.0, 0.0, 3, 0], [["key_on", 1.0, 1.0, 0.5], ["delay", 900.0, 99.0], ["key_on", 2.0**20, 0.0, 0.0]]),
    ]


def run(scenario: Scenario, evaluations: set[int]) -> tuple[list[Line], bool]:
    """A scenario's lines, and whether an event of it lies near a wrap of the phase or near a
    boundary of the fade's stages where it acts on them: a key-on with LFO SYNC, which counts
    the wraps, or a change of DELAY, which carries the stage over. A change of speed carries
    the count and the phase over alike on either side of a wrap."""
    (rate, speed, seed, base), events = scenario
    lfo = Lfo(rate, speed, seed, base)
    lines: list[Line] = [["scenario", rate, speed, seed, base]]
    near = False
    for sample in sorted({int(event[1]) for event in events} | evaluations):
        for event in (event for event in events if int(event[1]) == sample):
            lines.append(event)
            if event[0] == "key_on":
                near |= bool(event[2]) and lfo.near_wrap(sample)
                lfo.key_on(sample, bool(event[2]), event[3])
            elif event[0] == "speed":
                lfo.set_speed(sample, event[2])
            else:
                near |= lfo.fade_at(sample)[2]
                lfo.set_delay(sample, event[2])
        if sample in evaluations:
            (fade, fade_error), fade_near = lfo.fade(sample)
            if lfo.near_wrap(sample) or fade_near:
                continue
            turns, _, error = lfo.phase(sample)
            count, held = lfo.count(sample), float(lfo.held(sample))
            lines.append(["sample", float(sample), *double_double(turns), float(error), count, held])
            lines[-1] += [*double_double(fade), float(fade_error)]
    return lines, near


def evaluations_of(scenario: Scenario) -> set[int]:
    """The samples at which a scenario is read: its events, beside them and between them, and
    within each stage of the delay after each key-on."""
    rate, events = Fraction(scenario[0][0]), scenario[1]
    samples = sorted({int(event[1]) for event in events})
    points = [0, *samples, samples[-1] + 2**20]
    out = set(samples)
    for low, high in itertools.pairwise(points):
        out |= {low + 1, (low + high) // 2, high - 1, low + (high - low) // 7}
    for event in events:
        found = stages(event[3]) if event[0] == "key_on" else None
        if found is not None:
            (t1, t2), _ = found
            wait, rise = saturated(t1 * rate), saturated(t2 * rate)
            for offset in (wait / 2, wait + rise / 2, (wait + rise) * 5 / 4 + 1):
                if offset < 2**40:
                    out.add(int(event[1]) + math.floor(offset))
    return {n for n in out if 0 <= n <= points[-1]}


def scenarios(randoms: Iterator[int]) -> Iterator[Line]:
    """The scenarios' lines, the designed ones first; a random one drawn again where an event
    lies near a wrap or a boundary of the stages."""
    queue = designed()
    count = len(queue)
    made = 0
    while made < SCENARIOS + count:
        scenario = queue.pop(0) if queue else drawn(randoms)
        lines, near = run(scenario, evaluations_of(scenario))
        if near:
            if made < count:
                raise ValueError(f"a designed scenario's event lies near a boundary: {scenario[0]}")
            continue
        yield from lines
        made += 1


# The places of the u64 values in the lines of each word.
INTEGERS = {"value": {0, 1, 2}, "streams": {0, 1, 2, 3, 4}, "unit": {0}, "scenario": {2, 3}, "sample": {4}}


def words(name: str, numbers: list) -> str:
    """A line of the file: u64 values where INTEGERS says so, binary64 bits elsewhere."""
    marked = INTEGERS.get(name, set())
    return " ".join([name, *(f"{x:016x}" if k in marked else f"{to_bits(x):016x}" for k, x in enumerate(numbers))])


def main() -> None:
    """Writes the file."""
    mpmath.mp.prec = 400
    randoms = splitmix64(SEED)
    out = [HEADER.rstrip("\n")]
    out += [words(str(name), numbers) for name, *numbers in itertools.chain(laws(randoms), scenarios(randoms))]
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
