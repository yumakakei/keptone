#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of the laws of level for keptone-engine's tests.

    python3 scripts/level_references.py

Computes the exact values of the laws of docs/MATH.md §3.1, §3.3 and §3.4 at
binary64 inputs: the level curve s(L), the envelope's part E(L) of a total
level, linear between the steps as a whole and interpolated in the gain below
1, the total level E + O - 4256 and the gain 2^(T/256 - 14). The rational ones
are computed exactly with fractions, the logarithm and the exponential with
mpmath at 400 bits, and a value beyond binary64's largest finite number is
taken at it after each operation that keptone-engine saturates (MATH §0.5). It
writes crates/keptone-engine/tests/data/level.txt: on each line the law's name,
its inputs and its exact value as a double-double, every number as the 16
hexadecimal digits of its binary64, or "silent" and the level for a level at
or below 0. The inputs are the steps, their halves and their neighbours, and
draws of SplitMix64 from a fixed seed, on the scale and far beyond it, so that
the file is the same every time.
"""

import math
import sys
from collections.abc import Iterator
from fractions import Fraction
from pathlib import Path

import mpmath

from binary64 import binades, neighbours, splitmix64, to_bits, uniform

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "level.txt"
SEED = 0x6C6576656C
RANDOM = 600
LARGEST = sys.float_info.max

# s(L) at the steps 0 to 19 (MATH §3.1).
STEPS = [0, 5, 9, 13, 17, 20, 23, 25, 27, 29, 31, 33, 35, 37, 39, 41, 42, 43, 45, 46]

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/level_references.py with mpmath; do not edit.
# Each line: a law's name, its inputs, and its exact value as a double-double
# (high, low), every number the binary64 bits in hexadecimal:
#   curve level | envelope level | silent level (an envelope level at or below 0)
#   total envelope offset | gain level
"""

type Line = list[str | float]


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


def curve(level: Fraction) -> Fraction:
    """s(L): the steps, linear between them, 5L below 0 and 28 + L from 20 on."""
    if level >= 20:
        return 28 + level
    if level < 0:
        return 5 * level
    step = math.floor(level)
    above = 48 if step == 19 else STEPS[step + 1]
    return STEPS[step] + (level - step) * (above - STEPS[step])


def envelope_at(step: int) -> Fraction:
    """E at a step, 64 times the floor of half of s, as keptone-engine saturates it."""
    return saturated(Fraction(64 * math.floor(saturated(curve(Fraction(step))) / 2)))


def envelope(level: float) -> Fraction | mpmath.mpf | None:
    """E(L): linear between the steps as a whole, interpolated in the gain below 1."""
    if level >= 1:
        step = math.floor(level)
        below = envelope_at(step)
        return saturated(below + (Fraction(level) - step) * (envelope_at(step + 1) - below))
    if level > 0:
        return envelope_at(1) + 256 * mpmath.log(mpmath.mpf(level), 2)
    return None


def draw(randoms: Iterator[int], low: float, high: float) -> float:
    """An input: a real of [low, high] or one of any binade up to binary64's largest."""
    return uniform(randoms, low, high) if next(randoms) % 3 else binades(randoms, -60, 1023)


def lines() -> Iterator[Line]:
    """Every line of the file, as the law's name and its numbers."""
    mpmath.mp.prec = 400
    randoms = splitmix64(SEED)
    levels = [x for step in range(-5, 131) for x in (*neighbours(float(step), 1), step + 0.5)]
    levels += [2.0**-1074, 1e-300, 0.25, 0.999, 1e300, -1e300, LARGEST, -LARGEST]
    levels += [draw(randoms, -10.0, 130.0) for _ in range(RANDOM)]
    for level in levels:
        yield ["curve", level, *double_double(saturated(curve(Fraction(level))))]
        value = envelope(level)
        yield ["silent", level] if value is None else ["envelope", level, *double_double(value)]
    for _ in range(RANDOM):
        e, o = draw(randoms, -500.0, 5000.0), draw(randoms, -1000.0, 5000.0)
        yield ["total", e, o, *double_double(saturated(saturated(Fraction(e) + Fraction(o)) - 4256))]
    gains = [float(t) for t in range(-300, 301)] + [3840.0, 249_344.0, 249_345.0, 1e300, -258_048.0, -262_000.0]
    gains += [draw(randoms, -270_000.0, 260_000.0) for _ in range(RANDOM)]
    for t in gains:
        value = mpmath.power(2, min(mpmath.mpf(t), mpmath.mpf(249_344)) / 256 - 14)
        yield ["gain", t, *double_double(value)]


def main() -> None:
    """Writes the file."""
    out = [HEADER.rstrip("\n")]
    for name, *numbers in lines():
        out.append(" ".join([str(name), *(f"{to_bits(x):016x}" for x in numbers)]))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
