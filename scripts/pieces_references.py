#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of the increments over a sample's pieces for keptone-engine's tests.

    python3 scripts/pieces_references.py

Computes the exact increments of docs/MATH.md §2.5 over pieces of every kind that keptone-engine's pieces module
integrates, at binary64 inputs: r times the integral of 2^(c + lambda) over the piece, lambda the voice's moving part
of the pitch, the pitch envelope's line, a glide toward its target and the LFO's pitch, A times l, its depth A linear
and its wave l constant, linear or a sine, in the ratio mode, or the LFO's pitch alone in the fixed mode; by mpmath's
quadrature at 40 digits, the piece split into enough intervals for it, and in closed form where lambda is linear.
Beside each it writes the bound within which keptone-engine's increment lies, from the errors that its functions
document: the closed form's 12u and the rule's 8u, the rule's truncation below 2^-60 of each part's integral, the
rounding of the parts' instants, and the errors of the pitches that the pieces give the closed form and the rule; and
the fewest equal parts on which the rule's condition of MATH §2.5 holds, its bounds exact at the binary64 numbers. It
writes crates/keptone-engine/tests/data/pieces.txt: on each line the wave's kind, the piece's numbers, the operator's,
the exact value as a double-double and the bound, every number as the 16 hexadecimal digits of its binary64. The
inputs are special points and draws of SplitMix64 from a fixed seed, so that the file is the same every time.
"""

import math
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from pathlib import Path

import mpmath

from binary64 import binades, splitmix64, to_bits, uniform

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "pieces.txt"
SEED = 0x7069656365
PIECES = 1200
RATES = [8000.0, 22050.0, 44100.0, 48000.0, 96000.0, 192000.0]
U = 2.0**-53
TRUNCATION = 2016000.0 / 2.0**60
SAFETY = 0.999
KINDS = ("constant", "linear", "sine")
ERRORS = {"constant": 0.0, "linear": 1.0, "sine": 21.0}

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/pieces_references.py with mpmath; do not edit.
# Each line: the wave's kind (constant, linear or sine), then length pitch slope
# glide rate depth depth_slope first second ratio constant fixed, first and
# second the wave's value and slope, its value alone, or its turns and step,
# fixed 1 for the fixed mode and 0 for the ratio mode; then the exact increment
# as a double-double (high, low), the bound of keptone-engine's error, and the
# fewest parts on which MATH §2.5's condition holds, every number the binary64
# bits in hexadecimal.
"""


@dataclass(frozen=True)
class Piece:
    """A piece as keptone-engine's pieces module holds it, its numbers binary64."""

    kind: str
    length: float
    pitch: float
    slope: float
    glide: float
    rate: float
    depth: float
    depth_slope: float
    first: float
    second: float

    def ends(self, start: float, slope: float) -> float:
        """The largest magnitude of a linear quantity at the piece's ends, as the engine takes it."""
        return max(abs(start), abs(start + slope * self.length))

    def linear(self) -> tuple[float, float, float, float, bool] | None:
        """lambda as a line, in binary64 as the engine computes it: (lambda(0), its slope, the fixed mode's two, and
        whether a glide's offset was added), or None."""
        folded = False
        if self.glide == 0:
            pitch = self.pitch
        elif self.rate <= 0:
            pitch, folded = self.pitch + self.glide, True
        else:
            return None
        if self.depth == 0 and self.depth_slope == 0:
            lfo, lfo_slope = 0.0, 0.0
        elif self.kind == "constant":
            lfo, lfo_slope = self.depth * self.first, self.depth_slope * self.first
        elif self.kind == "linear" and self.depth_slope == 0:
            lfo, lfo_slope = self.depth * self.first, self.depth * self.second
        else:
            return None
        return pitch + lfo, self.slope + lfo_slope, lfo, lfo_slope, folded

    def bounds(self) -> list[float]:
        """The engine's bounds a1 to a6 of lambda ln 2's derivatives, without the products' saturation."""
        bounds = [abs(self.slope), 0.0, 0.0, 0.0, 0.0, 0.0]
        if self.glide != 0 and self.rate > 0:
            term = abs(self.glide) + 0.25
            for k in range(6):
                term *= self.rate
                bounds[k] += term
        depth, depth_slope = self.ends(self.depth, self.depth_slope), abs(self.depth_slope)
        if self.kind == "constant":
            bounds[0] += depth_slope * abs(self.first)
        elif self.kind == "linear":
            wave = self.ends(self.first, self.second)
            bounds[0] += depth_slope * wave + depth * abs(self.second)
            bounds[1] += 2 * (depth_slope * abs(self.second))
        else:
            omega = 2 * math.pi * abs(self.second)
            power, below = omega, 1.0
            for k in range(6):
                bounds[k] += depth * power + (k + 1) * (depth_slope * below)
                below, power = power, power * omega
        return [bound * math.log(2) for bound in bounds]

    def count(self) -> float:
        """The engine's count of the rule's parts, within its roundings."""
        if self.linear() is not None:
            return 1.0
        x1, x2, x3, x4, x5, x6 = self.bounds()
        bell = (
            x1**6
            + 15 * x1**4 * x2
            + 20 * x1**3 * x3
            + 45 * x1**2 * x2**2
            + 15 * x1**2 * x4
            + 60 * x1 * x2 * x3
            + 6 * x1 * x5
            + 15 * x2**3
            + 15 * x2 * x4
            + 10 * x3**2
            + x6
        )
        if bell <= 0:
            return 1.0
        first = (TRUNCATION / bell) ** (1 / 6) * SAFETY
        part = first * math.exp(-x1 * first / 6)
        return max(1.0, math.ceil(self.length / part))

    def minimal(self) -> int:
        """The fewest equal parts on which MATH's condition holds, h^6 B6 e^(a1 h) <= 2016000 2^-60, B6 the complete
        Bell polynomial of the bounds a1 to a6 of lambda ln 2's derivatives, all exact at the binary64 numbers; 1 for a
        line."""
        if self.linear() is not None:
            return 1
        exact_bounds = self.exact_bounds()
        x1, x2, x3, x4, x5, x6 = exact_bounds
        bell = (
            x1**6
            + 15 * x1**4 * x2
            + 20 * x1**3 * x3
            + 45 * x1**2 * x2**2
            + 15 * x1**2 * x4
            + 60 * x1 * x2 * x3
            + 6 * x1 * x5
            + 15 * x2**3
            + 15 * x2 * x4
            + 10 * x3**2
            + x6
        )
        if bell == 0:
            return 1
        limit = mpmath.log(mpmath.mpf(2016000) / mpmath.mpf(2) ** 60)
        low, high = mpmath.mpf(0), mpmath.power(mpmath.mpf(2016000) / mpmath.mpf(2) ** 60 / bell, mpmath.mpf(1) / 6)
        for _ in range(200):
            middle = (low + high) / 2
            if 6 * mpmath.log(middle) + mpmath.log(bell) + x1 * middle <= limit:
                low = middle
            else:
                high = middle
        return int(mpmath.ceil(mpmath.mpf(self.length) / high))

    def exact_bounds(self) -> list[mpmath.mpf]:
        """The bounds a1 to a6 in exact arithmetic."""
        m = mpmath.mpf
        length = m(self.length)

        def ends(start: float, slope: float) -> mpmath.mpf:
            return max(abs(m(start)), abs(m(start) + m(slope) * length))

        bounds = [abs(m(self.slope)), m(0), m(0), m(0), m(0), m(0)]
        if self.glide != 0 and self.rate > 0:
            for k in range(6):
                bounds[k] += (abs(m(self.glide)) + m(1) / 4) * m(self.rate) ** (k + 1)
        depth, depth_slope = ends(self.depth, self.depth_slope), abs(m(self.depth_slope))
        if self.kind == "constant":
            bounds[0] += depth_slope * abs(m(self.first))
        elif self.kind == "linear":
            bounds[0] += depth_slope * ends(self.first, self.second) + depth * abs(m(self.second))
            bounds[1] += 2 * depth_slope * abs(m(self.second))
        else:
            omega = 2 * mpmath.pi * abs(m(self.second))
            for k in range(6):
                bounds[k] += depth * omega ** (k + 1) + (k + 1) * depth_slope * omega**k
        return [bound * mpmath.log(2) for bound in bounds]

    def moving(self, t: mpmath.mpf, *, fixed: bool) -> mpmath.mpf:
        """lambda at t samples into the piece, exactly at the binary64 numbers."""
        depth = mpmath.mpf(self.depth) + mpmath.mpf(self.depth_slope) * t
        if self.kind == "constant":
            wave = mpmath.mpf(self.first)
        elif self.kind == "linear":
            wave = mpmath.mpf(self.first) + mpmath.mpf(self.second) * t
        else:
            wave = -mpmath.sinpi(2 * (mpmath.mpf(self.first) + mpmath.mpf(self.second) * t))
        lfo = depth * wave
        if fixed:
            return lfo
        envelope = mpmath.mpf(self.pitch) + mpmath.mpf(self.slope) * t
        glide = mpmath.mpf(0)
        if self.glide != 0:
            d = mpmath.mpf(self.glide)
            sign = 1 if self.glide > 0 else -1
            glide = d + (d + mpmath.mpf(sign) / 4) * mpmath.expm1(-mpmath.mpf(self.rate) * t)
        return envelope + glide + lfo

    def splits(self) -> list[mpmath.mpf]:
        """The points at which the quadrature splits the piece: enough intervals that each holds at most a quarter of
        the sine's period and a part of the glide's fall that changes it by less than a factor of e."""
        length = mpmath.mpf(self.length)
        intervals = 8
        if self.kind == "sine":
            intervals = max(intervals, math.ceil(4 * abs(self.second) * self.length))
        if self.glide != 0:
            intervals = max(intervals, math.ceil(self.rate * self.length))
        intervals = min(intervals, 400)
        return [length * k / intervals for k in range(intervals + 1)]


def exact(piece: Piece, ratio: float, constant: float, *, fixed: bool) -> mpmath.mpf:
    """r times the integral of 2^(c + lambda) over the piece, by quadrature, or in closed form where lambda is a
    line."""
    c = mpmath.mpf(constant)
    if piece.glide == 0 and piece.kind != "sine" and (piece.depth_slope == 0 or piece.kind == "constant"):
        start = piece.moving(mpmath.mpf(0), fixed=fixed)
        slope = (piece.moving(mpmath.mpf(piece.length), fixed=fixed) - start) / mpmath.mpf(piece.length)
        rate = slope * mpmath.log(2)
        length = mpmath.mpf(piece.length)
        integral = length if rate == 0 else mpmath.expm1(rate * length) / rate
        return mpmath.mpf(ratio) * mpmath.power(2, c + start) * integral
    return mpmath.mpf(ratio) * mpmath.quad(lambda t: mpmath.power(2, c + piece.moving(t, fixed=fixed)), piece.splits())


def bound(piece: Piece, constant: float, value: mpmath.mpf, *, fixed: bool) -> float:
    """The bound of keptone-engine's error that its functions document, for the closed form or the rule, beside two
    steps of the subnormals."""
    log2 = math.log(2)
    line = piece.linear()
    if line is not None:
        start, slope, lfo, lfo_slope, folded = line
        if fixed:
            moved, error, slope_error = lfo, U * abs(lfo), U * abs(lfo_slope)
        else:
            moved = start
            error = U * (abs(lfo) + abs(start)) + (U * abs(piece.pitch + piece.glide) if folded else 0.0)
            slope_error = U * (abs(lfo_slope) + abs(slope))
        error += U * abs(constant + moved)
        relative = 12 * U + log2 * (error + piece.length * slope_error)
    else:
        envelope = piece.ends(piece.pitch, piece.slope)
        glide = abs(piece.glide)
        depth = piece.ends(piece.depth, piece.depth_slope)
        e = ERRORS[piece.kind]
        if fixed:
            error, largest = U * depth * (2 + e), depth
        else:
            error = U * (3 * envelope + 6.35 * glide + 1.0875 + depth * (3 + e))
            largest = envelope + glide + depth
        error += U * (abs(constant) + largest)
        slope = piece.bounds()[0] / log2
        error += 3 * U * piece.length * slope
        relative = 8 * U + 2.0**-60 + U + log2 * error
    return relative * float(abs(value)) * (1 + 1e-6) + 2 * 2.0**-1074


def draw(randoms: Iterator[int], low: float, high: float) -> float:
    """A uniform draw from [low, high)."""
    return uniform(randoms, low, high)


def pick(randoms: Iterator[int], choices: list[Callable[[], float]]) -> float:
    """One of the choices, each as likely."""
    return choices[next(randoms) % len(choices)]()


def signed(randoms: Iterator[int], value: float) -> float:
    """value with a random sign."""
    return -value if next(randoms) & 1 else value


def wave(randoms: Iterator[int], kind: str, length: float) -> tuple[float, float]:
    """The wave's numbers: a value in [-1, 1]; a line from a value that stays within [-1, 1] over the piece at an
    LFO's slopes; or a sine's turns and step, at the LFO's speeds on its scale and beyond them."""
    if kind == "constant":
        return draw(randoms, -1.0, 1.0), 0.0
    step = pick(
        randoms,
        [
            lambda: draw(randoms, 0.0, 49.25 / 8000.0),
            lambda: draw(randoms, 0.0, 49.25 / 192000.0),
            lambda: draw(randoms, 0.0, 0.25),
            lambda: abs(binades(randoms, -30, -8)),
        ],
    )
    if kind == "sine":
        return draw(randoms, 0.0, 1.0), step
    value = draw(randoms, -1.0, 1.0)
    room = (1.0 - value) if next(randoms) & 1 else (-1.0 - value)
    slope = (4.0 if next(randoms) & 1 else 2.0) * step * (1 if room > 0 else -1)
    if abs(slope) * length > abs(room):
        slope = room / length
    return value, slope


def piece_of(randoms: Iterator[int], kind: str) -> Piece:
    """A piece of a kind, as the walk gives them: a sample long or shorter, the pitch envelope's level and slope, a
    glide's offset and rate, and the LFO's depth and its fade's slope, each on the scales, far beyond them, or none."""
    length = pick(randoms, [lambda: 1.0, lambda: draw(randoms, 0.0, 1.0) or 0.5, lambda: abs(binades(randoms, -40, 0))])
    pitch = pick(randoms, [lambda: draw(randoms, -4.0, 4.0), lambda: 0.0, lambda: draw(randoms, -60.0, 60.0)])
    slope = pick(
        randoms,
        [
            lambda: 0.0,
            lambda: signed(randoms, draw(randoms, 0.0, 11.6 / 8000.0)),
            lambda: binades(randoms, -30, -2),
        ],
    )
    glide, rate = 0.0, 0.0
    if next(randoms) % 3 == 0:
        glide = pick(randoms, [lambda: draw(randoms, -4.0, 4.0), lambda: binades(randoms, -30, 4)])
        rate = pick(
            randoms, [lambda: draw(randoms, 0.0, 4 * 11.6 / 8000.0), lambda: abs(binades(randoms, -20, 3)), lambda: 0.0]
        )
    depth = pick(
        randoms,
        [lambda: draw(randoms, 0.0, 1.0), lambda: 0.0, lambda: signed(randoms, draw(randoms, 1.0, 12.0))],
    )
    depth_slope = pick(
        randoms,
        [
            lambda: 0.0,
            lambda: draw(randoms, 0.0, 1.0 / (0.038 * 8000.0)),
            lambda: binades(randoms, -20, 0),
        ],
    )
    if glide != 0 and rate > 0:
        length = min(length, arrival(glide, rate))
    first, second = wave(randoms, kind, length)
    return Piece(kind, length, pitch, slope, glide, rate, depth, depth_slope, first, second)


def arrival(glide: float, rate: float) -> float:
    """The samples within which a glide reaches its target, ln(1 + 4|d|)/rho rounded down a little: a piece ends there
    at the latest, as the walk ends it."""
    return math.log1p(4 * abs(glide)) / rate * (1 - 1e-12)


def designed() -> list[Piece]:
    """Pieces by hand: each kind with nothing else, with a glide alone, with a fade alone, and with all of them,
    on the scales; a sine at the LFO's top speed on its scale and an octave deep at 8 kHz's sample, which takes eight
    parts; one at a quarter of the engine's rate; and a glide far faster than on its scale, up to its arrival."""
    top = 49.25 / 8000.0
    return [
        Piece("constant", 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 1.0, 0.0),
        Piece("linear", 1.0, 1.0, 1e-4, 0.0, 0.0, 1.0, 0.0, 1.0, -4 * top),
        Piece("sine", 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, top),
        Piece("sine", 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.3, 0.25),
        Piece("constant", 1.0, 0.0, 0.0, 2.0, 4 * 11.6 / 8000.0, 0.0, 0.0, 0.0, 0.0),
        Piece("constant", 1.0, 0.0, 0.0, -2.0, 4 * 11.6 / 8000.0, 0.0, 0.0, 0.0, 0.0),
        Piece("linear", 1.0, 0.0, 0.0, 0.0, 0.0, 0.2, 1e-3, -1.0, 2 * top),
        Piece("sine", 1.0, -3.0, -1e-3, 0.5, 1e-3, 0.4, 2e-3, 0.75, top),
        Piece("constant", arrival(3.0, 50.0), 0.0, 0.0, 3.0, 50.0, 0.0, 0.0, 0.0, 0.0),
        Piece("linear", 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5, 1.0, -1.0),
    ]


def lines(randoms: Iterator[int]) -> Iterator[list[str | float]]:
    """The references: the designed pieces and drawn ones of each kind, each for a few operators."""
    pieces = designed()
    while len(pieces) < PIECES:
        piece = piece_of(randoms, KINDS[next(randoms) % 3])
        if piece.count() <= 2000:
            pieces.append(piece)
    for piece in pieces:
        for _ in range(2):
            rate = RATES[next(randoms) % len(RATES)]
            fixed = next(randoms) % 3 == 0
            ratio = (1.0 if fixed else draw(randoms, 0.5, 62.0)) / rate
            constant = draw(randoms, 0.0, 13.3) if fixed else draw(randoms, 3.0, 14.0)
            value = exact(piece, ratio, constant, fixed=fixed)
            hi = float(value)
            lo = float(value - mpmath.mpf(hi))
            numbers = [
                piece.length,
                piece.pitch,
                piece.slope,
                piece.glide,
                piece.rate,
                piece.depth,
                piece.depth_slope,
                piece.first,
                piece.second,
                ratio,
                constant,
                1.0 if fixed else 0.0,
                hi,
                lo,
                bound(piece, constant, value, fixed=fixed),
                float(piece.minimal()),
            ]
            yield [piece.kind, *numbers]


def main() -> None:
    """Writes the file."""
    mpmath.mp.dps = 40
    randoms = splitmix64(SEED)
    out = [HEADER.rstrip("\n")]
    for kind, *numbers in lines(randoms):
        out.append(" ".join([str(kind), *(f"{to_bits(float(x)):016x}" for x in numbers)]))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
