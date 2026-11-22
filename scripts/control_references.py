#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of the controllers, the amplitude modulation and the bend for
keptone-engine's tests.

    python3 scripts/control_references.py

Computes the laws of docs/MATH.md §7.1, §7.2 and §2.4 exactly at binary64 inputs, with
fractions and, for the logarithm of the amplitude modulation, mpmath at 400 bits: a
step of a depth or a range scaled as the firmware scales it, floor(165v/64)/256,
linear between whole values; the controllers' depth of a destination, the depth of
the LFO's modulation with it and the EG bias, each in the two evaluations of MATH
§0.7, the parameters as they are and on their scales, at the limits of 255/256 and
0; the pitch's and the amplitude's sensitivities, along their end steps beyond the
scales; the LFO's pitch; the part of its amplitude that an operator keeps and the
level by which that lowers it; and the bend's position from a value of its message,
and the bend from its position, range and step, linear between whole steps and
along its first step below 0. Its inputs are drawn from SplitMix64 with a fixed
seed: at the steps, between them, beyond the scales, and at magnitudes up to 2^500,
below which no sum or product leaves binary64. Beside each value it writes the bound
of keptone-engine's error that the function's documentation gives, first order in
u = 2^-53, computed at the exact values. It writes
crates/keptone-engine/tests/data/control.txt, every number as the 16 hexadecimal
digits of its binary64, or of its u64 where the header says so, so that the file is
the same every time.
"""

import itertools
import math
import sys
from collections.abc import Callable, Iterator
from fractions import Fraction
from pathlib import Path

import mpmath

from binary64 import binades, neighbours, splitmix64, to_bits, uniform

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "control.txt"
SEED = 0x636F6E74726F6C
RANDOM = 600
SATURATION = Fraction(255, 256)
LARGEST = Fraction(sys.float_info.max)
PITCH_SENSITIVITY = (0, 10, 20, 33, 55, 92, 153, 255)
AMPLITUDE_SENSITIVITY = (0, 66, 109, 255)

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/control_references.py with mpmath; do not edit.
# Every number the binary64 bits in hexadecimal, but those of position's first
# two, u64 values; each line's inputs, then its values, each as a double-double,
# then the bounds of their errors:
#   depth v | controllers n x1 R1 .. xn Rn | modulation P w c c2 |
#   bias n x1 R1 .. xn Rn | pitch_sensitivity PMS | amplitude_sensitivity AMS |
#   lfo_pitch PMS D l | kept b b2 a a2 l | attenuation A AMS r |
#   position v bits (or noposition v bits) | bend x RANGE STEP
#   where controllers, modulation and bias give the first evaluation and the
#   second, then the bounds of both
"""

type Line = list[str | float | int]
type Exact = Fraction | mpmath.mpf
type Pair = tuple[float, float]
type Value = tuple[Fraction, mpmath.mpf]


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


def scale_value(whole: int) -> int:
    """floor(165n/64) at a whole n."""
    return (165 * whole) // 64


def depth(value: float) -> Fraction:
    """floor(165v/64)/256 at whole v, linear between whole values."""
    x = Fraction(value)
    whole = math.floor(x)
    part = x - whole
    below = Fraction(scale_value(whole), 256)
    if part == 0:
        return below
    return below + part * (Fraction(scale_value(whole + 1), 256) - below)


def depth_error(value: float) -> mpmath.mpf:
    """The bound that keptone-engine's depth documents."""
    u = ulps()
    x = Fraction(value)
    whole = math.floor(x)
    d = abs(exact(depth(value)))
    small = max(abs(scale_value(whole)), abs(scale_value(whole + 1))) < 2**53
    if x == whole:
        return mpmath.mpf(0) if abs(scale_value(whole)) < 2**53 else 2 * u * d
    if not small:
        return 3 * u * d
    return u * d + (3 * u / 512 if -1 < value < 0 else 0)


def nearer(value: float) -> float:
    """A parameter taken at its scale's nearer end, 0 to 99."""
    return min(max(value, 0.0), 99.0)


def on_scales(*values: float) -> bool:
    """Whether every parameter lies on its scale, 0 to 99, where the two evaluations of
    keptone-engine are the same computation."""
    return all(0 <= value <= 99 for value in values)


def limited(
    first: Value, second: Value, *, same: bool, bound: Fraction, at_most: bool
) -> tuple[Fraction, Fraction, mpmath.mpf, mpmath.mpf]:
    """A limit of MATH §0.7 at a bound, at most or at least, of the two evaluations at the
    limit's input with their errors: the values after it and the bounds of their errors as
    controllers' documentation gives them."""
    u = ulps()
    (value, value_error), (on_scale, on_scale_error) = first, second
    limit = min(on_scale, bound) if at_most else max(on_scale, bound)
    first = limit + (value - on_scale)
    if same:
        return first, limit, on_scale_error, on_scale_error
    error = value_error + on_scale_error + u * abs(exact(value - on_scale)) + u * abs(exact(first))
    return first, limit, error, on_scale_error


def sums(pairs: list[Pair], *, clamp: bool) -> tuple[Fraction, mpmath.mpf, Fraction, mpmath.mpf]:
    """The sum of the depths x r of the pairs and the sum of the ranges' depths, with ranges
    as they are or at their scales' nearer ends, and the bounds of their errors: a fused
    step for each depth, a sum for each range."""
    u = ulps()
    depths, depths_error, ranges, ranges_error = Fraction(0), mpmath.mpf(0), Fraction(0), mpmath.mpf(0)
    for position, range_ in pairs:
        taken = nearer(range_) if clamp else range_
        scaled, error = depth(taken), depth_error(taken)
        depths += Fraction(position) * scaled
        depths_error += abs(position) * error + u * abs(exact(depths))
        ranges += scaled
        ranges_error += error + u * abs(exact(ranges))
    return depths, depths_error, ranges, ranges_error


def controllers(pairs: list[Pair]) -> tuple[Fraction, Fraction, mpmath.mpf, mpmath.mpf]:
    """d_ctrl of MATH §7.1 in its two evaluations, and the bounds of their errors."""
    value, value_error, _, _ = sums(pairs, clamp=False)
    on_scale, on_scale_error, _, _ = sums(pairs, clamp=True)
    same = on_scales(*(range_ for _, range_ in pairs))
    return limited((value, value_error), (on_scale, on_scale_error), same=same, bound=SATURATION, at_most=True)


def modulation(
    parameter: float, fade: float, first: float, second: float
) -> tuple[Fraction, Fraction, mpmath.mpf, mpmath.mpf]:
    """D of §2.4 or a of §7.2 in its two evaluations, from the controllers' depth's two, and
    the bounds of their errors."""
    u = ulps()
    value = depth(parameter) * Fraction(fade) + Fraction(first)
    on_scale = depth(nearer(parameter)) * Fraction(fade) + Fraction(second)
    value_error = abs(fade) * depth_error(parameter) + u * abs(exact(value))
    on_scale_error = abs(fade) * depth_error(nearer(parameter)) + u * abs(exact(on_scale))
    same = on_scales(parameter) and first == second
    return limited((value, value_error), (on_scale, on_scale_error), same=same, bound=SATURATION, at_most=True)


def bias(pairs: list[Pair]) -> tuple[Fraction, Fraction, mpmath.mpf, mpmath.mpf]:
    """b of MATH §7.1 in its two evaluations, and the bounds of their errors."""
    u = ulps()
    depths, depths_error, ranges, ranges_error = sums(pairs, clamp=False)
    depths2, depths2_error, ranges2, ranges2_error = sums(pairs, clamp=True)
    same = on_scales(*(range_ for _, range_ in pairs))
    held, held2, held_error, held2_error = limited(
        (ranges, ranges_error), (ranges2, ranges2_error), same=same, bound=SATURATION, at_most=True
    )
    left, left2 = held - depths, held2 - depths2
    left_error = held_error + depths_error + u * abs(exact(left))
    left2_error = held2_error + depths2_error + u * abs(exact(left2))
    return limited((left, left_error), (left2, left2_error), same=same, bound=Fraction(0), at_most=False)


def table(steps: tuple[int, ...], first: int, last: int, value: float) -> Fraction:
    """A table's value in its steps: linear between them, along its first step below 0 and
    its last beyond its end."""
    x = Fraction(value)
    end = len(steps) - 1
    if x < 0:
        return first * x
    if x > end:
        return steps[end] + last * (x - end)
    whole = math.floor(x)
    part = x - whole
    return steps[whole] + (part * (steps[whole + 1] - steps[whole]) if part else 0)


def pitch_sensitivity(pms: float) -> Fraction:
    """sigma_PMS of MATH §2.4 in octaves."""
    return table(PITCH_SENSITIVITY, 10, 102, pms) / 256


def amplitude_sensitivity(ams: float) -> Fraction:
    """alpha_AMS of MATH §7.2."""
    return table(AMPLITUDE_SENSITIVITY, 66, 146, ams) / 255


def pitch_sensitivity_error(pms: float) -> mpmath.mpf:
    """The relative bound that keptone-engine's pitch_sensitivity documents."""
    if pms > 7:
        return 2 * ulps()
    return mpmath.mpf(0) if pms >= 0 and pms == math.floor(pms) else ulps()


def amplitude_sensitivity_error(ams: float) -> mpmath.mpf:
    """The relative bound that keptone-engine's amplitude_sensitivity documents."""
    if ams > 3:
        return 3 * ulps()
    return ulps() if ams >= 0 and ams == math.floor(ams) else 2 * ulps()


def kept(bias_: float, bias2: float, depth_: float, depth2: float, wave: float) -> tuple[Fraction, mpmath.mpf]:
    """r of MATH §7.2 at binary64 inputs and the bound that kept's documentation gives."""
    u = ulps()
    b, b2, a, a2, ell = (Fraction(x) for x in (bias_, bias2, depth_, depth2, wave))
    room = SATURATION - b2
    held = min(a2, room) + (a - a2)
    h = (1 - ell) / 2
    r = 1 - b - held * h
    error = u * (abs(exact(1 - b)) + abs(exact(h)) * (abs(exact(room)) + abs(exact(a - a2)) + 2 * abs(exact(held))))
    return r, error + u * abs(exact(r))


def attenuation(maximum: float, ams: float, r: float) -> tuple[Exact, mpmath.mpf]:
    """Delta T_AM of MATH §7.2 at binary64 inputs and the bound that attenuation's
    documentation gives."""
    scale = Fraction(maximum) / 8 * amplitude_sensitivity(ams)
    if scale == 0:
        return Fraction(0), mpmath.mpf(0)
    if r <= 0:
        return LARGEST if scale > 0 else -LARGEST, mpmath.mpf(0)
    value = exact(scale) * -mpmath.log(mpmath.mpf(r), 2)
    return value, (amplitude_sensitivity_error(ams) + mpmath.mpf("3.62") * ulps()) * abs(value)


def position(value: int, bits: int) -> Fraction | None:
    """x of MATH §2.4 of an m-bit value."""
    if not 2 <= bits <= 53 or value >> bits:
        return None
    centre = 2 ** (bits - 1)
    offset = value - centre
    return Fraction(offset, centre) if offset < 0 else Fraction(offset, centre - 1)


def bend_at(x: Fraction, range_: Fraction, step: int) -> Fraction:
    """k_bend of MATH §2.4 at a whole STEP of 0 or more."""
    if step <= 0:
        return x * range_
    most = 12 // step
    steps = min(most, math.floor(abs(x) * (most + Fraction(1, 2))))
    return steps * step * (1 if x >= 0 else -1)


def bend(x: float, range_: float, step: float) -> tuple[Fraction, mpmath.mpf]:
    """k_bend of MATH §2.4, linear between whole STEPs and along its first step below 0, and
    the bound that bend's documentation gives."""
    u = ulps()
    xs, rs, s = Fraction(x), Fraction(range_), Fraction(step)
    if s < 0:
        whole, t = 0, s
    else:
        whole = math.floor(s)
        t = s - whole
    below = bend_at(xs, rs, whole)
    if t == 0:
        return below, (u * abs(exact(below)) if whole == 0 else mpmath.mpf(0))
    above = bend_at(xs, rs, whole + 1)
    value = below + t * (above - below)
    first = abs(exact(1 - t)) * abs(exact(below)) if whole == 0 else 0
    return value, u * (first + abs(exact(t)) * abs(exact(above - below)) + abs(exact(value)))


def draw(randoms: Iterator[int], low: float, high: float) -> float:
    """An input: a real of [low, high] or one of any binade up to 2^500."""
    return uniform(randoms, low, high) if next(randoms) % 4 else binades(randoms, -60, 500)


def whole(randoms: Iterator[int], low: int, high: int) -> float:
    """A whole input of [low, high]."""
    return float(low + next(randoms) % (high - low + 1))


def pick(randoms: Iterator[int], choices: list[Callable[[], float]]) -> float:
    """One of the ways of drawing an input, each as likely."""
    return choices[next(randoms) % len(choices)]()


def parameter(randoms: Iterator[int], end: int) -> float:
    """A parameter of a scale from 0 to end: whole, between the steps, or beyond the scale."""
    return pick(
        randoms,
        [
            lambda: whole(randoms, 0, end),
            lambda: whole(randoms, 0, end),
            lambda: uniform(randoms, 0.0, float(end)),
            lambda: draw(randoms, -0.2 * end - 2, 1.2 * end + 2),
        ],
    )


def fraction_of(randoms: Iterator[int]) -> float:
    """A position or a fade: 0, 1, from 0 to 1, or beyond."""
    return pick(
        randoms,
        [
            lambda: 0.0,
            lambda: 1.0,
            lambda: uniform(randoms, 0.0, 1.0),
            lambda: uniform(randoms, 0.0, 1.0),
            lambda: draw(randoms, -0.5, 1.5),
        ],
    )


def pairs_of(randoms: Iterator[int]) -> list[Pair]:
    """From none to four controllers' positions and ranges."""
    return [(fraction_of(randoms), parameter(randoms, 99)) for _ in range(next(randoms) % 5)]


def depths(randoms: Iterator[int]) -> Iterator[Line]:
    """The depth at the steps, between them, about the regimes of its error, and drawn."""
    values = [x for v in range(-130, 230) for x in (float(v), v + 0.5)] + [*neighbours(0.0, 2), -0.25, -0.75]
    for edge in (3493701529111658, 2**52, 2**53):
        values += [float(edge - 1), float(edge), float(edge + 2), float(-edge)]
    values += [3493701529111658.5, -3493701529111658.5, 1e300, -1e300]
    values += [draw(randoms, -150.0, 250.0) for _ in range(RANDOM)]
    for v in values:
        yield ["depth", v, *double_double(depth(v)), float(depth_error(v))]


def errors(*bounds: mpmath.mpf) -> list[float]:
    """Bounds as binary64, rounded up."""
    return [math.nextafter(float(bound), math.inf) if bound else 0.0 for bound in bounds]


def limits(randoms: Iterator[int]) -> Iterator[Line]:
    """The controllers' depths, the modulation's depths and the biases."""
    designed = [[], [(1.0, 99.0)], [(1.0, 99.0), (1.0, 99.0)], [(0.5, 150.0)], [(1.0, 99.0), (0.25, -40.0)]]
    for pairs in designed + [pairs_of(randoms) for _ in range(RANDOM)]:
        flat = [x for pair in pairs for x in pair]
        value, on_scale, value_error, on_scale_error = controllers(pairs)
        for name, law in (("controllers", controllers), ("bias", bias)):
            value, on_scale, value_error, on_scale_error = law(pairs)
            yield [
                name,
                float(len(pairs)),
                *flat,
                *double_double(value),
                *double_double(on_scale),
                *errors(value_error, on_scale_error),
            ]
    for _ in range(RANDOM):
        first, second, _, _ = controllers(pairs_of(randoms))
        first, second = float(first), float(second)
        p, w = parameter(randoms, 99), fraction_of(randoms)
        value, on_scale, value_error, on_scale_error = modulation(p, w, first, second)
        yield [
            "modulation",
            p,
            w,
            first,
            second,
            *double_double(value),
            *double_double(on_scale),
            *errors(value_error, on_scale_error),
        ]


def sensitivities(randoms: Iterator[int]) -> Iterator[Line]:
    """The sensitivities, the LFO's pitch, the part kept and its attenuation."""
    pms_values = [x for v in range(-3, 12) for x in (float(v), v + 0.5)] + [
        draw(randoms, -5.0, 12.0) for _ in range(RANDOM)
    ]
    for pms in pms_values:
        yield [
            "pitch_sensitivity",
            pms,
            *double_double(pitch_sensitivity(pms)),
            float(pitch_sensitivity_error(pms) * abs(exact(pitch_sensitivity(pms)))),
        ]
    ams_values = [x for v in range(-3, 8) for x in (float(v), v + 0.5)] + [
        draw(randoms, -4.0, 8.0) for _ in range(RANDOM)
    ]
    for ams in ams_values:
        alpha = amplitude_sensitivity(ams)
        yield [
            "amplitude_sensitivity",
            ams,
            *double_double(alpha),
            float(amplitude_sensitivity_error(ams) * abs(exact(alpha))),
        ]
    for _ in range(RANDOM):
        pms, d, ell = parameter(randoms, 7), uniform(randoms, 0.0, 1.0), uniform(randoms, -1.0, 1.0)
        p = pitch_sensitivity(pms) * Fraction(d) * Fraction(ell)
        yield [
            "lfo_pitch",
            pms,
            d,
            ell,
            *double_double(p),
            float((pitch_sensitivity_error(pms) + 2 * ulps()) * abs(exact(p))),
        ]
    for _ in range(RANDOM):
        b2 = float(bias(pairs_of(randoms))[1])
        b = b2 if next(randoms) % 2 else draw(randoms, -0.5, 1.5)
        a2 = float(modulation(parameter(randoms, 99), fraction_of(randoms), 0.0, 0.0)[1])
        a = a2 if next(randoms) % 2 else draw(randoms, -0.5, 2.0)
        ell = pick(randoms, [lambda: -1.0, lambda: 1.0, lambda: uniform(randoms, -1.0, 1.0)])
        r, error = kept(b, b2, a, a2, ell)
        yield ["kept", b, b2, a, a2, ell, *double_double(r), float(error)]
    kept_values = [1.0, 0.5, 1 / 256, 0.0, -0.25, 2.0, 2.0**-60, 5e-324]
    kept_values += [
        pick(randoms, [lambda: uniform(randoms, 0.0, 1.0), lambda: draw(randoms, -0.5, 4.0)]) for _ in range(RANDOM)
    ]
    for r in kept_values:
        maximum = pick(
            randoms, [lambda: 2048.0, lambda: whole(randoms, 0, 4096), lambda: uniform(randoms, 0.0, 8192.0)]
        )
        ams = parameter(randoms, 3)
        value, error = attenuation(maximum, ams, r)
        yield ["attenuation", maximum, ams, r, *double_double(value), float(error)]


def random_value(randoms: Iterator[int]) -> tuple[int, int]:
    """A value of a message of 2 to 53 bits, and its bits."""
    bits = 2 + next(randoms) % 52
    return next(randoms) % (1 << bits), bits


def edges() -> list[tuple[float, float]]:
    """Positions just below an edge of the bend's staircase, where the product |x|(N + 1/2)
    rounded to binary64 would reach the edge that the exact product lies below, and the
    STEPs whose N they are of."""
    out = []
    for step in range(1, 13):
        most = 12 // step
        c = most + Fraction(1, 2)
        for k in range(1, most + 1):
            x = float(k / c)
            for near in (x, math.nextafter(x, 0.0), math.nextafter(math.nextafter(x, 0.0), 0.0)):
                if math.floor(near * float(c)) != math.floor(Fraction(near) * c):
                    out += [(near, float(step)), (-near, float(step))]
    return out


def bends(randoms: Iterator[int]) -> Iterator[Line]:
    """The bend's positions and the bend."""
    cases = [
        (v, bits)
        for bits in (2, 7, 14, 53)
        for v in (0, 1, 2 ** (bits - 1) - 1, 2 ** (bits - 1), 2 ** (bits - 1) + 1, 2**bits - 1)
    ]
    cases += [(2**14, 14), (5, 1), (5, 54), (2**60, 60), (0, 0)]
    cases += [random_value(randoms) for _ in range(RANDOM // 2)]
    for v, bits in cases:
        x = position(v, bits)
        if x is None:
            yield ["noposition", v, bits]
        else:
            yield ["position", v, bits, *double_double(x), float(ulps() * abs(exact(x)) if x >= 0 else 0)]
    for x, step in edges():
        value, error = bend(x, 2.0, step)
        yield ["bend", x, 2.0, step, *double_double(value), float(error)]
    for _ in range(RANDOM * 2):
        x = pick(
            randoms,
            [
                lambda: uniform(randoms, -1.0, 1.0),
                lambda: uniform(randoms, -1.0, 1.0),
                lambda: draw(randoms, -1.5, 1.5),
            ],
        )
        range_ = parameter(randoms, 12)
        step = pick(
            randoms,
            [
                lambda: 0.0,
                lambda: whole(randoms, 1, 13),
                lambda: uniform(randoms, 0.0, 13.0),
                lambda: uniform(randoms, -3.0, 0.0),
                lambda: draw(randoms, 13.0, 40.0),
            ],
        )
        value, error = bend(x, range_, step)
        yield ["bend", x, range_, step, *double_double(value), float(error)]


# The places of the u64 values in the lines of each word.
INTEGERS = {"position": {0, 1}, "noposition": {0, 1}}


def words(name: str, numbers: list) -> str:
    """A line of the file: u64 values where INTEGERS says so, binary64 bits elsewhere."""
    marked = INTEGERS.get(name, set())
    return " ".join([name, *(f"{x:016x}" if k in marked else f"{to_bits(x):016x}" for k, x in enumerate(numbers))])


def main() -> None:
    """Writes the file."""
    mpmath.mp.prec = 400
    randoms = splitmix64(SEED)
    out = [HEADER.rstrip("\n")]
    lines = itertools.chain(depths(randoms), limits(randoms), sensitivities(randoms), bends(randoms))
    out += [words(str(name), numbers) for name, *numbers in lines]
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
