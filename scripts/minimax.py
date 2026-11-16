#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the coefficients of keptone-math's polynomials from Sollya's design of them.

    python3 scripts/minimax.py

Runs Sollya on scripts/minimax.sollya, which designs each polynomial with
fpminimax and certifies its error with supnorm (docs/MATH.md §12), and writes
crates/keptone-math/src/coefficients.rs: every coefficient as the bits of its
binary64, with the hexadecimal value Sollya printed beside it, and each
polynomial's certified bound in its documentation. The output is the same for
the same Sollya; the file says which one wrote it.
"""

import re
import shutil
import struct
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "minimax.sollya"
OUTPUT = ROOT / "crates" / "keptone-math" / "src" / "coefficients.rs"

# What each polynomial is, for its documentation, and the name of its constants.
DESCRIPTIONS = {
    "sin_2pi": ("SIN_2PI", "sin 2 pi t on |t| <= 1/8, odd: t (c1 + c3 t^2 + ... + c15 t^14)"),
    "cos_2pi": ("COS_2PI", "cos 2 pi t - 1 on |t| <= 1/8, even: t^2 (c2 + c4 t^2 + ... + c16 t^14)"),
    "exp2": ("EXP2", "2^r - 1 on |r| <= 1/2: r (c1 + c2 r + ... + c12 r^11)"),
    "sinc_prime": ("SINC_PRIME", "the derivative of sinc u on |u| < 1/2, odd: u (c1 + c3 u^2 + ... + c19 u^18)"),
    "sinc_taylor": (
        "SINC_TAYLOR",
        "sinc u - 1 on |u| < 2^-6, the Taylor polynomial: u^2 (c2 + c4 u^2 + ... + c12 u^10)",
    ),
    "pi": ("PI", "π"),
}

# The head of the module written, which names its copyright holder and license
# as every file of the repository does.
HEADER = """\
// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).
//
// Written by scripts/minimax.py from scripts/minimax.sollya; do not edit.
"""


@dataclass
class Polynomial:
    """A polynomial as Sollya printed it: coefficients by power, low parts, bound."""

    coefficients: dict[int, str] = field(default_factory=dict)
    lows: dict[int, str] = field(default_factory=dict)
    bound: tuple[str, str] | None = None


def bits(value: str) -> str:
    """The bits of a binary64 written as a hexadecimal float, grouped as rustfmt keeps them."""
    number = float.fromhex(value)
    if number.hex() != float.fromhex(number.hex()).hex():
        raise ValueError(f"{value} is not a binary64")
    raw = struct.unpack(">Q", struct.pack(">d", number))[0]
    digits = f"{raw:016x}"
    return "0x" + "_".join(digits[i : i + 4] for i in range(0, 16, 4))


def run_sollya() -> tuple[str, dict[str, Polynomial]]:
    """Sollya's version line and the polynomials it printed."""
    sollya = shutil.which("sollya")
    if sollya is None:
        raise SystemExit("error: sollya is not on PATH")
    banner = subprocess.run(  # noqa: S603 -- Sollya itself, found on PATH
        [sollya, "--version"], capture_output=True, text=True, check=False
    ).stdout
    found = re.search(r"sollya (\S+)", banner)
    version = f"Sollya {found.group(1)}" if found else "Sollya"
    result = subprocess.run(  # noqa: S603 -- this repository's own script
        [sollya, str(SCRIPT)], capture_output=True, text=True, check=False
    )
    if result.returncode != 0:
        raise SystemExit(f"error: sollya exited with {result.returncode}\n{result.stdout}{result.stderr}")
    polynomials: dict[str, Polynomial] = {}
    for line in result.stdout.splitlines():
        words = line.split()
        if not words or words[0] not in {"coefficient", "low", "bound"}:
            continue
        if len(words) != 4 or words[1] not in DESCRIPTIONS:
            raise SystemExit(f"error: unexpected line from sollya: {line}")
        polynomial = polynomials.setdefault(words[1], Polynomial())
        if words[0] == "bound":
            if words[3] == "error":
                raise SystemExit(f"error: supnorm could not bound {words[1]}")
            polynomial.bound = (words[2], words[3])
        elif words[0] == "coefficient":
            polynomial.coefficients[int(words[2])] = words[3]
        else:
            polynomial.lows[int(words[2])] = words[3]
    return version, polynomials


def short(bound: str) -> str:
    """A certified upper bound, rounded up to three significant digits."""
    mantissa, exponent = f"{float(bound):.2e}".split("e")
    up = float(mantissa) + 0.01 if float(f"{mantissa}e{exponent}") < float(bound) else float(mantissa)
    return f"{up:.2f}e{int(exponent)}"


def constant(name: str, value: str, doc: str) -> list[str]:
    """A documented binary64 constant."""
    return [f"/// {doc}", f"pub(crate) const {name}: f64 = f64::from_bits({bits(value)}); // {value}"]


def render(version: str, polynomials: dict[str, Polynomial]) -> str:
    """The Rust module."""
    lines = [
        f"// The bounds are those of {version}.",
        "",
        "//! The coefficients of the polynomials, as Sollya's fpminimax designed them in",
        "//! binary64, the leading one of each a double-double (high and low parts), and",
        "//! the error of each polynomial, with its coefficients as they are, as Sollya's",
        "//! supnorm certified it: an upper bound over the whole interval.",
        "",
    ]
    for key, polynomial in polynomials.items():
        name, description = DESCRIPTIONS[key]
        lines.append(f"// {description}.")
        if polynomial.bound is not None:
            kind, value = polynomial.bound
            lines.append(f"// Certified {kind} error ≤ {short(value)} ({value}).")
        powers = sorted(polynomial.coefficients)
        leading = powers[0] if polynomial.lows else None
        for power in powers:
            value = polynomial.coefficients[power]
            if power == leading:
                lines += constant(f"{name}_{power}_HI", value, f"The coefficient of power {power}, high part.")
                low = polynomial.lows[power]
                lines += constant(f"{name}_{power}_LO", low, f"The coefficient of power {power}, low part.")
            else:
                lines += constant(f"{name}_{power}", value, f"The coefficient of power {power}.")
        lines.append("")
    return HEADER + "\n".join(lines).rstrip() + "\n"


def main() -> int:
    """Run Sollya and write the module."""
    version, polynomials = run_sollya()
    missing = set(DESCRIPTIONS) - set(polynomials)
    if missing:
        raise SystemExit(f"error: sollya printed nothing for {sorted(missing)}")
    OUTPUT.write_text(render(version, polynomials), encoding="utf-8")
    print(f"wrote {OUTPUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
