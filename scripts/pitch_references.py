#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of the laws of pitch for keptone-engine's tests.

    python3 scripts/pitch_references.py

Computes the exact values of the laws of docs/MATH.md §2.1-§2.3 at binary64
inputs: the shift tau and the transposition under the tuning's period, the key
after transposition, 12-TET's pitch and its inverse, the ratio, the detune of
the ratio mode, the pitch of the fixed mode under each law of its detune, and
the frequency. The laws that are rational, the shifts, the ratio and phi, are
computed exactly with fractions, and the others with mpmath at 400 bits. A
value beyond binary64's largest finite number is taken at it, with its sign,
after each operation that keptone-engine saturates (MATH §0.5). It writes
crates/keptone-engine/tests/data/pitch.txt: on each line the law's name, its
inputs and its value as a double-double, every number as the 16 hexadecimal
digits of its binary64. The inputs are special points and draws of SplitMix64
from a fixed seed, on the parameters' scales and far beyond them, so that the
file is the same every time.
"""

import math
import sys
from collections.abc import Iterator
from fractions import Fraction
from pathlib import Path

import mpmath

from binary64 import binades, from_bits, neighbours, splitmix64, to_bits, uniform

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "pitch.txt"
SEED = 0x7069746368
RANDOM = 400
LARGEST = sys.float_info.max

# keptone-engine's constants of the detune law: the pitches of 12-TET's keys 0
# and 127 at A4 = 440 Hz, correctly rounded.
MEASURED_LOWEST = from_bits(0x40084039858000B3)
MEASURED_HIGHEST = from_bits(0x402B3AB90C0AAAD7)
FIXED_DETUNE_STEP = Fraction(13457, 1 << 24)

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/pitch_references.py with mpmath; do not edit.
# Each line: a law's name, its inputs, and its exact value as a double-double
# (high, low), every number the binary64 bits in hexadecimal:
#   shift sigma period | transposition transpose period
#   transposed-keys key transpose shift | transposed-period key transpose shift period
#   pitch a4 key | key a4 pitch | ratio coarse fine | detune detune pitch
#   fixed-msfa, fixed-symmetric, fixed-none coarse fine detune | frequency ratio pitch
"""


def saturated(value: Fraction | mpmath.mpf) -> Fraction | mpmath.mpf:
    """The value, or binary64's largest finite number with its sign beyond it."""
    if abs(value) > LARGEST:
        return type(value)(LARGEST if value > 0 else -LARGEST)
    return value


def double_double(value: Fraction | mpmath.mpf) -> list[float]:
    """value as hi + lo, hi the nearest binary64 and lo the nearest to the rest."""
    hi = float(value)
    rest = value - (Fraction(hi) if isinstance(value, Fraction) else mpmath.mpf(hi))
    return [hi, float(rest)]


def tau(sigma: Fraction, period: Fraction) -> Fraction:
    """tau(sigma) under the tuning's period, exactly."""
    return sigma + (period - 12) * math.trunc(sigma / 12)


def phi(coarse: Fraction) -> Fraction:
    """phi(C) of MATH 2.3, exactly."""
    floor = math.floor(coarse)
    below = floor % 4
    return 3 * ((floor + 1) - coarse) if below == 3 else below + (coarse - floor)


def fixed_detune(detune: Fraction, law: str) -> Fraction:
    """The fixed mode's detune under a law, exactly."""
    steps = detune - 7
    if law == "symmetric" or (law == "msfa" and steps > 0):
        return FIXED_DETUNE_STEP * steps
    return Fraction(0)


def specials(values: list[float]) -> list[float]:
    """The values, their negatives, and each one's nearest neighbours."""
    out = []
    for x in values:
        for y in (x, -x):
            out += neighbours(y, 1)
    return out


def draw(randoms: Iterator[int], low: float, high: float, exponents: tuple[int, int]) -> float:
    """A random input: an integer of [low, high], a real of it, or one of any
    binade from 2^exponents[0] to 2^exponents[1], a third of the time each."""
    kind = next(randoms) % 3
    if kind == 0:
        return float(math.floor(uniform(randoms, low, high + 1)))
    if kind == 1:
        return uniform(randoms, low, high)
    return binades(randoms, *exponents)


type Line = list[str | float]

PERIODS = [12.0, 7.0, 5.0, 19.0, 31.0, 53.0, 0.0, 1.0, 12.5, 1000.0, -12.0]


def exact(value: Fraction) -> mpmath.mpf:
    """A fraction as an mpf, rounded once at mpmath's precision."""
    return mpmath.mpf(value.numerator) / value.denominator


def shifts(randoms: Iterator[int]) -> Iterator[Line]:
    """tau of a shift and of a transposition under the tuning's period: at
    multiples of 12 and their neighbours, where tau jumps, and at random."""
    points = [0.0, 0.5, 1.0, 11.0, 12.0, 13.0, 24.0, 36.0, 120.0, 2.0**-60, 1e6, 12.0 * 2.0**52, 1e17, 1e300]
    for sigma in specials(points):
        for period in PERIODS:
            yield ["shift", sigma, period, *double_double(saturated(tau(Fraction(sigma), Fraction(period))))]
    for transpose in specials([0.0, 12.0, 24.0, 36.0, 48.0, 2.0**-60, 1e6, 12.0 * 2.0**52, 1e17, 1e300]):
        for period in PERIODS:
            value = tau(Fraction(transpose) - 24, Fraction(period))
            yield ["transposition", transpose, period, *double_double(saturated(value))]
    for _ in range(RANDOM):
        sigma, transpose = draw(randoms, -200.0, 200.0, (-60, 80)), draw(randoms, -60.0, 110.0, (-60, 80))
        period = PERIODS[next(randoms) % len(PERIODS)] if next(randoms) % 2 else uniform(randoms, -20.0, 120.0)
        yield ["shift", sigma, period, *double_double(saturated(tau(Fraction(sigma), Fraction(period))))]
        value = tau(Fraction(transpose) - 24, Fraction(period))
        yield ["transposition", transpose, period, *double_double(saturated(value))]


def transposed(randoms: Iterator[int]) -> Iterator[Line]:
    """The key after transposition, each sum taken at binary64's largest beyond it."""
    for _ in range(RANDOM):
        key = draw(randoms, 0.0, 127.0, (-40, 1023))
        transpose = draw(randoms, 0.0, 48.0, (-40, 1023))
        shift = draw(randoms, -24.0, 24.0, (-40, 1023))
        keys = saturated(saturated(Fraction(key) + Fraction(transpose) - 24) + Fraction(shift))
        yield ["transposed-keys", key, transpose, shift, *double_double(keys)]
        period = PERIODS[next(randoms) % len(PERIODS)]
        first = saturated(Fraction(key) + saturated(tau(Fraction(transpose) - 24, Fraction(period))))
        value = saturated(first + saturated(tau(Fraction(shift), Fraction(period))))
        yield ["transposed-period", key, transpose, shift, period, *double_double(value)]


def tunings(randoms: Iterator[int]) -> Iterator[Line]:
    """12-TET's pitch of a key and the key of a pitch."""
    a4s = [440.0, 432.0, 415.0, 442.0, 1.0, 2.0**-1000, 1e300]
    for _ in range(RANDOM):
        a4 = a4s[next(randoms) % len(a4s)] if next(randoms) % 2 else abs(binades(randoms, -30, 30))
        key = draw(randoms, 0.0, 127.0, (-40, 1023))
        pitch = mpmath.log(mpmath.mpf(a4), 2) + (mpmath.mpf(key) - 69) / 12
        yield ["pitch", a4, key, *double_double(pitch)]
        target = draw(randoms, -10.0, 30.0, (-40, 1023))
        value = saturated(69 + 12 * (mpmath.mpf(target) - mpmath.log(mpmath.mpf(a4), 2)))
        yield ["key", a4, target, *double_double(value)]


def ratios(randoms: Iterator[int]) -> Iterator[Line]:
    """The ratio at the DX7's steps, between them and beyond them."""
    for coarse in [*range(-2, 33), 0.5, -0.5, 1e300, -1e300]:
        for fine in [0.0, 1.0, 50.0, 99.0, -100.0, 1e300]:
            value = saturated(ratio_of(Fraction(coarse), Fraction(fine)))
            yield ["ratio", float(coarse), fine, *double_double(value)]
    for _ in range(RANDOM):
        coarse, fine = draw(randoms, -5.0, 40.0, (-60, 1023)), draw(randoms, -200.0, 300.0, (-60, 1023))
        yield ["ratio", coarse, fine, *double_double(saturated(ratio_of(Fraction(coarse), Fraction(fine))))]


def detunes(randoms: Iterator[int]) -> Iterator[Line]:
    """The detune of the ratio mode, its pitch taken within the measured ones."""
    for _ in range(RANDOM):
        detune, pitch = draw(randoms, 0.0, 14.0, (-60, 1023)), draw(randoms, -5.0, 20.0, (-60, 1023))
        clamped = mpmath.mpf(min(max(pitch, MEASURED_LOWEST), MEASURED_HIGHEST))
        exponential = mpmath.exp(-mpmath.mpf("0.396") * clamped)
        value = mpmath.mpf("0.0209") * exponential * clamped * (mpmath.mpf(detune) - 7) / 7
        yield ["detune", detune, pitch, *double_double(value)]


def fixed(randoms: Iterator[int]) -> Iterator[Line]:
    """The pitch of the fixed mode under each law of its detune."""
    for law in ("msfa", "symmetric", "none"):
        for _ in range(RANDOM):
            coarse = draw(randoms, -4.0, 36.0, (-60, 1023))
            fine, detune = draw(randoms, -100.0, 199.0, (-60, 1023)), draw(randoms, 0.0, 14.0, (-60, 1023))
            sum_ = exact(phi(Fraction(coarse)) + Fraction(fine) / 100)
            value = mpmath.log(10, 2) * sum_ + exact(fixed_detune(Fraction(detune), law))
            yield [f"fixed-{law}", coarse, fine, detune, *double_double(value)]


def frequencies(randoms: Iterator[int]) -> Iterator[Line]:
    """The frequency of a pitch times a ratio, the pitch held at 1023 octaves."""
    for _ in range(2 * RANDOM):
        ratio = 1.0 if next(randoms) % 4 == 0 else draw(randoms, -2.0, 40.0, (-80, 80))
        pitch = draw(randoms, -1100.0, 1100.0, (-60, 1023)) if next(randoms) % 2 else uniform(randoms, -10.0, 20.0)
        value = saturated(mpmath.mpf(ratio) * mpmath.power(2, min(mpmath.mpf(pitch), mpmath.mpf(1023))))
        yield ["frequency", ratio, pitch, *double_double(value)]


def lines() -> Iterator[Line]:
    """Every line of the file, as the law's name and its numbers."""
    mpmath.mp.prec = 400
    randoms = splitmix64(SEED)
    for law in (shifts, transposed, tunings, ratios, detunes, fixed, frequencies):
        yield from law(randoms)


def ratio_of(coarse: Fraction, fine: Fraction) -> Fraction:
    """The ratio of MATH 2.2, exactly."""
    rho = coarse if coarse >= 1 else (1 + coarse) / 2
    return rho * (100 + fine) / 100


def main() -> None:
    """Writes the file."""
    out = [HEADER.rstrip("\n")]
    for name, *numbers in lines():
        out.append(" ".join([str(name), *(f"{to_bits(x):016x}" for x in numbers)]))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
