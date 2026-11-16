# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""What the scripts that write references share: the bits of binary64 numbers,
SplitMix64, as the Rust tests have it, and double-doubles."""

import math
import struct
from collections.abc import Iterator

import mpmath

MASK = (1 << 64) - 1


def splitmix64(state: int) -> Iterator[int]:
    """SplitMix64, as the Rust tests have it."""
    while True:
        state = (state + 0x9E3779B97F4A7C15) & MASK
        z = state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
        yield z ^ (z >> 31)


def from_bits(bits: int) -> float:
    """The binary64 of these bits."""
    return struct.unpack("<d", struct.pack("<Q", bits))[0]


def to_bits(x: float) -> int:
    """The bits of a binary64."""
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def neighbours(x: float, k: int) -> list[float]:
    """x and the k binary64 on either side of it."""
    out, up, down = [x], x, x
    for _ in range(k):
        up, down = math.nextafter(up, math.inf), math.nextafter(down, -math.inf)
        out += [up, down]
    return out


def double_double(value: mpmath.mpf) -> tuple[float, float]:
    """value as hi + lo, hi the nearest binary64 and lo the nearest to the rest."""
    hi = float(value)
    lo = float(value - mpmath.mpf(hi))
    return hi, lo


def line(numbers: list[float]) -> str:
    """One line of a reference file: each number as the 16 hexadecimal digits of its bits."""
    return " ".join(f"{to_bits(x):016x}" for x in numbers)


def binades(randoms: Iterator[int], low: int, high: int) -> float:
    """A binary64 of random sign whose binade, 2^low to 2^high, and mantissa are uniform."""
    exponent = 1023 + low + next(randoms) % (high - low + 1)
    magnitude = from_bits((exponent << 52) | (next(randoms) & ((1 << 52) - 1)))
    return -magnitude if next(randoms) & 1 else magnitude


def uniform(randoms: Iterator[int], low: float, high: float) -> float:
    """A binary64 uniform on [low, high), on the grid of 2^-52 of the interval."""
    return low + (high - low) * (from_bits(0x3FF0000000000000 | (next(randoms) >> 12)) - 1.0)
