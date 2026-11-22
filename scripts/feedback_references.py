#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of the feedback for keptone-engine's tests.

    python3 scripts/feedback_references.py

Computes the laws of docs/MATH.md §9 exactly at binary64 inputs, with fractions and
mpmath at 400 bits: kappa of FB, 2^(FB - 8) at the steps, linear between them from 0
at FB 0 and along its first step below 0; the loop gain beta = 2 pi kappa g; the
delay-free operator y = g sin Theta, Theta - beta sin Theta = Phi, solved by
Newton's method to 400 bits, for beta of either sign below 1 in magnitude, and
clamped; sequences of the DX7's recurrence, y[n] = g sin(2 pi (theta[n] +
kappa (y[n-1] + y[n-2])/2)), at the engine rate; and sequences of the
relaxation's trapezoidal substeps, each of whose equations it solves to 400 bits,
with theta and g given at the substeps' instants, the substeps short enough that
each equation has one root. Its inputs are drawn from SplitMix64 with a fixed seed.
Beside each value it writes the bound of keptone-engine's error that its
functions' documentation gives, first order in u = 2^-53, which it follows through
the sequences from step to step. It writes
crates/keptone-engine/tests/data/feedback.txt, every number as the 16 hexadecimal
digits of its binary64, so that the file is the same every time.
"""

import itertools
import math
import sys
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from fractions import Fraction
from pathlib import Path

import mpmath

from binary64 import binades, neighbours, splitmix64, to_bits, uniform

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "feedback.txt"
SEED = 0x6665656462616B
RANDOM = 400
LARGEST = Fraction(sys.float_info.max)

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/feedback_references.py with mpmath; do not edit.
# Every number the binary64 bits in hexadecimal; each line's inputs, then its
# value as a double-double and the bound of its error:
#   kappa FB | beta kappa g | point phase g beta | clamped phase g beta clamp
#   discrete kappa, then its lines: step phase g, then y
#   relaxation kappa tau rate, then its lines: sample m, then m + 1 pairs of
#     theta and g, the sample before's first, then y and the bound of its
#     error, and Y and the bound of its error
"""

type Line = list[str | float]
type Exact = Fraction | mpmath.mpf


def ulps() -> mpmath.mpf:
    """u = 2^-53."""
    return mpmath.mpf(2) ** -53


def exact(value: Exact) -> mpmath.mpf:
    """A rational or an mpf as an mpf."""
    if isinstance(value, mpmath.mpf):
        return value
    value = Fraction(value)
    return mpmath.mpf(value.numerator) / value.denominator


def double_double(value: Exact) -> list[float]:
    """value as hi + lo, hi the nearest binary64 and lo the nearest to the rest."""
    hi = float(value)
    rest = value - (Fraction(hi) if isinstance(value, Fraction) else mpmath.mpf(hi))
    return [hi, float(rest)]


def kappa(fb: float) -> Fraction:
    """kappa of MATH §9.1: 2^(FB - 8) at whole FBs from 1, linear between them from 0 at
    FB 0, along the first step below 0, and binary64's largest finite number beyond it."""
    x = Fraction(fb)
    if x < 0:
        return x / 128

    def at(whole: int) -> Fraction:
        if whole <= 0:
            return Fraction(0)
        if whole > 1100:
            return LARGEST
        return min(Fraction(2) ** (whole - 8), LARGEST)

    whole = math.floor(x)
    part = x - whole
    return at(whole) if part == 0 else at(whole) + part * (at(whole + 1) - at(whole))


def kappa_error(fb: float) -> mpmath.mpf:
    """The bound that keptone-engine's kappa documents."""
    if fb < 0 or fb == math.floor(fb):
        return mpmath.mpf(0)
    return ulps() * abs(exact(kappa(fb)))


def bracketed(
    f: Callable[[mpmath.mpf], mpmath.mpf], slope: Callable[[mpmath.mpf], mpmath.mpf], low: mpmath.mpf, high: mpmath.mpf
) -> mpmath.mpf:
    """The root of an increasing function inside a bracket: bisected to 2^-90 of its width,
    then polished by Newton's method to 400 bits."""
    for _ in range(90):
        middle = (low + high) / 2
        if f(middle) > 0:
            high = middle
        else:
            low = middle
    value = (low + high) / 2
    for _ in range(8):
        value -= f(value) / slope(value)
    return value


def theta_of(phase: float, beta: float) -> mpmath.mpf:
    """The root Theta of Theta - beta sin Theta = Phi, Phi = 2 pi r for the phase reduced to a
    half turn, |beta| below 1: it lies within |beta| of Phi."""
    r = exact(Fraction(phase)) - round(Fraction(phase))
    target = 2 * mpmath.pi * r
    b = exact(Fraction(beta))
    reach = abs(b) + mpmath.mpf(2) ** -300
    return bracketed(
        lambda theta: theta - b * mpmath.sin(theta) - target,
        lambda theta: 1 - b * mpmath.cos(theta),
        target - reach,
        target + reach,
    )


def point(phase: float, gain: float, beta: float, shifted: bool) -> tuple[mpmath.mpf, mpmath.mpf]:  # noqa: FBT001
    """y = g sin Theta and the bound that keptone-engine's point documents; shifted where the
    engine solves half a turn away, beta being negative."""
    u = ulps()
    theta = theta_of(phase, beta)
    g, b = exact(Fraction(gain)), exact(Fraction(beta))
    y = g * mpmath.sin(theta)
    r = exact(Fraction(phase)) - round(Fraction(phase))
    if shifted:
        moved = exact(Fraction(phase - 0.5))
        r = moved - mpmath.nint(moved)
    residual = 4 * u * max(1, abs(2 * mpmath.pi * r))
    if shifted:
        residual += mpmath.pi * u
    slope = 1 - b * mpmath.cos(theta)
    error = g * (abs(mpmath.cos(theta)) * residual / slope + mpmath.mpf("1.46") * u * abs(mpmath.sin(theta)))
    return y, error + u * abs(y)


def draw(randoms: Iterator[int], low: float, high: float) -> float:
    """An input: a real of [low, high] or one of any binade up to 2^60."""
    return uniform(randoms, low, high) if next(randoms) % 4 else binades(randoms, -30, 60)


def pick(randoms: Iterator[int], choices: list[Callable[[], float]]) -> float:
    """One of the ways of drawing an input, each as likely."""
    return choices[next(randoms) % len(choices)]()


def laws(randoms: Iterator[int]) -> Iterator[Line]:
    """kappa, beta, the point solution and the clamped one."""
    u = ulps()
    fbs = [x for f in range(-3, 12) for x in (float(f), f + 0.5, f + 0.25)] + [*neighbours(0.0, 1), 1e3, 1100.0, 2e3]
    fbs += [draw(randoms, -5.0, 12.0) for _ in range(RANDOM)]
    for fb in fbs:
        yield ["kappa", fb, *double_double(kappa(fb)), float(kappa_error(fb))]
    for _ in range(RANDOM):
        k, g = uniform(randoms, -0.1, 0.5), uniform(randoms, 0.0, 2.0)
        b = 2 * mpmath.pi * exact(Fraction(k)) * exact(Fraction(g))
        yield ["beta", k, g, *double_double(b), float(3 * u * abs(b))]
    for _ in range(RANDOM * 2):
        phase = pick(randoms, [lambda: uniform(randoms, 0.0, 1.0), lambda: draw(randoms, -3.0, 3.0)])
        gain = uniform(randoms, 0.0, 2.0)
        beta = pick(
            randoms,
            [
                lambda: uniform(randoms, -0.999, 0.999),
                lambda: uniform(randoms, 0.9, 0.999999),
                lambda: -uniform(randoms, 0.9, 0.999999),
                lambda: uniform(randoms, 0.0, 0.3),
            ],
        )
        y, error = point(phase, gain, beta, beta < 0)
        yield ["point", phase, gain, beta, *double_double(y), float(error)]
    for _ in range(RANDOM):
        phase, gain = uniform(randoms, 0.0, 1.0), uniform(randoms, 0.0, 2.0)
        beta = pick(randoms, [lambda: uniform(randoms, -4.0, 4.0), lambda: uniform(randoms, -1.0, 1.0)])
        clamp = pick(randoms, [lambda: 0.999, lambda: uniform(randoms, 0.5, 0.999)])
        taken = max(-clamp, min(clamp, beta))
        y, error = point(phase, gain, taken, taken < 0)
        yield ["clamped", phase, gain, beta, clamp, *double_double(y), float(error)]


def discrete(randoms: Iterator[int]) -> Iterator[Line]:
    """Sequences of the DX7's recurrence from a new voice's outputs, with the bounds of the
    engine's errors followed through them."""
    u = ulps()
    for _ in range(60):
        k = pick(randoms, [lambda: float(2 ** (next(randoms) % 7 - 7)), lambda: uniform(randoms, -0.02, 0.6)])
        yield ["discrete", k]
        kk = exact(Fraction(k))
        previous = before = (mpmath.mpf(0), mpmath.mpf(0))
        phase = uniform(randoms, 0.0, 1.0)
        step = uniform(randoms, 0.0, 0.05)
        steps = 12
        for _ in range(steps):
            gain = uniform(randoms, 0.0, 2.0)
            g = exact(Fraction(gain))
            mean = (previous[0] + before[0]) / 2
            mean_error = (previous[1] + before[1]) / 2 + u * abs(mean)
            argument = exact(Fraction(phase)) + kk * mean
            y = g * mpmath.sin(2 * mpmath.pi * argument)
            slope = g * 2 * mpmath.pi * abs(mpmath.cos(2 * mpmath.pi * argument))
            error = slope * (abs(kk) * mean_error + u * abs(argument)) + mpmath.mpf("2.46") * u * abs(y)
            yield ["step", phase, gain, *double_double(y), float(error)]
            before, previous = previous, (y, error)
            phase = float(Fraction(phase) + Fraction(step)) % 1.0


@dataclass(frozen=True)
class Substep:
    """A trapezoidal substep of the relaxation: Y and the output at its start, theta and g at
    its end, kappa and a = h/(2 tau)."""

    start: mpmath.mpf
    before: mpmath.mpf
    phase: float
    gain: float
    k: mpmath.mpf
    a: mpmath.mpf

    def residual(self, value: mpmath.mpf) -> mpmath.mpf:
        """F of the substep at a value."""
        a, g, p = self.a, exact(Fraction(self.gain)), exact(Fraction(self.phase))
        sine = mpmath.sin(2 * mpmath.pi * (p + self.k * value))
        return (1 + a) * value - (1 - a) * self.start - a * self.before - a * g * sine

    def slope(self, value: mpmath.mpf) -> mpmath.mpf:
        """F' of the substep at a value."""
        a, g, p = self.a, exact(Fraction(self.gain)), exact(Fraction(self.phase))
        return 1 + a - a * g * 2 * mpmath.pi * self.k * mpmath.cos(2 * mpmath.pi * (p + self.k * value))

    def root(self) -> mpmath.mpf:
        """The root of its equation, unique, inside its bracket: within a g/(1 + a) of
        ((1 - a) start + a before)/(1 + a)."""
        a, g = self.a, exact(Fraction(self.gain))
        centre = ((1 - a) * self.start + a * self.before) / (1 + a)
        reach = a * abs(g) / (1 + a) + mpmath.mpf(2) ** -300
        return bracketed(self.residual, self.slope, centre - reach, centre + reach)


def relaxations(randoms: Iterator[int]) -> Iterator[Line]:
    """Sequences of the relaxation's samples, each of trapezoidal substeps, from a new
    voice's Y, with the bounds of the engine's errors followed through them."""
    u = ulps()
    for _ in range(60):
        rate = pick(randoms, [lambda: 48000.0, lambda: 44100.0, lambda: 96000.0])
        k = pick(randoms, [lambda: 2.0**-3, lambda: 2.0**-2, lambda: 2.0**-1, lambda: uniform(randoms, -0.4, 0.8)])
        tau = pick(randoms, [lambda: 1.5 / 49096, lambda: uniform(randoms, 1e-6, 1e-4)])
        yield ["relaxation", k, tau, rate]
        kk = exact(Fraction(k))
        lagged = (mpmath.mpf(0), mpmath.mpf(0))
        phase, speed = uniform(randoms, 0.0, 1.0), uniform(randoms, 0.0, 0.05)
        gain, slope = uniform(randoms, 0.2, 2.0), uniform(randoms, -1e-3, 1e-3)
        for n in range(1, 13):
            pairs = []
            m_values = [0]
            beta_max = max(abs(2 * math.pi * k * (gain + slope * (n - 1))), abs(2 * math.pi * k * (gain + slope * n)))
            excess = beta_max - 1
            m = 1 if excess <= 0 else math.floor(excess / (2 * tau * rate)) + 1
            m = min(m, 8)
            if excess > 0 and 1 / (m * rate) >= 2 * tau / excess:
                m_values = []
            for j in range(m + 1):
                t = Fraction(n - 1) + Fraction(j, m)
                pairs.append(
                    (float((Fraction(phase) + Fraction(speed) * t) % 1), float(Fraction(gain) + Fraction(slope) * t))
                )
            if not m_values:
                break
            a = exact(Fraction(1)) / (2 * exact(Fraction(tau)) * exact(Fraction(rate)) * m)
            for j in range(1, m + 1):
                (p0, g0), (p1, g1) = pairs[j - 1], pairs[j]
                start, start_error = lagged
                arg0 = exact(Fraction(p0)) + kk * start
                before = exact(Fraction(g0)) * mpmath.sin(2 * mpmath.pi * arg0)
                cos0 = abs(mpmath.cos(2 * mpmath.pi * arg0))
                before_error = exact(Fraction(g0)) * 2 * mpmath.pi * cos0 * (abs(kk) * start_error + u * abs(arg0))
                before_error += mpmath.mpf("2.46") * u * abs(before)
                value = Substep(start, before, p1, g1, kk, a).root()
                arg1 = exact(Fraction(p1)) + kk * value
                g1x = exact(Fraction(g1))
                derivative = 1 + a - a * g1x * 2 * mpmath.pi * kk * mpmath.cos(2 * mpmath.pi * arg1)
                rounding = u * (
                    3 * abs((1 + a) * value) + 4 * abs((1 - a) * start) + mpmath.mpf("5.5") * a * abs(before)
                )
                rounding += u * mpmath.mpf("3.5") * a * abs(g1x)
                rounding += a * g1x * 2 * mpmath.pi * abs(mpmath.cos(2 * mpmath.pi * arg1)) * u * abs(arg1)
                rounding += 3 * u * abs(value - start)
                carried = abs(1 - a) * start_error + a * before_error
                reduced = arg1 - mpmath.nint(arg1)
                stop = 2 * u * max(1 / (2 * mpmath.pi), abs(reduced)) / max(abs(kk), mpmath.mpf(2) ** -60)
                lagged = (value, (rounding + carried) / abs(derivative) + stop)
            p_end, g_end = pairs[-1]
            arg = exact(Fraction(p_end)) + kk * lagged[0]
            g_endx = exact(Fraction(g_end))
            y = g_endx * mpmath.sin(2 * mpmath.pi * arg)
            y_error = (
                g_endx * 2 * mpmath.pi * abs(mpmath.cos(2 * mpmath.pi * arg)) * (abs(kk) * lagged[1] + u * abs(arg))
            )
            y_error += mpmath.mpf("2.46") * u * abs(y)
            flat = [x for pair in pairs for x in pair]
            yield [
                "sample",
                float(m),
                *flat,
                *double_double(y),
                float(y_error),
                *double_double(lagged[0]),
                float(lagged[1]),
            ]


def main() -> None:
    """Writes the file."""
    mpmath.mp.prec = 400
    randoms = splitmix64(SEED)
    out = [HEADER.rstrip("\n")]
    for name, *numbers in itertools.chain(laws(randoms), discrete(randoms), relaxations(randoms)):
        out.append(" ".join([str(name), *(f"{to_bits(x):016x}" for x in numbers)]))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
