#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of an envelope segment's progress over a moving rates' key for keptone-engine's tests.

    python3 scripts/progress_references.py

Computes, at binary64 inputs, the exact progress of docs/MATH.md §4.7 over a piece of a sample, the integral of
rho(q(k_R)) per sample as the rates' key k_R moves, a line with a glide's law beside it (§7.3), and the instant at
which the progress reaches a target: q exactly at whole keys as scripts/eg_references.py computes it, linear between
them; rho under both of eg.rate_law's laws; the piece split exactly at the keys where rho has a corner, whole keys
from 23 to 114 at which q's slope changes and, under the dx7 law, q's multiples of 4, a glide's crossings found by
bisection to 120 digits on either side of the instant at which it turns; on each stretch the trapezoid's exact value
where k_R is a line under the dx7 law, an exponential's closed form under the exponential law, and mpmath's
quadrature at 40 digits with a glide; and the instants by the quadratic's root, the logarithm and Newton's iteration
at 40 digits. Beside each it writes the bound within which keptone-engine's value lies, from the errors that its
functions document. It writes crates/keptone-engine/tests/data/progress.txt: on each line the kind, its inputs, the
exact value as a double-double and the bound, every number as the 16 hexadecimal digits of its binary64. The inputs
are draws of SplitMix64 from a fixed seed, on the scales and beyond them, so that the file is the same every time.
"""

import itertools
import math
from collections.abc import Callable, Iterator
from dataclasses import dataclass, field
from fractions import Fraction
from pathlib import Path

import mpmath

from binary64 import binades, splitmix64, to_bits, uniform
from eg_references import rate_law

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "progress.txt"
SEED = 0x70726F6772657373
CASES = 700
RATES = [8000.0, 22050.0, 44100.0, 48000.0, 96000.0, 192000.0]
LOWEST, HIGHEST = 23, 114
U = 2.0**-53

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/progress_references.py with mpmath; do not edit.
# Each line: a kind, then law rate sensitivity clock sample_rate key slope glide
# glide_rate length (law 0 for dx7, 1 for exponential); for reach the target
# after them; then the exact value as a double-double (high, low) and the bound
# of keptone-engine's error, every number the binary64 bits in hexadecimal:
#   progress: the progress over the piece
#   reach: the samples into the piece at which the progress reaches the target
"""


def mp(value: Fraction | float) -> mpmath.mpf:
    """A rational or a binary64 as an mpf of the working precision."""
    if isinstance(value, Fraction):
        return mpmath.mpf(value.numerator) / value.denominator
    return mpmath.mpf(value)


@dataclass
class Rates:
    """A segment's rates, exactly."""

    rate: float
    sensitivity: float
    clock: float
    law: int
    sample_rate: float
    cache: dict = field(default_factory=dict)

    def on_scale(self) -> bool:
        """Whether its rate and sensitivity lie on their scales."""
        return 0 <= self.rate <= 99 and 0 <= self.sensitivity <= 7

    def whole(self, key: int) -> tuple[Fraction, int]:
        """q at a whole key, exactly, and the largest magnitude of the values that it interpolates."""
        if key not in self.cache:
            value, largest, slack = rate_law(self.rate, self.sensitivity, float(key))
            if slack != 0:
                message = "floors that keptone-engine computes exactly"
                raise ValueError(message)
            self.cache[key] = (value, largest)
        return self.cache[key]

    def q(self, key: mpmath.mpf) -> mpmath.mpf:
        """q at a key, linear between whole keys."""
        whole = int(mpmath.floor(key))
        low, high = mp(self.whole(whole)[0]), mp(self.whole(whole + 1)[0])
        return low + (high - low) * (key - whole)

    def slope(self, key: mpmath.mpf) -> mpmath.mpf:
        """q's slope in the key on the ramp that holds a key."""
        whole = int(mpmath.floor(key))
        return mp(self.whole(whole + 1)[0] - self.whole(whole)[0])

    def rho(self, q: mpmath.mpf) -> mpmath.mpf:
        """The progress a sample at a q, rho(q)/f_e."""
        base = mpmath.mpf(self.clock) / 4096
        if self.law == 0:
            quarter = mpmath.floor(q / 4)
            value = base * mpmath.power(2, quarter) * (1 + (q - 4 * quarter) / 4)
        else:
            value = base * mpmath.power(2, q / 4)
        return value / mpmath.mpf(self.sample_rate)

    def step(self, key: mpmath.mpf) -> mpmath.mpf:
        """The progress a sample at a key."""
        return self.rho(self.q(key))

    def bends(self, key: int) -> bool:
        """Whether q's slope changes at a whole key."""
        below, at, above = (self.whole(key + d)[0] for d in (-1, 0, 1))
        return above - at != at - below

    def knots(self, start: mpmath.mpf, end: mpmath.mpf) -> list[mpmath.mpf]:
        """The keys strictly between two at which rho has a corner, in the order from the first to the second."""
        low, high = min(start, end), max(start, end)
        out = [mpmath.mpf(k) for k in range(LOWEST, HIGHEST + 1) if low < k < high and self.bends(k)]
        if self.law == 0:
            for k in range(LOWEST, HIGHEST):
                if high <= k or low >= k + 1:
                    continue
                q_low, q_high = self.whole(k)[0], self.whole(k + 1)[0]
                if q_low == q_high:
                    continue
                left, right = max(low, mpmath.mpf(k)), min(high, mpmath.mpf(k + 1))
                q_left, q_right = sorted([self.q(left), self.q(right)])
                m = int(mpmath.floor(q_left / 4)) + 1
                while 4 * m < q_right:
                    key = k + (4 * m - mp(q_low)) / mp(q_high - q_low)
                    if low < key < high:
                        out.append(key)
                    m += 1
        return sorted(out, reverse=end < start)

    def quarter_error(self) -> mpmath.mpf:
        """The relative error of rho at a key that keptone-engine's functions document: speed's 2.5u or 2.86u,
        the quotient's u, and a quarter of q's error, 12uM on the scales and 41uM beyond them, M the largest
        magnitude of the values at whole inputs that q interpolates."""
        largest = max(self.whole(k)[1] for k in (0, 23, 60, 114, 140))
        q_error = (12 if self.on_scale() else 41) * U * largest
        return mpmath.mpf((2.5 if self.law == 0 else 2.86) * U + U + q_error / 4)


@dataclass(frozen=True)
class Key:
    """The rates' key on a piece, its numbers binary64."""

    key: float
    slope: float
    glide: float
    rate: float

    def glides(self) -> bool:
        """Whether a glide moves it."""
        return self.glide != 0 and self.rate > 0

    def amplitude(self) -> mpmath.mpf:
        """The glide's amplitude d + 3 sgn d."""
        return mpmath.mpf(self.glide) + (3 if self.glide > 0 else -3)

    def at(self, t: mpmath.mpf) -> mpmath.mpf:
        """k_R at t samples, exactly."""
        line = mpmath.mpf(self.key) + mpmath.mpf(self.slope) * t
        if self.glide == 0:
            return line
        return line + mpmath.mpf(self.glide) + self.amplitude() * mpmath.expm1(-mpmath.mpf(self.rate) * t)

    def turn(self, length: float) -> mpmath.mpf | None:
        """The instant within the piece at which its derivative vanishes."""
        if not self.glides():
            return None
        ratio = mpmath.mpf(self.slope) / (mpmath.mpf(self.rate) * self.amplitude())
        if not 0 < ratio < 1:
            return None
        t = -mpmath.log(ratio) / mpmath.mpf(self.rate)
        return t if 0 < t < length else None

    def crossing(self, target: mpmath.mpf, a: mpmath.mpf, b: mpmath.mpf) -> mpmath.mpf:
        """The instant within [a, b] at which a monotone k_R reaches a key."""
        if not self.glides():
            return (target - (mpmath.mpf(self.key) + mpmath.mpf(self.glide))) / mpmath.mpf(self.slope)
        rising = self.at(b) > self.at(a)
        low, high = a, b
        with mpmath.workdps(120):
            for _ in range(420):
                middle = (low + high) / 2
                if (self.at(middle) < target) == rising:
                    low = middle
                else:
                    high = middle
        return (low + high) / 2


def stretches(rates: Rates, key: Key, length: float) -> list[tuple[mpmath.mpf, mpmath.mpf, mpmath.mpf, mpmath.mpf]]:
    """The piece's stretches between rho's corners: their instants and k_R at them, the knots' keys exact."""
    turn = key.turn(length)
    bounds = [mpmath.mpf(0), *([turn] if turn is not None else []), mpmath.mpf(length)]
    out = []
    for a, b in itertools.pairwise(bounds):
        start, key_start = a, key.at(a)
        for knot in rates.knots(key.at(a), key.at(b)):
            end = key.crossing(knot, a, b)
            out.append((start, end, key_start, knot))
            start, key_start = end, knot
        out.append((start, b, key_start, key.at(b)))
    return out


def stretch_progress(rates: Rates, key: Key, stretch: tuple) -> mpmath.mpf:
    """The progress over a stretch, exactly."""
    a, b, key_a, key_b = stretch
    if b <= a:
        return mpmath.mpf(0)
    if not key.glides():
        if rates.law == 0:
            return (b - a) * (rates.step(key_a) + rates.step(key_b)) / 2
        c = (rates.q(key_b) - rates.q(key_a)) / 4 / (b - a)
        rho = rates.step(key_a)
        if c == 0:
            return rho * (b - a)
        return rho * mpmath.expm1(c * (b - a) * mpmath.log(2)) / (c * mpmath.log(2))
    return mpmath.quad(lambda t: rates.step(key.at(t)), [a, b])


def partial(rates: Rates, key: Key, stretch: tuple, t: mpmath.mpf) -> mpmath.mpf:
    """The progress over a stretch from its start to an instant within it."""
    a, b, key_a, key_b = stretch
    if not key.glides():
        # k_R is a line: the stretch's law to t is the same law over [a, t].
        key_t = key_a + (key_b - key_a) * (t - a) / (b - a)
        return stretch_progress(rates, key, (a, t, key_a, key_t))
    return mpmath.quad(lambda s: rates.step(key.at(s)), [a, t])


def key_within(key: Key, stretch: tuple, t: mpmath.mpf) -> mpmath.mpf:
    """k_R at an instant of a stretch: its law with a glide, and the line between the stretch's ends without."""
    a, b, key_a, key_b = stretch
    if key.glides():
        return key.at(t)
    return key_a + (key_b - key_a) * (t - a) / (b - a)


def reach(rates: Rates, key: Key, pieces: list[tuple], target: mpmath.mpf) -> tuple[mpmath.mpf, int]:
    """The instant at which the progress reaches a target, and the stretch where it does: Newton's iteration within
    the stretch's bracket, from the share of the stretch that the target leaves, until the progress lies within
    10^-36 of the target relatively."""
    done = mpmath.mpf(0)
    for index, stretch in enumerate(pieces):
        progress = stretch_progress(rates, key, stretch)
        if done + progress >= target:
            left = target - done
            a, b = stretch[0], stretch[1]
            low, high = a, b
            t = a + (b - a) * left / progress if progress > 0 else a
            for _ in range(100):
                value = partial(rates, key, stretch, t) - left
                if abs(value) <= mpmath.mpf(10) ** -36 * target:
                    break
                if value < 0:
                    low = t
                else:
                    high = t
                step = t - value / rates.step(key_within(key, stretch, t))
                t = step if low < step < high else (low + high) / 2
            return t, index
        done += progress
    message = "a target within the piece"
    raise AssertionError(message)


def stretch_error(rates: Rates, key: Key, stretch: tuple, progress: mpmath.mpf) -> mpmath.mpf:
    """The bound of keptone-engine's error over a stretch: rho's relative error at its ends, with the error of k_R
    there times a quarter of q's slope, and the stretch's law's roundings."""
    a, b, key_a, key_b = stretch
    if b <= a:
        return mpmath.mpf(0)
    u = mpmath.mpf(U)
    slope = abs(rates.slope((key_a + key_b) / 2)) if LOWEST < (key_a + key_b) / 2 < HIGHEST else mpmath.mpf(0)
    key_error = u * max(abs(key_a), abs(key_b))
    if key.glide != 0:
        key_error += mpmath.mpf("4.35") * u * (abs(mpmath.mpf(key.glide)) + 3)
    relative = rates.quarter_error() + slope * key_error / 4
    if not key.glides():
        if rates.law == 0:
            return progress * (relative + 3 * u)
        change = abs(rates.q(key_b) - rates.q(key_a))
        return progress * (relative + 12 * u + u * change)
    if rates.law == 0:
        change = abs(rates.step(key_b) - rates.step(key_a)) * (b - a)
        return progress * (relative + 2 * u) + 16 * u * change
    return progress * (relative + 8 * u + mpmath.mpf(2) ** -60)


def draw(randoms: Iterator[int], low: float, high: float) -> float:
    """A uniform draw from [low, high)."""
    return uniform(randoms, low, high)


def pick(randoms: Iterator[int], choices: list[Callable[[], float]]) -> float:
    """One of the choices, each as likely."""
    return choices[next(randoms) % len(choices)]()


def case(randoms: Iterator[int]) -> tuple[Rates, Key, float]:
    """A segment's rates and a piece's key and length: on the scales and beyond them, a line slow or steep, a glide
    on its scale's speeds or far faster, up to its arrival, or one that does not move."""
    rate = pick(
        randoms,
        [lambda: float(next(randoms) % 100), lambda: draw(randoms, 0.0, 99.0), lambda: draw(randoms, 99.0, 400.0)],
    )
    sensitivity = pick(
        randoms, [lambda: float(next(randoms) % 8), lambda: draw(randoms, 0.0, 7.0), lambda: draw(randoms, 7.0, 30.0)]
    )
    clock = pick(randoms, [lambda: 49096.0, lambda: 44100.0, lambda: draw(randoms, 1000.0, 200000.0)])
    sample_rate = RATES[next(randoms) % len(RATES)]
    rates = Rates(rate, sensitivity, clock, next(randoms) % 2, sample_rate)
    start = pick(
        randoms,
        [lambda: draw(randoms, 20.0, 117.0), lambda: draw(randoms, 0.0, 140.0), lambda: draw(randoms, 60.0, 61.0)],
    )
    slope = pick(
        randoms,
        [
            lambda: 0.0,
            lambda: draw(randoms, -0.003, 0.003),
            lambda: draw(randoms, -3.0, 3.0),
            lambda: draw(randoms, -120.0, 120.0),
        ],
    )
    glide, glide_rate = 0.0, 0.0
    if next(randoms) % 3 == 0:
        glide = pick(randoms, [lambda: draw(randoms, -24.0, 24.0), lambda: draw(randoms, -60.0, 60.0)])
        glide_rate = pick(
            randoms,
            [
                lambda: draw(randoms, 0.0, 0.006),
                lambda: abs(binades(randoms, -10, 1)),
                lambda: abs(binades(randoms, -50, -31)),
                lambda: 0.0,
            ],
        )
    length = pick(randoms, [lambda: 1.0, lambda: draw(randoms, 0.0, 1.0) or 0.5])
    if glide != 0 and glide_rate > 0:
        length = min(length, math.log1p(abs(glide) / 3) / glide_rate * (1 - 1e-12))
    return rates, Key(start, slope, glide, glide_rate), length


def turning(randoms: Iterator[int]) -> tuple[Rates, Key, float]:
    """A glide that turns the key across the corners of rho: a line that rises while a glide from above falls
    faster at first, or one that falls while a glide from below rises, about a key of the ramps."""
    rates = Rates(
        draw(randoms, 20.0, 80.0),
        float(4 + next(randoms) % 4),
        49096.0,
        next(randoms) % 2,
        RATES[next(randoms) % len(RATES)],
    )
    sign = 1.0 if next(randoms) % 2 else -1.0
    glide = sign * draw(randoms, 0.5, 6.0)
    glide_rate = draw(randoms, 1.0, 8.0)
    amplitude = abs(glide) + 3
    slope = sign * glide_rate * amplitude * draw(randoms, 0.05, 0.8)
    length = min(1.0, math.log1p(abs(glide) / 3) / glide_rate * (1 - 1e-12))
    return rates, Key(draw(randoms, 40.0, 100.0), slope, glide, glide_rate), length


def lines(randoms: Iterator[int]) -> Iterator[list[str | float]]:
    """The references: for each case its progress and the instant at which it reaches a share of it, a sixth of
    them glides that turn the key."""
    count = 0
    while count < CASES:
        rates, key, length = turning(randoms) if count % 6 == 0 else case(randoms)
        pieces = stretches(rates, key, length)
        if len(pieces) > 200:
            continue
        progresses = [stretch_progress(rates, key, stretch) for stretch in pieces]
        total = sum(progresses, mpmath.mpf(0))
        if not total > 0:
            continue
        errors = [stretch_error(rates, key, s, p) for s, p in zip(pieces, progresses, strict=True)]
        inputs = [float(rates.law), rates.rate, rates.sensitivity, rates.clock, rates.sample_rate]
        inputs += [key.key, key.slope, key.glide, key.rate, length]
        bound = sum(errors, mpmath.mpf(0)) + mpmath.mpf(U) * total
        hi = float(total)
        yield ["progress", *inputs, hi, float(total - mpmath.mpf(hi)), float(bound * (1 + mpmath.mpf(10) ** -6))]
        target = float(total * mpmath.mpf(draw(randoms, 0.0, 1.0)))
        if target <= 0:
            count += 1
            continue
        instant, index = reach(rates, key, pieces, mpmath.mpf(target))
        rho = rates.step(key.at(instant))
        before = sum(errors[:index], mpmath.mpf(0)) + errors[index] + mpmath.mpf(U) * mpmath.mpf(target)
        bound = before / rho + 8 * mpmath.mpf(U) * abs(instant) + mpmath.mpf(2) ** -1070
        hi = float(instant)
        yield ["reach", *inputs, target, hi, float(instant - mpmath.mpf(hi)), float(bound * (1 + mpmath.mpf(10) ** -6))]
        count += 1


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
