#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of the laws of the operator envelope for keptone-engine's tests.

    python3 scripts/eg_references.py

Computes the exact values of the laws of docs/MATH.md §4.1 and §4.3 at binary64
inputs: the rate q of a segment from its rate, the rate scaling and the key,
bounded at whole inputs, interpolated between them and evaluated twice as §0.7
says; the speed rho(q) under both of `eg.rate_law`'s laws; the level of an
attack after a progress, and the progress at which an attack reaches a target.
The rational ones are computed exactly with fractions, the logarithms and the
exponentials with mpmath at 400 bits, and a value beyond binary64's largest
finite number is taken at it after each operation that keptone-engine
saturates (MATH §0.5).

Before it writes anything, it checks the attack's closed form against its law,
dT/dPhi = max(1, 2 + (3840 - T)/256): from starts below the knee, mpmath's
integration of the law by Taylor series gives the closed form's levels within
10^-40 of them relatively, and, at the progresses at which the closed form says
that the attack reaches the knee and targets below it, the knee and those
targets within 10^-40 of the knee, the law's slope there being 1 or more; and
the law's slope from the knee on, 1, is the closed form's at the knee. It stops
if any does not.

A floor of the rate is exact in keptone-engine where its argument is computed
exactly, and may lie 1 from the exact one beyond 2^47; the file gives, beside
the rate, the slack that its floors so allow. It writes
crates/keptone-engine/tests/data/eg.txt: on each line the law's name, its
inputs and its exact value as a double-double, every number as the 16
hexadecimal digits of its binary64. The inputs are whole values, values between
them, the knee and its neighbours, and draws of SplitMix64 from a fixed seed,
on the scales and far beyond them, so that the file is the same every time.
"""

import math
import sys
from collections.abc import Callable, Iterator
from fractions import Fraction
from pathlib import Path

import mpmath

from binary64 import binades, neighbours, splitmix64, to_bits, uniform

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "eg.txt"
SEED = 0x656E76656C6F7065
RANDOM = 600
LARGEST = sys.float_info.max
U = Fraction(1, 2**53)
TWO_47 = 2**47

# T at full scale, the attack's knee and the asymptote below it (MATH §3.3, §4.3).
FULL_SCALE = 3840
KNEE = FULL_SCALE + 256
ASYMPTOTE = FULL_SCALE + 512

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/eg_references.py with mpmath; do not edit.
# Each line: a law's name, its inputs, and its exact value as a double-double
# (high, low), every number the binary64 bits in hexadecimal:
#   rate rate sensitivity key, then the largest magnitude of its values at whole
#     inputs and the slack of their floors
#   speed rate clock law (0 dx7, 1 exponential)
#   attack start progress | attack_progress start target
"""

type Line = list[str | float]
type Exact = Fraction | mpmath.mpf


def saturated(value: Exact) -> Exact:
    """The value, or binary64's largest finite number with its sign beyond it."""
    if abs(value) > LARGEST:
        return type(value)(LARGEST if value > 0 else -LARGEST)
    return value


def double_double(value: Exact) -> list[float]:
    """value as hi + lo, hi the nearest binary64 and lo the nearest to the rest."""
    hi = float(value)
    rest = value - (Fraction(hi) if isinstance(value, Fraction) else mpmath.mpf(hi))
    return [hi, float(rest)]


def clamp(value: Fraction, low: int, high: int) -> Fraction:
    """The value taken from low to high."""
    return min(max(value, Fraction(low)), Fraction(high))


def weights(x: Fraction) -> list[tuple[Fraction, int]]:
    """The whole inputs that keptone-engine interpolates between at x, with their weights."""
    whole = math.floor(x)
    part = x - whole
    return [(Fraction(1), whole)] if part == 0 else [(1 - part, whole), (part, whole + 1)]


def mp(value: Fraction) -> mpmath.mpf:
    """A rational as an mpf of the working precision."""
    return mpmath.mpf(value.numerator) / value.denominator


def group(key: int) -> int:
    """The rate scaling's group of keys at a whole key, clamp(floor(k/3) - 7, 0, 31)."""
    return min(max(key // 3 - 7, 0), 31)


def unbounded(rate: int, sensitivity: int, key: int) -> int:
    """floor(41R/64) + RS(k) at whole inputs, saturated as keptone-engine saturates it."""
    rated = math.floor(saturated(Fraction(41 * rate)) / 64)
    scaling = math.floor(saturated(Fraction(sensitivity * group(key))) / 8)
    return int(saturated(Fraction(rated + scaling)))


def unbounded_range(rate: int, sensitivity: int, key: int) -> tuple[int, int]:
    """The least and the greatest floor(41R/64) + RS(k) that keptone-engine may give within
    the errors it documents: exact where |R| and |KRS| lie below 2^47, and otherwise 41R
    and KRS times the group within u of themselves, and their sum as well."""
    if abs(rate) < TWO_47 and abs(sensitivity) < TWO_47:
        exact = unbounded(rate, sensitivity, key)
        return exact, exact
    rated = [math.floor(saturated(41 * rate * (1 + s * U)) / 64) for s in (-1, 1)]
    scaled = [math.floor(saturated(sensitivity * group(key) * (1 + s * U)) / 8) for s in (-1, 1)]
    sums = [saturated((a + b) * (1 + s * U)) for a in rated for b in scaled for s in (-1, 1)]
    return math.floor(min(sums)), math.floor(max(sums))


def trilinear(inputs: tuple[Fraction, Fraction, Fraction], at: Callable[[int, int, int], int]) -> tuple[Fraction, int]:
    """A law at whole rates, sensitivities and keys, trilinear between them, and the largest
    magnitude of its values at the whole inputs around."""
    rate, sensitivity, key = inputs
    value, largest = Fraction(0), 0
    for rate_weight, whole_rate in weights(rate):
        for sensitivity_weight, whole_sensitivity in weights(sensitivity):
            for key_weight, whole_key in weights(key):
                corner = at(whole_rate, whole_sensitivity, whole_key)
                value += rate_weight * sensitivity_weight * key_weight * corner
                largest = max(largest, abs(corner))
    return value, largest


def rate_law(rate: float, sensitivity: float, key: float) -> tuple[Fraction, int, Fraction]:
    """q of MATH §4.1 with the limits of §0.7, the largest magnitude of the values at whole
    inputs that it interpolates, and the slack of its floors."""
    given = (Fraction(rate), Fraction(sensitivity), Fraction(key))
    on_scale_inputs = (clamp(given[0], 0, 99), clamp(given[1], 0, 7), given[2])
    on_scale, on_largest = trilinear(on_scale_inputs, lambda r, s, k: min(63, unbounded(r, s, k)))
    if 0 <= given[0] <= 99 and 0 <= given[1] <= 7:
        return on_scale, on_largest, Fraction(0)
    value, largest = trilinear(given, unbounded)
    at_ends, end_largest = trilinear(on_scale_inputs, unbounded)
    slack = Fraction(0)
    for rate_weight, whole_rate in weights(given[0]):
        for sensitivity_weight, whole_sensitivity in weights(given[1]):
            for key_weight, whole_key in weights(given[2]):
                low, high = unbounded_range(whole_rate, whole_sensitivity, whole_key)
                slack += rate_weight * sensitivity_weight * key_weight * (high - low)
    total = saturated(on_scale + saturated(value - at_ends))
    return total, max(on_largest, largest, end_largest), slack


def speed(rate: float, clock: float, law: int) -> Exact:
    """rho(q) of MATH §4.1: C 2^floor(q/4) (1 + (q - 4 floor(q/4))/4), or C 2^(q/4), C = clock/4096."""
    base = Fraction(clock) / 4096
    if law == 0:
        quarter = math.floor(Fraction(rate) / 4)
        if quarter > 1100:
            return Fraction(LARGEST)
        if quarter < -1200:
            return Fraction(0)
        return saturated(base * Fraction(2) ** quarter * (1 + (Fraction(rate) - 4 * quarter) / 4))
    return saturated(mp(base) * mpmath.power(2, mpmath.mpf(rate) / 4))


def to_knee(start: mpmath.mpf) -> mpmath.mpf:
    """The progress of an attack from below the knee to it, 256 ln((T* - T0)/256)."""
    return 256 * mpmath.log((ASYMPTOTE - start) / 256)


def attack(start: float, progress: float) -> Exact:
    """The level of an attack after a progress, saturated as keptone-engine saturates it."""
    if start >= KNEE:
        return saturated(Fraction(start) + Fraction(progress))
    knee = to_knee(mpmath.mpf(start))
    if progress >= knee:
        return saturated(KNEE + (mpmath.mpf(progress) - knee))
    return ASYMPTOTE - (ASYMPTOTE - mpmath.mpf(start)) * mpmath.exp(-mpmath.mpf(progress) / 256)


def attack_progress(start: float, target: float) -> Exact:
    """The progress at which an attack from a start reaches a target."""
    if target <= start:
        return Fraction(0)
    if start >= KNEE:
        return saturated(Fraction(target) - Fraction(start))
    if target <= KNEE:
        return 256 * mpmath.log((ASYMPTOTE - mpmath.mpf(start)) / (ASYMPTOTE - mpmath.mpf(target)))
    return saturated(to_knee(mpmath.mpf(start)) + (mpmath.mpf(target) - KNEE))


def below_knee(start: mpmath.mpf, progress: mpmath.mpf) -> mpmath.mpf:
    """The closed form below the knee, T* - (T* - T0) e^(-Phi/256)."""
    return ASYMPTOTE - (ASYMPTOTE - start) * mpmath.exp(-progress / 256)


def check_attack() -> None:
    """Stops unless the attack's closed form is the solution of its law, within 10^-40."""
    mpmath.mp.prec = 160
    tolerance = mpmath.mpf(10) ** -40
    for start in (-5000, -1, 0, 16, 1716, 3000, 3840, 4000, 4095):
        law = mpmath.odefun(lambda _, t: 2 + (FULL_SCALE - t) / 256, 0, mpmath.mpf(start))
        knee = to_knee(mpmath.mpf(start))
        for share in (Fraction(1, 64), Fraction(1, 8), Fraction(1, 2), Fraction(63, 64)):
            progress = knee * mp(share)
            integrated, closed = law(progress), below_knee(mpmath.mpf(start), progress)
            if abs(integrated - closed) > tolerance * abs(closed):
                raise ValueError(f"the attack from {start} at {progress}: {integrated} against {closed}")
        # Where the closed form says the attack reaches the knee and a target
        # below it, the integrated law is there, its slope at least 1.
        target = (start + KNEE) / 2
        for progress, level in ((knee, KNEE), (attack_progress(float(start), target), target)):
            if abs(law(progress) - level) > tolerance * KNEE:
                raise ValueError(f"the attack from {start} at {progress}: {law(progress)} against {level}")
        slope = mpmath.diff(lambda p, s=start: below_knee(mpmath.mpf(s), p), knee)
        if abs(slope - 1) > tolerance:
            raise ValueError(f"the slope at the knee from {start}: {slope}")
    if any(max(1, 2 + (FULL_SCALE - mpmath.mpf(t)) / 256) != 1 for t in (KNEE, KNEE + 1, 10**6)):
        raise ValueError("the law's slope above the knee")


def draw(randoms: Iterator[int], low: float, high: float) -> float:
    """An input: a real of [low, high] or one of any binade up to binary64's largest."""
    return uniform(randoms, low, high) if next(randoms) % 3 else binades(randoms, -60, 1023)


def whole(randoms: Iterator[int], low: int, high: int) -> float:
    """A whole input of [low, high]."""
    return float(low + next(randoms) % (high - low + 1))


def pick(randoms: Iterator[int], choices: list[Callable[[], float]]) -> float:
    """One of the ways of drawing an input, each as likely."""
    return choices[next(randoms) % len(choices)]()


def rates(randoms: Iterator[int]) -> Iterator[Line]:
    """The rate, at random inputs on the scales and beyond them."""
    for _ in range(RANDOM):
        rate = pick(
            randoms,
            [lambda: whole(randoms, 0, 99), lambda: uniform(randoms, 0.0, 99.0), lambda: draw(randoms, -50.0, 300.0)],
        )
        sensitivity = pick(
            randoms,
            [lambda: whole(randoms, 0, 7), lambda: uniform(randoms, 0.0, 7.0), lambda: draw(randoms, -5.0, 20.0)],
        )
        key = pick(
            randoms,
            [
                lambda: whole(randoms, 0, 127),
                lambda: uniform(randoms, -20.0, 150.0),
                lambda: draw(randoms, -20.0, 150.0),
            ],
        )
        value, largest, slack = rate_law(rate, sensitivity, key)
        yield ["rate", rate, sensitivity, key, *double_double(value), float(largest), float(slack)]


def speeds(randoms: Iterator[int]) -> Iterator[Line]:
    """The speed under both laws, at whole rates, between them and beyond them."""
    inputs = [(float(q), 49096.0) for q in range(-8, 72)] + [(q + 0.5, 44100.0) for q in range(-8, 72)]
    inputs += [(x, 49096.0) for x in (*neighbours(0.0, 2), *neighbours(-4.0, 2), -1e-300, 1e300, -1e300, LARGEST)]
    for _ in range(RANDOM):
        rate = pick(randoms, [lambda: uniform(randoms, -10.0, 80.0), lambda: draw(randoms, -100.0, 400.0)])
        clock = pick(randoms, [lambda: 49096.0, lambda: 44100.0, lambda: uniform(randoms, 1.0, 1e6)])
        inputs.append((rate, clock))
    for law in (0, 1):
        for rate, clock in inputs:
            yield ["speed", rate, clock, float(law), *double_double(speed(rate, clock, law))]


def attacks(randoms: Iterator[int]) -> Iterator[Line]:
    """The attack's level and its progress to a target, at the knee and around it, and at
    random starts, progresses and targets on the scales and beyond them."""
    starts = [-5000.0, -1.0, 0.0, 16.0, 1716.0, 3840.0, 4000.0, *neighbours(float(KNEE), 2), 4352.0, 5000.0]
    pairs = []
    for start in starts:
        knee = float(to_knee(mpmath.mpf(start))) if start < KNEE else 0.0
        pairs += [(start, p) for p in (0.0, 2.0**-40, 1e-6, 1.0, 100.0, *neighbours(knee, 2), knee + 50.0)]
    for _ in range(RANDOM):
        start = pick(
            randoms,
            [
                lambda: uniform(randoms, -5000.0, 4352.0),
                lambda: whole(randoms, -100, 4400),
                lambda: draw(randoms, -5000.0, 6000.0),
            ],
        )
        progress = pick(
            randoms,
            [
                lambda: uniform(randoms, 0.0, 2000.0),
                lambda: uniform(randoms, 0.0, 1.0),
                lambda: abs(draw(randoms, 0.0, 6000.0)),
            ],
        )
        pairs.append((start, progress))
    for start, progress in pairs:
        yield ["attack", start, progress, *double_double(attack(start, progress))]
    targets = [
        (start, t)
        for start in starts
        for t in (start, start + 2.0**-30, 3000.0, *neighbours(float(KNEE), 1), 4352.0, 6000.0)
    ]
    for _ in range(RANDOM):
        start = pick(randoms, [lambda: uniform(randoms, -5000.0, 4352.0), lambda: draw(randoms, -5000.0, 6000.0)])
        target = pick(
            randoms,
            [
                lambda s=start: s + uniform(randoms, 0.0, 1.0),
                lambda: uniform(randoms, -5000.0, 4352.0),
                lambda: draw(randoms, -5000.0, 6000.0),
            ],
        )
        targets.append((start, target))
    for start, target in targets:
        yield ["attack_progress", start, target, *double_double(attack_progress(start, target))]


def lines() -> Iterator[Line]:
    """Every line of the file, as the law's name and its numbers."""
    check_attack()
    mpmath.mp.prec = 400
    randoms = splitmix64(SEED)
    yield from rates(randoms)
    yield from speeds(randoms)
    yield from attacks(randoms)


def main() -> None:
    """Writes the file."""
    out = [HEADER.rstrip("\n")]
    for name, *numbers in lines():
        out.append(" ".join([str(name), *(f"{to_bits(x):016x}" for x in numbers)]))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
