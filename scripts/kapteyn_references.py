#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of the Kapteyn coefficients for keptone-math's tests.

    python3 scripts/kapteyn_references.py

The feedback operator's output, sin Theta for Theta - beta sin Theta = Phi, has the Fourier series
sum over n >= 1 of a_n(beta) sin n Phi with a_n(beta) = 2 J_n(n beta)/(n beta) (docs/MATH.md §9.5). This
script computes a_n with mpmath's besselj at 40 digits, for n from 1 to 2000 and beta on a grid -- below
and up to feedback.series_beta_max's default of 0.8, between and on the points of keptone-math's tables,
and beyond it, at 0.9 and 0.95 -- and writes crates/keptone-math/tests/data/kapteyn.txt: on each line beta
and a_n as the 16 hexadecimal digits of their binary64, and n in decimal, for every a_n of size 10^-20 or
more; the tests take the others to be 0, which they are within 10^-20.
"""

import sys
from pathlib import Path

import mpmath

from binary64 import line

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-math" / "tests" / "data" / "kapteyn.txt"
BETAS = (0.01, 0.05, 0.1234, 0.3, 0.5, 0.6543, 0.7, 0.785, 0.7999, 0.8, 0.9, 0.95)
LARGEST = 2000
SMALLEST = mpmath.mpf(10) ** -20

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/kapteyn_references.py with mpmath; do not edit.
# beta, n (decimal), a_n = 2 J_n(n beta)/(n beta): binary64 bits in hexadecimal, a_n of size 1e-20 or more.
"""


def coefficient(n: int, beta: float) -> mpmath.mpf:
    """a_n(beta) = 2 J_n(n beta)/(n beta) for beta > 0."""
    b = mpmath.mpf(beta)
    return 2 * mpmath.besselj(n, n * b) / (n * b)


def main() -> int:
    """Write the references."""
    mpmath.mp.dps = 40
    lines = []
    for beta in BETAS:
        for n in range(1, LARGEST + 1):
            value = coefficient(n, beta)
            if abs(value) < SMALLEST:
                break
            hexadecimal = line([beta, float(value)]).split(" ")
            lines.append(f"{hexadecimal[0]} {n} {hexadecimal[1]}")
    OUTPUT.write_text(HEADER + "\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {len(lines)} references to {OUTPUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
