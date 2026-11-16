#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Derive the expansion of ADAA-2 with sympy and check it with mpmath (docs/MATH.md §10.3).

    python3 scripts/adaa_derivatives.py

ADAA-2 is twice the second divided difference of F2(x) = -sin(2 pi x)/(4 pi^2). For a small
h = x_n - x_(n-2), keptone-math takes 2 dF2[a, b]/da + (h^2/12) d^3F2[a, b]/da^3 at a = (x_n + x_(n-2))/2
and b = x_(n-1), F2[a, b] = -cos pi (a + b) sinc(a - b)/(2 pi) being the first divided difference. This
script

1. differentiates that first difference with sinc kept as an unknown function S, so that the derivatives
   come out in S, S', S'' and S''' of a - b and in sin and cos of pi (a + b), and checks them against the
   forms that crates/keptone-math/src/adaa.rs evaluates;
2. checks with sympy that sinc satisfies u S'' + 2 S' + pi^2 u S = 0, and the form of S''' that follows,
   which crates/keptone-math/src/sinc.rs takes from |u| = 1/4 on;
3. derives the Taylor coefficients of f'' and f''' for f(z) = sin z / z, and checks them, as exact
   rationals, against the tables of sinc.rs;
4. checks with mpmath, at 60 digits and more, that the closed first difference is the divided difference,
   that the expansion's remainder stays below (2 pi)^4 h^4/5760 and reaches it, at a still phase of 1/4
   turn, and that |d^5 F2[a, b]/da^5| stays below (2 pi)^4/6.

It prints what it derives and finds, and fails if a check fails.
"""

import re
import sys
from fractions import Fraction
from pathlib import Path

import mpmath
import sympy as sp

from binary64 import splitmix64, uniform

ROOT = Path(__file__).resolve().parent.parent
SINC_RS = ROOT / "crates" / "keptone-math" / "src" / "sinc.rs"
mpmath.mp.dps = 60


def symbolic() -> bool:
    """Steps 1 and 2: the derivatives in S and its derivatives, and the equation of sinc."""
    a, b, u = sp.symbols("a b u", real=True)
    pi = sp.pi
    s = sp.Function("S")
    derivative = [s(u).diff(u, k).subs(u, a - b) for k in range(4)]
    first = -sp.cos(pi * (a + b)) * s(a - b) / (2 * pi)
    sine, cosine = sp.sin(pi * (a + b)), sp.cos(pi * (a + b))
    expected_first = sine * derivative[0] / 2 - cosine * derivative[1] / (2 * pi)
    expected_third = sine * (sp.Rational(3, 2) * derivative[2] - pi**2 / 2 * derivative[0]) + cosine * (
        3 * pi / 2 * derivative[1] - derivative[3] / (2 * pi)
    )
    once = sp.diff(first, a)
    thrice = sp.diff(first, a, 3)
    print("dF2[a,b]/da     =", sp.simplify(once))
    print("d^3F2[a,b]/da^3 =", sp.collect(sp.expand(thrice), [sine, cosine]))
    ok = sp.simplify(once - expected_first) == 0 and sp.simplify(thrice - expected_third) == 0
    print("the forms of adaa.rs:", "match" if ok else "DIFFER")
    sinc = sp.sin(pi * u) / (pi * u)
    s1, s2, s3 = (sp.diff(sinc, u, k) for k in (1, 2, 3))
    equation = sp.simplify(u * s2 + 2 * s1 + pi**2 * u * sinc) == 0
    third = sp.simplify(s3 - (-(3 * s2 + pi**2 * sinc) / u - pi**2 * s1)) == 0
    print("u S'' + 2 S' + pi^2 u S = 0:", equation, "; S''' = -(3 S'' + pi^2 S)/u - pi^2 S':", third)
    return ok and equation and third


def series() -> bool:
    """Step 3: the Taylor coefficients of f'' and f''', against the tables of sinc.rs."""
    z = sp.symbols("z")
    f = sp.sin(z) / z
    text = SINC_RS.read_text(encoding="utf-8")
    ok = True
    for name, order, offset in (("SECOND_SERIES", 2, 0), ("THIRD_SERIES", 3, 1)):
        expansion = sp.series(sp.diff(f, z, order), z, 0, 2 * 9 + offset).removeO()
        derived = [Fraction(str(expansion.coeff(z, 2 * j + offset))) for j in range(9)]
        table = re.search(name + r": \[f64; 9\] = \[(.*?)\];", text, re.DOTALL)
        if table is None:
            print(f"{name}: not found in sinc.rs")
            return False
        entries = re.findall(r"(-?)1\.0 / ([0-9_]+)\.0", table.group(1))
        written = [Fraction(-1 if sign else 1, int(n.replace("_", ""))) for sign, n in entries]
        print(f"f^({order}): {[str(c) for c in derived]}")
        print(f"  {name} of sinc.rs:", "the same" if written == derived else "DIFFERENT")
        ok = ok and written == derived
    return ok


def numeric() -> bool:
    """Step 4: the first difference, the remainder of the expansion, and the bound of the fifth derivative."""
    pi = mpmath.pi

    def f2(x: mpmath.mpf) -> mpmath.mpf:
        return -mpmath.sinpi(2 * x) / (4 * pi**2)

    def sinc(v: mpmath.mpf) -> mpmath.mpf:
        return mpmath.mpf(1) if v == 0 else mpmath.sinpi(v) / (pi * v)

    def first(p: mpmath.mpf, q: mpmath.mpf) -> mpmath.mpf:
        return -mpmath.cospi(p + q) * sinc(p - q) / (2 * pi)

    def derivatives(v: mpmath.mpf) -> list[mpmath.mpf]:
        return [mpmath.diff(sinc, v, k) for k in range(4)]

    randoms = splitmix64(0x61646161_44)
    worst_first = mpmath.mpf(0)
    for _ in range(50):
        p, q = mpmath.mpf(uniform(randoms, -2.0, 2.0)), mpmath.mpf(uniform(randoms, -2.0, 2.0))
        worst_first = max(worst_first, abs(first(p, q) - (f2(p) - f2(q)) / (p - q)))
    print(f"closed first difference against the divided one: {mpmath.nstr(worst_first, 3)}")
    limit = (2 * pi) ** 4 / 5760
    worst_ratio, at = mpmath.mpf(0), None
    cases = [(0.25, 0.0, h) for h in (2.0**-8, 2.0**-10)] + [(0.25, 0.3, 2.0**-10), (0.1, 0.0, 2.0**-10)]
    cases += [(uniform(randoms, -1.0, 1.0), uniform(randoms, -1.0, 1.0), 2.0**-10) for _ in range(30)]
    for case in cases:
        x, kink, h = (mpmath.mpf(v) for v in case)
        # x_n = x + h/2 + kink, x_(n-1) = x, x_(n-2) = x - h/2 + kink: a = x + kink, d = a - b = kink.
        xn, xm, xmm = x + h / 2 + kink, x, x - h / 2 + kink
        exact = 2 * (first(xn, xm) - first(xm, xmm)) / (xn - xmm)
        s0, s1, s2, s3 = derivatives(kink)
        sine, cosine = mpmath.sinpi(2 * x + kink), mpmath.cospi(2 * x + kink)
        once = sine * s0 - cosine * s1 / pi
        third = sine * (mpmath.mpf(3) / 2 * s2 - pi**2 / 2 * s0) + cosine * (3 * pi / 2 * s1 - s3 / (2 * pi))
        ratio = abs(once + h**2 / 12 * third - exact) / h**4
        if ratio > worst_ratio:
            worst_ratio, at = ratio, case
    print(f"remainder / h^4: at most {mpmath.nstr(worst_ratio, 6)} at (x, d, h) = {at}")
    print(f"  against (2 pi)^4/5760 = {mpmath.nstr(limit, 6)}")
    fifth = mpmath.mpf(0)
    for _ in range(40):
        a, b = mpmath.mpf(uniform(randoms, -1.0, 1.0)), mpmath.mpf(uniform(randoms, -1.0, 1.0))
        fifth = max(fifth, abs(mpmath.diff(lambda t, b=b: first(t, b), a, 5)))
    print(
        f"|d^5F2[a,b]/da^5|: at most {mpmath.nstr(fifth, 6)}, against (2 pi)^4/6 = {mpmath.nstr((2 * pi) ** 4 / 6, 6)}"
    )
    return (
        worst_first < mpmath.mpf(10) ** -50
        and limit * (1 - mpmath.mpf(10) ** -3) < worst_ratio <= limit
        and fifth <= (2 * pi) ** 4 / 6
    )


def main() -> int:
    """Run the checks."""
    results = [symbolic(), series(), numeric()]
    print("all checks pass" if all(results) else "A CHECK FAILED")
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
