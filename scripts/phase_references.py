#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of an operator's phase increments for keptone-engine's tests.

    python3 scripts/phase_references.py

Computes the exact increments of docs/MATH.md §2.5 at binary64 inputs,
r times the integral of 2^p over a piece, the pitch held at 1023 octaves: where
the pitch moves linearly, in closed form with mpmath at 600 bits, at slopes
from none and barely any to steep, and at pitches from far below 1023 octaves
to across and above them; and where it moves as a sine, as an LFO moves it, by
mpmath's quadrature at 50 digits, with the pitches, exact and rounded once, at
the instants of the 3-point Gauss-Legendre rule; and those instants. A value
beyond binary64's largest finite number is taken at it (MATH §0.5). It writes
crates/keptone-engine/tests/data/phase.txt: on each line the kind, its inputs
and its exact value as a double-double, every number as the 16 hexadecimal
digits of its binary64. The inputs are special points and draws of SplitMix64
from a fixed seed, so that the file is the same every time.
"""

import sys
from collections.abc import Iterator
from pathlib import Path

import mpmath

from binary64 import binades, splitmix64, to_bits, uniform

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "phase.txt"
SEED = 0x7068617365
LINEAR = 1500
GAUSS = 500
LARGEST = sys.float_info.max
HIGHEST = mpmath.mpf(1023)
RATES = [8000.0, 22050.0, 44100.0, 48000.0, 96000.0, 192000.0, 768000.0]

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/phase_references.py with mpmath; do not edit.
# Each line: a kind, its inputs, and its exact value as a double-double (high,
# low), every number the binary64 bits in hexadecimal:
#   linear ratio pitch slope length
#   gauss ratio length lfo_hz depth pitch phase p1 p2 p3 (p = pitch +
#     depth sin 2 pi (lfo_hz t + phase), p1 to p3 at the rule's instants)
#   instants length, then the three instants, each a double-double
"""

type Line = list[str | float]


def double_double(value: mpmath.mpf) -> list[float]:
    """value as hi + lo, hi the nearest binary64 and lo the nearest to the rest,
    taken at binary64's largest finite number beyond it."""
    if abs(value) > LARGEST:
        return [LARGEST if value > 0 else -LARGEST, 0.0]
    hi = float(value)
    return [hi, float(value - mpmath.mpf(hi))]


def rising(pitch: mpmath.mpf, slope: mpmath.mpf, length: mpmath.mpf) -> mpmath.mpf:
    """The integral of 2^(pitch + slope t) over [0, length], exactly."""
    if slope == 0:
        return mpmath.power(2, pitch) * length
    rate = slope * mpmath.log(2)
    return mpmath.power(2, pitch) * mpmath.expm1(rate * length) / rate


def linear(ratio: float, pitch: float, slope: float, length: float) -> mpmath.mpf:
    """r times the integral of 2^min(p + b t, 1023) over [0, h], exactly."""
    p, b, h = mpmath.mpf(pitch), mpmath.mpf(slope), mpmath.mpf(length)
    end = p + b * h
    if p <= HIGHEST and end <= HIGHEST:
        value = rising(p, b, h)
    elif p >= HIGHEST and end >= HIGHEST:
        value = mpmath.power(2, HIGHEST) * h
    else:
        crossing = (HIGHEST - p) / b
        if b > 0:
            value = rising(p, b, crossing) + mpmath.power(2, HIGHEST) * (h - crossing)
        else:
            value = mpmath.power(2, HIGHEST) * crossing + rising(HIGHEST, b, h - crossing)
    return mpmath.mpf(ratio) * value


def lengths(randoms: Iterator[int]) -> float:
    """A piece's length: a sample at one of the rates, a part of one, or any."""
    kind = next(randoms) % 4
    if kind < 2:
        return 1.0 / RATES[next(randoms) % len(RATES)]
    if kind == 2:
        return uniform(randoms, 0.0, 1.0) / RATES[next(randoms) % len(RATES)]
    return abs(binades(randoms, -60, 3))


def linears(randoms: Iterator[int]) -> Iterator[Line]:
    """Linear pieces: slopes of none, barely any, a pitch envelope's and steep
    ones, at pitches on the audible scale, far below it and about 1023; first a
    few by hand, among them one whose exponents' integers, rounded to even, sum
    to 1024 although the pitch reaches only 1023."""
    for ratio, pitch, slope, length in [
        (1.0, 1021.5, 1.5, 1.0),
        (1.0, 0.0, 1.0, 1.0),
        (1.0, 8.0, -12.0, 1.0 / 48000.0),
        (2.0, 1022.0, 2.0, 1.0),
        (1.0, 1024.0, -2.0, 1.0),
        (1e-300, 1023.0, 0.0, 1.0),
    ]:
        yield ["linear", ratio, pitch, slope, length, *double_double(linear(ratio, pitch, slope, length))]
    for _ in range(LINEAR):
        ratio = 1.0 if next(randoms) % 4 == 0 else uniform(randoms, 0.5, 62.0)
        if next(randoms) % 8 == 0:
            ratio = binades(randoms, -80, 80)
        kind = next(randoms) % 6
        pitch = [
            uniform(randoms, 0.0, 15.0),
            uniform(randoms, 0.0, 15.0),
            uniform(randoms, -1100.0, 1100.0),
            uniform(randoms, 1020.0, 1026.0),
            uniform(randoms, 0.0, 15.0),
            binades(randoms, -60, 10),
        ][kind]
        length = lengths(randoms)
        slope = [
            0.0,
            binades(randoms, -80, -30),
            uniform(randoms, -100.0, 100.0),
            binades(randoms, 0, 20) * 1000.0,
            binades(randoms, -10, 30),
            binades(randoms, -20, 1000),
        ][next(randoms) % 6]
        if kind == 3 and next(randoms) % 2:
            # Across 1023 octaves within the piece, from either side.
            slope = (1023.0 - pitch) * 2.0 / length
        yield ["linear", ratio, pitch, slope, length, *double_double(linear(ratio, pitch, slope, length))]


def gausses(randoms: Iterator[int]) -> Iterator[Line]:
    """Sine pieces, an LFO's speeds on its scale and beyond it, at depths of up
    to one octave and beyond, the pitches at the rule's exact instants."""
    node = mpmath.sqrt(mpmath.mpf(3) / 5)
    speeds = [0.0625, 1.0, 5.0, 20.0, 49.2]
    for _ in range(GAUSS):
        ratio = 1.0 if next(randoms) % 4 == 0 else uniform(randoms, 0.5, 62.0)
        length = 1.0 / RATES[next(randoms) % len(RATES)]
        speed = speeds[next(randoms) % len(speeds)] if next(randoms) % 4 else uniform(randoms, 50.0, 2000.0)
        depth = uniform(randoms, 0.0, 1.0) if next(randoms) % 4 else uniform(randoms, 1.0, 4.0)
        pitch, phase = uniform(randoms, 0.0, 15.0), uniform(randoms, 0.0, 1.0)

        def p(
            t: mpmath.mpf, depth: float = depth, speed: float = speed, pitch: float = pitch, phase: float = phase
        ) -> mpmath.mpf:
            return mpmath.mpf(pitch) + mpmath.mpf(depth) * mpmath.sinpi(2 * (mpmath.mpf(speed) * t + phase))

        h = mpmath.mpf(length)
        instants = [h / 2 * (1 - node), h / 2, h / 2 * (1 + node)]
        pitches = [float(p(t)) for t in instants]
        with mpmath.workdps(50):
            value = mpmath.mpf(ratio) * mpmath.quad(lambda t: mpmath.power(2, p(t)), [0, h])
        yield ["gauss", ratio, length, speed, depth, pitch, phase, *pitches, *double_double(value)]


def instants(randoms: Iterator[int]) -> Iterator[Line]:
    """The rule's instants, exactly."""
    node = mpmath.sqrt(mpmath.mpf(3) / 5)
    for rate in RATES:
        for length in [1.0 / rate, 0.5 / rate, abs(binades(randoms, -60, 3))]:
            h = mpmath.mpf(length)
            exact = [h / 2 * (1 - node), h / 2, h / 2 * (1 + node)]
            yield ["instants", length, *(x for t in exact for x in double_double(t))]


def main() -> None:
    """Writes the file."""
    mpmath.mp.prec = 600
    randoms = splitmix64(SEED)
    out = [HEADER.rstrip("\n")]
    for kind in (linears, gausses, instants):
        for name, *numbers in kind(randoms):
            out.append(" ".join([str(name), *(f"{to_bits(x):016x}" for x in numbers)]))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
