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

import math
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


def sinc_derivative(c: Coefficients) -> mpmath.mpf:
    """sinc' 's polynomial, c1 u + u^3 R(u^2) for |u| < 1/2; its error in ulp."""
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
    return error + mpmath.mpf(0.5)


def sinc_and_closed_form(sine_ulp: mpmath.mpf, sine_relative: mpmath.mpf) -> tuple[mpmath.mpf, mpmath.mpf]:
    """sinc from |u| = 2^-6 on, and sinc' from 1/2 on, from the bounds of sin2pi and cos2pi; sinc's relative error
    in u, its last rounding included, and the absolute bound of sinc' in 2^-53."""
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
    return sine_relative + 1, 2 * numerator + 1


def ulps(v: mpmath.mpf) -> mpmath.mpf:
    """The spacing of binary64 at |v|, in u."""
    return mpmath.mpf(2) ** (mpmath.floor(mpmath.log(abs(v), 2)) - 52) / U


def sinc_values(v: mpmath.mpf) -> tuple[mpmath.mpf, mpmath.mpf, mpmath.mpf]:
    """sinc v, sinc' v and sinc'' v for v > 0, sinc'' from u sinc'' + 2 sinc' + pi^2 u sinc = 0."""
    s0 = mpmath.sinpi(v) / (mpmath.pi * v)
    s1 = (mpmath.cospi(v) - s0) / v
    return s0, s1, -(mpmath.pi**2) * s0 - 2 * s1 / v


def sinc_higher(sinc_relative: mpmath.mpf, prime_ulp: mpmath.mpf, prime_absolute: mpmath.mpf) -> None:
    """sinc'' and sinc''' from |u| = 1/4 on, where sinc_derivatives takes the equation and its derivative."""
    pi2 = mpmath.pi**2
    second = third = mpmath.mpf(0)
    for v in grid(mpmath.mpf(1) / 4, mpmath.mpf(64)):
        s0, s1, s2 = sinc_values(v)
        e1 = prime_ulp * ulps(s1) if v < mpmath.mpf(1) / 2 else prime_absolute
        # sinc'' = -pi^2 sinc - 2 sinc'/u: the parts' errors, and three roundings at most of the size of the parts.
        e2 = pi2 * sinc_relative * abs(s0) + 2 * e1 / v + 3 * (pi2 * abs(s0) + 2 * abs(s1) / v)
        # sinc''' = -(3 sinc'' + pi^2 sinc)/u - pi^2 sinc': likewise.
        size = (3 * abs(s2) + pi2 * abs(s0)) / v + pi2 * abs(s1)
        e3 = (3 * e2 + pi2 * sinc_relative * abs(s0)) / v + pi2 * e1 + 4 * size
        second, third = max(second, e2), max(third, e3)
    print("sinc'' and sinc''', |u| >= 1/4, absolute:")
    show("sinc'', at most (u)", second)
    show("sinc'', at most", second * U)
    show("sinc''', at most (u)", third)
    show("sinc''', at most", third * U)


def adaa(
    sine_relative: mpmath.mpf, sinc_relative: mpmath.mpf, prime_ulp: mpmath.mpf, prime_absolute: mpmath.mpf
) -> None:
    """ADAA-1 and ADAA-2 (keptone-math's adaa), from the bounds of the functions they take."""
    pi = mpmath.pi
    product = sinc_relative + sine_relative + 1
    far = max(abs(sinc_values(v)[0]) * (product + pi * ulps(mpmath.mpf(0.5) + v / 2)) for v in grid(1, 1024))
    print("ADAA-1, absolute:")
    show("sinc, sin2pi and their product, at most (u)", product)
    show("with the midpoint's rounding, |D| < 1, at most (u)", product + pi)
    show("from |D| = 1 on, at most (u)", far)
    numerator = product + (product - 1) + 2 * pi
    relative = 4 + abs(mpmath.mpf(math.pi) - pi) / pi / U
    tau2 = mpmath.mpf(2) ** -10
    print("ADAA-2's quotient, absolute:")
    show("the difference of the two products, at most (u)", numerator)
    show("over pi h: the coefficient of u/|h|", numerator / pi)
    show("h, pi, the product, the division and the difference, relative, at most (u)", relative)
    show("at |h| = 2^-10, at most", numerator / pi * U / tau2 + relative * U)
    remainder = (2 * pi) ** 4 / 5760
    terms = {"d": 0, "m": 0, "trig": 0, "sinc": 0, "prime": 0, "operations": 0, "total": 0}
    for v in grid(mpmath.mpf(2) ** -12, mpmath.mpf(64)):
        s0, s1, s2 = sinc_values(v)
        part = {
            "d": v * (pi * abs(s0) + 2 * abs(s1) + abs(s2) / pi),
            "m": pi * ulps(mpmath.mpf(0.5) + v / 2) * (abs(s0) + abs(s1) / pi),
            "trig": sine_relative * (abs(s0) + abs(s1) / pi),
            "sinc": sinc_relative * abs(s0),
            "prime": (prime_ulp * ulps(s1) if v < mpmath.mpf(1) / 2 else prime_absolute) / pi,
            "operations": abs(s1) * (2 + relative - 4) / pi + abs(s0) + abs(s1) / pi,
        }
        part["total"] = sum(part.values())
        for name, value in part.items():
            terms[name] = max(terms[name], value)
    print("ADAA-2's expansion, absolute:")
    show("the remainder: the coefficient of h^4, (2 pi)^4/5760", remainder)
    show("the term in h^2: the coefficient of h^2, pi^2/12", pi**2 / 12)
    for name in ("d", "m", "trig", "sinc", "prime", "operations"):
        show(f"the rounding of the first term, from {name}, at most (u)", terms[name])
    show("the rounding of the first term, at most (u)", terms["total"])
    show("at |h| = 2^-10, at most", remainder * tau2**4 + terms["total"] * U)


def binary64(text: str) -> mpmath.mpf:
    """A binary64 written as a hexadecimal float, as mpmath's."""
    return mpmath.mpf(float.fromhex(text))


def ulp_in_u(v: mpmath.mpf) -> mpmath.mpf:
    """The spacing of binary64 at |v|, in u, for v normal."""
    return 2 * mpmath.mpf(2) ** mpmath.floor(mpmath.log(abs(v), 2))


def kernel_bound(cs: list[mpmath.mpf], c0: mpmath.mpf, c1: mpmath.mpf, points: list[mpmath.mpf]) -> tuple:
    """A kernel t (c0 + c1 t + t^2 Q(t)), c0 + c1 t a double-double and t^2 Q rounded: the largest |t^2 Q| beside
    the sum, the least sum, Horner's bound on Q, and the error of the sum relative to it (u): t^2 Q within Horner's
    bound and 2u (t^2 and the product), and the one addition that brings it in, u."""
    ratio = steps = relative = mpmath.mpf(0)
    least = mpmath.inf
    for t in points:
        q, rounding, _ = horner(cs, t)
        second = abs(t * t * q)
        total = abs(c0 + c1 * t + t * t * q)
        least = min(least, total)
        ratio, steps = max(ratio, second / total), max(steps, rounding)
        relative = max(relative, second * (rounding + 3) / total)
    return ratio, least, steps, relative


def logarithm(c: Coefficients) -> None:
    """log2's kernel, t (c0 + c1 t + t^2 Q(t)) for fl(sqrt 2)/2 - 1 <= t < fl(sqrt 2) - 1, t exact."""
    cs = [c[f"LOG2_{k}"] for k in range(2, 23)]
    c0, c1 = c["LOG2_0_HI"] + c["LOG2_0_LO"], c["LOG2_1_HI"] + c["LOG2_1_LO"]
    root = binary64("0x1.6a09e667f3bcdp0")
    ratio, least, steps, relative = kernel_bound(cs, c0, c1, grid(root / 2 - 1, root - 1))
    approximation = mpmath.mpf("2.19e-19") / U
    kernel = relative + approximation
    print("log2(1 + t), fl(sqrt 2)/2 - 1 <= t < fl(sqrt 2) - 1:")
    show("t^2 Q beside the sum, at most", ratio)
    show("the sum, at least", least)
    show("Q from Horner's steps, within (u)", steps)
    show("the sum, within (u, relative)", relative)
    show("with the polynomial's own bound (u, relative)", kernel)
    # The result is the kernel's double-double rounded once (k = 0), or k plus it, summed exactly but for terms of
    # the order of u^2 and rounded once. |log2(1 + t)| is at most log2(fl(sqrt 2)), a little above 1/2, and
    # |k + log2(1 + t)| at least 1 - log2(fl(sqrt 2)), a little below: the kernel's error is at most its own
    # relative error times |log2(1 + t)| u, beside an ulp of the result of at least u/2 there.
    top = mpmath.log(root, 2)
    show("|log2(1 + t)|, at most", top)
    show("log2 and log2p1, error at most (ulp)", kernel * max(1, top / (ulp_in_u(1 - top) / 1)) + mpmath.mpf(0.5))


def exponential_minus_one(c: Coefficients) -> None:
    """exp2m1's kernel, r (c0 + c1 r + r^2 Q(r)) for |r| <= 1/2, r exact."""
    cs = [c[f"EXP2M1_{k}"] for k in range(2, 12)]
    c0, c1 = c["EXP2M1_0_HI"] + c["EXP2M1_0_LO"], c["EXP2M1_1_HI"] + c["EXP2M1_1_LO"]
    ratio, least, steps, relative = kernel_bound(cs, c0, c1, grid(mpmath.mpf(-1) / 2, mpmath.mpf(1) / 2))
    approximation = mpmath.mpf("2.41e-19") / U
    kernel = relative + approximation
    print("2^r - 1, |r| <= 1/2:")
    show("r^2 Q beside the sum, at most", ratio)
    show("the sum, at least", least)
    show("Q from Horner's steps, within (u)", steps)
    show("the sum, within (u, relative)", relative)
    show("with the polynomial's own bound (u, relative)", kernel)
    show("for |z| <= 1/2, error at most (ulp)", kernel + mpmath.mpf(0.5))
    # Beyond, the result is (2^k - 1) + 2^k D, D = 2^r - 1, summed exactly but for terms of the order of u^2 and
    # rounded once: D's error is at most its relative error times 2^k |D| u, beside an ulp of the result.
    worst, at = mpmath.mpf(0), None
    for k in range(-54, 61):
        for r in grid(mpmath.mpf(-1) / 2, mpmath.mpf(1) / 2):
            if k == 0 or (k == 1 and r == mpmath.mpf(-1) / 2) or (k == -1 and r == mpmath.mpf(1) / 2):
                continue
            d = mpmath.power(2, r) - 1
            result = mpmath.power(2, k) * (1 + d) - 1
            ratio_here = mpmath.power(2, k) * abs(d) / ulp_in_u(result)
            if ratio_here > worst:
                worst, at = ratio_here, (k, r)
    show(f"2^k |D| beside an ulp of the result, at most (u; at k = {at[0]}, r = {mpmath.nstr(at[1], 3)})", worst)
    show("beyond, error at most (ulp)", kernel * worst + mpmath.mpf(0.5))


def main() -> int:
    """Print the report."""
    c = coefficients()
    sin_ulp, sin_relative = sine(c)
    cos_ulp, cos_relative = cosine(c)
    print("sin2pi and cos2pi, either kernel:")
    show("error, at most (ulp)", max(sin_ulp, cos_ulp))
    show("relative error, at most (u)", max(sin_relative, cos_relative))
    exponential(c)
    exponential_minus_one(c)
    logarithm(c)
    sinc_series(c)
    prime_ulp = sinc_derivative(c)
    sine_relative = max(sin_relative, cos_relative)
    sinc_relative, prime_absolute = sinc_and_closed_form(max(sin_ulp, cos_ulp), sine_relative)
    sinc_higher(sinc_relative, prime_ulp, prime_absolute)
    adaa(sine_relative, sinc_relative, prime_ulp, prime_absolute)
    return 0


if __name__ == "__main__":
    sys.exit(main())
