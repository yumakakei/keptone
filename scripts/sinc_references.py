#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of sinc and its derivative for keptone-math's tests.

    python3 scripts/sinc_references.py

Computes sinc u = sin(pi u)/(pi u) and sinc' u = (cos pi u - sinc u)/u with
mpmath's sinpi and cospi, which reduce the argument exactly, so that sinc is
exactly 0 at the integers however large, and at a precision raised as u nears
0 so that the cancellation in sinc' leaves at least 40 digits, for the special
points of docs/MATH.md §12.2 (the
thresholds 2^-6 and 1/2, integers and half-integers, the zeros of sinc') and
for random inputs, and writes crates/keptone-math/tests/data/sinc.txt: on each
line the input and both references as double-doubles, every number as the 16
hexadecimal digits of its binary64. The inputs come from SplitMix64 with a
fixed seed, so the file is the same every time.
"""

import math
import sys
from pathlib import Path

import mpmath

from binary64 import double_double, from_bits, line, neighbours, splitmix64

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-math" / "tests" / "data" / "sinc.txt"
SEED = 0x73696E63
RANDOM = 5000

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/sinc_references.py with mpmath; do not edit.
# u, sinc u (high, low), sinc' u (high, low): binary64 bits in hexadecimal.
"""


def inputs() -> list[float]:
    """The special points, then the random inputs: three quarters of them
    spread over the binades from 2^-40 to 2^12, a quarter uniform on (-4, 4)."""
    special = [0.0, 2.0**-1074, 2.0**-1022, 2.0**-60, 2.0**-30, 1e-10, 0.1, 0.3, 0.75, 1.5, 7.25, 1e3, 1e6, 2.0**40]
    special += neighbours(2.0**-6, 4) + neighbours(0.5, 4)
    for n in range(1, 17):
        special += [*neighbours(float(n), 2), n + 0.5, n - 0.25, n + 0.25]
    # The first zeros of sinc', where it changes sign: the roots of
    # sin(pi u) - pi u cos(pi u), which has no poles, unlike tan(pi u) - pi u.
    for k in range(1, 6):
        with mpmath.workdps(40):
            zero = mpmath.findroot(
                lambda u: mpmath.sin(mpmath.pi * u) - mpmath.pi * u * mpmath.cos(mpmath.pi * u), k + 0.45
            )
        special += neighbours(float(zero), 3)
    special += [2.0**52 - 0.5, 2.0**52, 2.0**53]
    randoms = splitmix64(SEED)
    out = []
    for _ in range(RANDOM):
        r = next(randoms)
        if r % 4 == 3:
            u = (from_bits(0x3FF0000000000000 | (next(randoms) >> 12)) - 1.0) * 8.0 - 4.0
        else:
            exponent = 1023 - 40 + next(randoms) % 53
            u = from_bits((exponent << 52) | (next(randoms) & ((1 << 52) - 1)))
            u = -u if r & 8 else u
        out.append(u)
    return special + [-x for x in special] + out


def references(u: float) -> tuple[mpmath.mpf, mpmath.mpf]:
    """sinc u and sinc' u to well over 40 digits."""
    digits = 60 + max(0, 2 * int(-math.log10(abs(u)))) if u != 0 else 60
    with mpmath.workdps(digits):
        if u == 0:
            return mpmath.mpf(1), mpmath.mpf(0)
        x = mpmath.mpf(u)
        sinc = mpmath.sinpi(x) / (mpmath.pi * x)
        derivative = (mpmath.cospi(x) - sinc) / x
        return +sinc, +derivative


def main() -> int:
    """Write the references."""
    lines = []
    for u in inputs():
        sinc, derivative = references(u)
        numbers = [u, *double_double(sinc), *double_double(derivative)]
        lines.append(line(numbers))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(HEADER + "\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {len(lines)} references to {OUTPUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
