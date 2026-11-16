#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Print the constants of the rounding analyses in keptone-math's documentation.

    python3 scripts/rounding_bounds.py

The documentation of each kernel of crates/keptone-math bounds its error to
first order in u = 2^-53: Horner's scheme with fused steps, in which a term is
rounded once for each step it passes through ([HIGHAM] §5.1), the roundings
around it, and the double-doubles, which keep their parts exact but for terms
of the order of u^2. Those bounds take a few numbers from the polynomials: how
large the tail of each is beside the result, how much Horner's steps can lose
relative to the value, and their products. This script computes them from the
coefficients in crates/keptone-math/src/coefficients.rs, each the largest over
4096 points of its interval and the interval's ends, where each of them is
largest, and the bounds that follow; the documentation rounds them up.
"""

import re
import struct
import sys
from pathlib import Path

import mpmath

ROOT = Path(__file__).resolve().parent.parent
COEFFICIENTS = ROOT / "crates" / "keptone-math" / "src" / "coefficients.rs"
POINTS = 4096
mpmath.mp.dps = 40
U = mpmath.mpf(2) ** -53

Coefficients = dict[str, mpmath.mpf]


def coefficients() -> Coefficients:
    """The constants of coefficients.rs, by name."""
    text = COEFFICIENTS.read_text(encoding="utf-8")
    found = {}
    for name, bits in re.findall(r"const (\w+): f64 = f64::from_bits\(0x([0-9a-f_]+)\)", text):
        found[name] = mpmath.mpf(struct.unpack("<d", struct.pack("<Q", int(bits.replace("_", ""), 16)))[0])
    return found


def horner(cs: list[mpmath.mpf], x: mpmath.mpf) -> tuple[mpmath.mpf, mpmath.mpf, mpmath.mpf]:
    """The polynomial's value at x; the bound of Horner's fused steps on its error, and the error a rounding of x
    by u brings, both in u relative to the value."""
    steps = len(cs) - 1
    value = sum(c * x**k for k, c in enumerate(cs))
    rounding = sum(abs(c) * abs(x) ** k * min(k + 1, steps) for k, c in enumerate(cs))
    slope = abs(sum(k * c * x**k for k, c in enumerate(cs)))
    return value, rounding / abs(value), slope / abs(value)


def grid(low: mpmath.mpf, high: mpmath.mpf) -> list[mpmath.mpf]:
    """POINTS + 1 evenly spaced points from low to high."""
    return [low + (high - low) * mpmath.mpf(i) / POINTS for i in range(POINTS + 1)]


def half_ulp(v: mpmath.mpf) -> mpmath.mpf:
    """Half the spacing of binary64 at |v|."""
    return mpmath.mpf(2) ** (mpmath.floor(mpmath.log(abs(v), 2)) - 53)


def show(what: str, value: mpmath.mpf) -> None:
    """One line of the report."""
    print(f"  {what}: {mpmath.nstr(value, 4)}")


def sine(c: Coefficients) -> tuple[mpmath.mpf, mpmath.mpf]:
    """sin2pi's kernel, c1 t + t^3 P(t^2) for |t| <= 1/8; its error in ulp and its relative error in u."""
    cs = [c[f"SIN_2PI_{k}"] for k in range(3, 16, 2)]
    c1 = c["SIN_2PI_1_HI"] + c["SIN_2PI_1_LO"]
    tail = steps = error = mpmath.mpf(0)
    for t in grid(mpmath.mpf(0), mpmath.mpf(1) / 8)[1:]:
        p, rounding, slope = horner(cs, t * t)
        ratio = abs(t**3 * p / (c1 * t + t**3 * p))
        tail, steps = max(tail, ratio), max(steps, rounding + slope)
        error = max(error, ratio * (rounding + slope + 3))
    print("sin 2 pi t, |t| <= 1/8:")
    show("the tail t^3 P beside the result, at most", tail)
    show("P from Horner's steps, the rounding of t^2 included, within (u)", steps)
    show("the tail, with t^2 and the two fused steps that add it, within (u)", steps + 3)
    show("the error of the tail beside the result, at most (u)", error)
    show("error, at most (ulp)", error + mpmath.mpf(0.5))
    show("relative error, at most (u)", error + 1)
    return error + mpmath.mpf(0.5), error + 1


def cosine(c: Coefficients) -> tuple[mpmath.mpf, mpmath.mpf]:
    """sin2pi's other kernel, 1 + c2 t^2 + t^4 Q(t^2) for |t| <= 1/8; its error in ulp and its relative error in u."""
    cs = [c[f"COS_2PI_{k}"] for k in range(4, 17, 2)]
    fourth = steps = error = mpmath.mpf(0)
    for t in grid(mpmath.mpf(0), mpmath.mpf(1) / 8):
        q, rounding, slope = horner(cs, t * t)
        size = abs(t**4 * q)
        fourth, steps = max(fourth, size), max(steps, rounding + slope)
        error = max(error, size * (rounding + slope + 4))
    additions = 2 * half_ulp(fourth) / U
    total = error + additions
    least = mpmath.cos(mpmath.pi / 4)
    print("cos 2 pi t, |t| <= 1/8:")
    show("t^4 Q, at most", fourth)
    show("Q from Horner's steps, the rounding of t^2 included, within (u)", steps)
    show("t^4 Q, with t^2 rounded and squared and the two products, within (u)", steps + 4)
    show("its error, at most (u)", error)
    show("the two additions below 2^-6, at most (u)", additions)
    show("before the final addition, at most (u)", total)
    show("error, the result being at least cos(pi/4), at most (ulp)", total + mpmath.mpf(0.5))
    show("relative error, at most (u)", (total + mpmath.mpf(0.5)) / least)
    return total + mpmath.mpf(0.5), (total + mpmath.mpf(0.5)) / least


def exponential(c: Coefficients) -> None:
    """exp2's kernel, 1 + c1 r + r^2 Q(r) for |r| <= 1/2, r exact."""
    cs = [c[f"EXP2_{k}"] for k in range(2, 13)]
    second = steps = error = mpmath.mpf(0)
    for r in grid(mpmath.mpf(-1) / 2, mpmath.mpf(1) / 2):
        q, rounding, _ = horner(cs, r)
        size = abs(r * r * q)
        second, steps = max(second, size), max(steps, rounding)
        error = max(error, size * (rounding + 2))
    bound = second * (steps + 2)
    total = bound + 2 * half_ulp(second) / U
    print("2^r, |r| <= 1/2:")
    show("r^2 Q, at most", second)
    show("Q from Horner's steps, within (u)", steps)
    show("r^2 Q, with r^2 and the product, within (u)", steps + 2)
    show("its error, at most (u; the product of the two above, which the documentation takes)", bound)
    show("its error, where it is largest (u)", error)
    show("with the two additions below 1/8, before the final addition, at most (u)", total)
    show("error below 1, at most (ulp)", total + mpmath.mpf(0.5))
    show("error from 1 on, at most (ulp)", total / 2 + mpmath.mpf(0.5))
    show("the error of 2^r at k = -1022, in the subnormal ulp", (total + mpmath.mpf(0.5)) / 2)
    show("error of a subnormal result, at most (ulp)", (total + mpmath.mpf(0.5)) / 2 + mpmath.mpf(0.5))


def sinc_series(c: Coefficients) -> None:
    """sinc's Taylor polynomial, 1 + u^2 P(u^2) for |u| < 2^-6."""
    cs = [c[f"SINC_TAYLOR_{k}"] for k in range(2, 13, 2)]
    size = error = mpmath.mpf(0)
    for v in grid(mpmath.mpf(0), mpmath.mpf(2) ** -6):
        p, rounding, slope = horner(cs, v * v)
        size = max(size, abs(v * v * p))
        error = max(error, abs(v * v * p) * (rounding + slope + 1))
    print("sinc u, |u| < 2^-6:")
    show("u^2 P, at most", size)
    show("its error, at most (u)", error)


def sinc_derivative(c: Coefficients) -> None:
    """sinc' 's polynomial, c1 u + u^3 R(u^2) for |u| < 1/2."""
    cs = [c[f"SINC_PRIME_{k}"] for k in range(3, 20, 2)]
    c1 = c["SINC_PRIME_1_HI"] + c["SINC_PRIME_1_LO"]
    tail = steps = error = mpmath.mpf(0)
    for v in grid(mpmath.mpf(0), mpmath.mpf(1) / 2)[1:]:
        r, rounding, slope = horner(cs, v * v)
        ratio = abs(v**3 * r / (c1 * v + v**3 * r))
        tail, steps = max(tail, ratio), max(steps, rounding + slope)
        error = max(error, ratio * (rounding + slope + 3))
    print("sinc' u, |u| < 1/2:")
    show("the tail u^3 R beside the result, at most", tail)
    show("R from Horner's steps, the rounding of u^2 included, within (u)", steps)
    show("the tail, with u^2 and the two fused steps that add it, within (u)", steps + 3)
    show("the error of the tail beside the result, at most (u)", error)
    show("error, at most (ulp)", error + mpmath.mpf(0.5))


def sinc_and_closed_form(sine_ulp: mpmath.mpf, sine_relative: mpmath.mpf) -> None:
    """sinc from |u| = 2^-6 on, and sinc' from 1/2 on, from the bounds of sin2pi and cos2pi."""
    sinc_ulp = sine_relative + mpmath.mpf(0.5)
    print("sinc u, |u| >= 2^-6:")
    show("the sine's relative error, at most (u)", sine_relative)
    show("error, at most (ulp)", sinc_ulp)

    def derivative(v: mpmath.mpf) -> mpmath.mpf:
        return (mpmath.cospi(v) - mpmath.sinpi(v) / (mpmath.pi * v)) / v

    peak = mpmath.findroot(lambda v: mpmath.diff(derivative, v), mpmath.mpf(0.66))
    sampled = max(abs(derivative(v)) for v in grid(mpmath.mpf(1) / 2, mpmath.mpf(8)))
    numerator = sine_ulp + sinc_ulp + 1
    print("sinc' u, |u| >= 1/2, absolute:")
    show("|sinc' u|, at most, at u", peak)
    show("|sinc' u|, at most", max(abs(derivative(peak)), sampled))
    show("|sinc u|, at most (2/pi)", 2 / mpmath.pi)
    show("the numerator, at most (2^-53)", numerator)
    show("error, at most (2^-53)", 2 * numerator + 1)


def main() -> int:
    """Print the report."""
    c = coefficients()
    sin_ulp, sin_relative = sine(c)
    cos_ulp, cos_relative = cosine(c)
    print("sin2pi and cos2pi, either kernel:")
    show("error, at most (ulp)", max(sin_ulp, cos_ulp))
    show("relative error, at most (u)", max(sin_relative, cos_relative))
    exponential(c)
    sinc_series(c)
    sinc_derivative(c)
    sinc_and_closed_form(max(sin_ulp, cos_ulp), max(sin_relative, cos_relative))
    return 0


if __name__ == "__main__":
    sys.exit(main())
