#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of ADAA-1, ADAA-2 and the derivatives of sinc for keptone-math's tests.

    python3 scripts/adaa_references.py

Computes with mpmath, the binary64 inputs taken as exact and sums of them formed exactly:

- ADAA-1 (docs/MATH.md §10.2): sinc(D) sin 2 pi (x + D/2), with x the phase at the previous sample and D the
  increment;
- ADAA-2 (§10.3): twice the second divided difference of F2(x) = -sin(2 pi x)/(4 pi^2) at x + D, x and x - P,
  P the increment before, as the difference of the first differences -cos pi (a + b) sinc(a - b)/(2 pi) over
  D + P, at a precision raised by the bits that difference cancels, and its limit, the derivative of the first
  difference, where D + P = 0;
- sinc'' and sinc''' (the expansion of §10.3), from their closed forms at a precision raised as u nears 0;

with mpmath's sinpi and cospi, which reduce their arguments exactly. The inputs are special points -- a still
phase, constant increments, a reversing phase, D + P on either side of the settings 2^-12, 2^-10 and 2^-8 of
adaa.tau2, large phases -- and random inputs from SplitMix64 with fixed seeds, so that the files are the same
every time. On each line are the inputs and the reference as a double-double, every number the 16 hexadecimal
digits of its binary64, in crates/keptone-math/tests/data/: adaa1.txt (x, D), adaa2.txt (x, D, P) and
sinc_derivatives.txt (u, sinc'' u, sinc''' u).
"""

import math
import sys
from pathlib import Path

import mpmath

from binary64 import binades, double_double, line, neighbours, splitmix64, uniform

ROOT = Path(__file__).resolve().parent.parent
DATA = ROOT / "crates" / "keptone-math" / "tests" / "data"
THRESHOLDS = (2.0**-12, 2.0**-10, 2.0**-8)
PHASES = (0.0, 0.1, 0.25, 0.375, -0.3, 0.5, 0.75, 1.0e6 + 0.125, -12.3456)
GUARD = 160

# The head of each file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/adaa_references.py with mpmath; do not edit.
"""


def span(*values: float) -> int:
    """The bits that hold every sum of the values exactly, and a guard."""
    exponents = [math.frexp(v)[1] for v in values if v != 0]
    if not exponents:
        return GUARD
    return max(exponents) - min(exponents) + 53 + GUARD


def sinc(v: mpmath.mpf) -> mpmath.mpf:
    """sinc v = sin(pi v)/(pi v), sinc 0 = 1."""
    return mpmath.mpf(1) if v == 0 else mpmath.sinpi(v) / (mpmath.pi * v)


def adaa1(x: float, delta: float) -> mpmath.mpf:
    """sinc(D) sin 2 pi (x + D/2) = sinc(D) sinpi(2x + D)."""
    with mpmath.workprec(span(x, delta)):
        xs, d = mpmath.mpf(x), mpmath.mpf(delta)
        return +(sinc(d) * mpmath.sinpi(2 * xs + d))


def adaa2(x: float, delta: float, previous: float) -> mpmath.mpf:
    """2 F2[x + D, x, x - P], or 2 dF2[a, b]/da at a = x + D where D + P = 0."""
    exact = span(x, delta, previous)
    with mpmath.workprec(exact):
        xs, d1, d2 = mpmath.mpf(x), mpmath.mpf(delta), mpmath.mpf(previous)
        h = d1 + d2
    if h == 0:
        d = d1
        cancelled = 2 * max(0, -math.frexp(delta)[1]) if delta != 0 else 0
        with mpmath.workprec(exact + cancelled):
            prime = mpmath.mpf(0) if d == 0 else (mpmath.cospi(d) - sinc(d)) / d
            return +(mpmath.sinpi(2 * xs + d) * sinc(d) - mpmath.cospi(2 * xs + d) * prime / mpmath.pi)
    with mpmath.workprec(exact + max(0, -int(mpmath.floor(mpmath.log(abs(h), 2))))):
        later = mpmath.cospi(2 * xs + d1) * sinc(d1)
        earlier = mpmath.cospi(2 * xs - d2) * sinc(d2)
        return +((earlier - later) / (mpmath.pi * h))


def sinc_derivatives(u: float) -> tuple[mpmath.mpf, mpmath.mpf]:
    """sinc'' u = pi^2 f''(pi u) and sinc''' u = pi^3 f'''(pi u), f(z) = sin z / z."""
    if u == 0:
        return -(mpmath.pi**2) / 3, mpmath.mpf(0)
    lost = 4 * max(0, -math.frexp(u)[1])
    with mpmath.workprec(GUARD + 53 + lost):
        v = mpmath.mpf(u)
        z = mpmath.pi * v
        sine, cosine = mpmath.sinpi(v), mpmath.cospi(v)
        second = (-(z**2) * sine - 2 * z * cosine + 2 * sine) / z**3
        third = (-(z**3) * cosine + 3 * z**2 * sine + 6 * z * cosine - 6 * sine) / z**4
        return +(mpmath.pi**2 * second), +(mpmath.pi**3 * third)


def first_order_inputs() -> list[tuple[float, float]]:
    """The special points of ADAA-1, then random ones: one phase in eight large."""
    increments = [0.0, 2.0**-1074, 1e-300, 2.0**-40, 2.0**-20, *neighbours(2.0**-6, 1), 0.01, 0.1, 0.25, 0.5]
    increments += [0.99, 1.0, 1.01, 1.5, 2.5, 10.3, 1000.37]
    special = [(x, s * d) for x in PHASES for d in increments for s in (1.0, -1.0)]
    randoms = splitmix64(0x61646161_31)
    out = []
    for _ in range(1500):
        large = next(randoms) % 8 == 0
        x = uniform(randoms, -1.0e6, 1.0e6) if large else uniform(randoms, -4.0, 4.0)
        out.append((x, binades(randoms, -40, 6)))
    return special + out


def second_order_inputs() -> list[tuple[float, float, float]]:
    """The special points of ADAA-2, then random ones, a third of them with D + P near a threshold."""
    special = []
    constants = [0.0, 2.0**-1074, 1e-300, 2.0**-40, 2.0**-20, 1e-3, 0.01, 0.1, 0.25, 0.5, 1.0, 1.5, 3.7]
    for t in THRESHOLDS:
        constants += neighbours(t / 2, 2)
    special += [(x, s * d, s * d) for x in PHASES for d in constants for s in (1.0, -1.0)]
    gaps = [0.0, 1e-300, 1e-12, 1e-6, 2.0**-9, 0.01]
    for t in THRESHOLDS:
        gaps += [t * (1 - 2.0**-50), t, t * (1 + 2.0**-50)]
    for q in (2.0**-20, 0.3, 0.7, 1.25, 4.5):
        special += [(x, q, -q + e) for x in (0.1, -0.3, 1.0e6 + 0.125) for e in gaps]
    special += [(x, 2.0**-12, 3 * 2.0**-12) for x in PHASES] + [(x, 0.1, 0.3) for x in PHASES]
    special += [(x, 0.3, -0.1) for x in PHASES] + [(x, -0.25, 0.75) for x in PHASES]
    randoms = splitmix64(0x61646161_32)
    out = []
    for k in range(2800):
        large = next(randoms) % 8 == 0
        x = uniform(randoms, -1.0e6, 1.0e6) if large else uniform(randoms, -2.0, 2.0)
        delta = binades(randoms, -30, 2)
        if k % 3 == 0:
            out.append((x, delta, binades(randoms, -30, 2)))
            continue
        if k % 3 == 1:
            t = THRESHOLDS[next(randoms) % 3]
            h = t * 2.0 ** uniform(randoms, -2.0, 2.0)
        else:
            h = abs(binades(randoms, -60, -14))
        h = -h if next(randoms) & 1 else h
        out.append((x, delta, h - delta))
    return special + out


def derivative_inputs() -> list[float]:
    """The special points of sinc'' and sinc''', the threshold 1/4 of their series among them, then random ones."""
    special = [0.0, 2.0**-1074, 1e-300, 1e-20, 1e-10, 1e-3, 0.1, 0.2, *neighbours(0.25, 2), 0.3, 0.5, 0.7]
    special += [1.0, 1.4303, 2.5, 7.0, 100.0, 1.0e6 + 0.5]
    randoms = splitmix64(0x61646161_33)
    return special + [-u for u in special] + [binades(randoms, -30, 7) for _ in range(400)]


def write(name: str, header: str, lines: list[str]) -> None:
    """One reference file, its header naming the copyright holder and license as every file does."""
    (DATA / name).write_text(HEADER + f"# {header}\n" + "\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {len(lines)} references to {(DATA / name).relative_to(ROOT)}")


def main() -> int:
    """Write the three files."""
    DATA.mkdir(parents=True, exist_ok=True)
    lines = [line([x, d, *double_double(adaa1(x, d))]) for x, d in first_order_inputs()]
    write("adaa1.txt", "x, D, ADAA-1 (high, low): binary64 bits in hexadecimal.", lines)
    lines = [line([x, d, p, *double_double(adaa2(x, d, p))]) for x, d, p in second_order_inputs()]
    write("adaa2.txt", "x, D, P, ADAA-2 (high, low): binary64 bits in hexadecimal.", lines)
    lines = []
    for u in derivative_inputs():
        second, third = sinc_derivatives(u)
        lines.append(line([u, *double_double(second), *double_double(third)]))
    write(
        "sinc_derivatives.txt", "u, sinc'' u (high, low), sinc''' u (high, low): binary64 bits in hexadecimal.", lines
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
