#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
"""Write the references of the laws of level for keptone-engine's tests.

    python3 scripts/level_references.py

Computes the exact values of the laws of docs/MATH.md §3 at binary64 inputs:
the level curve s(L); the operator's offset of §3.2, from the groups of the
keys, the distance of a key's group from the break point's, the keyboard
scaling's curves, the keyboard scaling, evaluated twice as §0.7 says, and the
velocity's part, with the limits of the instrument; the envelope's part E(L) of
a total level, linear between the steps as a whole and interpolated in the gain
below 1, the total level E + O - 4256 and the gain 2^(T/256 - 14). The rational
ones are computed exactly with fractions, the logarithms and the exponentials
with mpmath at 400 bits, and a value beyond binary64's largest finite number is
taken at it after each operation that keptone-engine saturates (MATH §0.5).

A floor of the offset's laws is exact in keptone-engine where its argument is
computed exactly or lies farther from every integer than the errors that
keptone-engine documents, and is within 1 of the exact one elsewhere; the file
gives, beside such a law's value, the slack that the floors so allow, computed
from those errors, and the script stops if any floor at a whole distance,
depth, sensitivity or velocity of the scales allows any. It writes
crates/keptone-engine/tests/data/level.txt: on each line the law's name, its
inputs and its exact value as a double-double, every number as the 16
hexadecimal digits of its binary64, but for the lines whose header says their
numbers are decimal. The inputs are the steps, their halves and their
neighbours, and draws of SplitMix64 from a fixed seed, on the scales and far
beyond them, so that the file is the same every time.
"""

import math
import sys
from collections.abc import Callable, Iterator
from fractions import Fraction
from pathlib import Path

import mpmath

from binary64 import binades, neighbours, splitmix64, to_bits, uniform

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "crates" / "keptone-engine" / "tests" / "data" / "level.txt"
SEED = 0x6C6576656C
RANDOM = 600
LARGEST = sys.float_info.max
U = Fraction(1, 2**53)
TWO_53 = 2**53

# s(L) at the steps 0 to 19 (MATH §3.1).
STEPS = [0, 5, 9, 13, 17, 20, 23, 25, 27, 29, 31, 33, 35, 37, 39, 41, 42, 43, 45, 46]

# The knots of Keptone's velocity curve B(v), as (v, B) (MATH §3.2).
VELOCITY = [(0, 128), (28, 104), (56, 60), (88, 30), (116, 0)]

# The relative errors that keptone-engine documents: the exponential curve's
# value from u = 10 on, and the product of a curve's value and a depth, each
# widened by a share that covers their terms of the order of u^2; the quotient
# of the velocity's floor beyond 2^53.
CURVE_ERROR = Fraction(382, 100) * U
PRODUCT_ERROR = (CURVE_ERROR + U) * (1 + Fraction(1, 2**40))
QUOTIENT_ERROR = 4 * U

# The steps of the curves of the keyboard scaling: -LIN, -EXP, +EXP, +LIN.
LINEAR = (0, 3)
POSITIVE = (2, 3)

# The laws whose lines hold decimal numbers.
DECIMAL = ("steps", "velocity_steps", "operator", "operator_silent")

# The head of the file written, which names its copyright holder and license as
# every file of the repository does.
HEADER = """\
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Written by scripts/level_references.py with mpmath; do not edit.
# Each line: a law's name, its inputs, and its exact value as a double-double
# (high, low), every number the binary64 bits in hexadecimal:
#   curve level | envelope level | silent level (an envelope level at or below 0)
#   group key | distance key break_point | scaling_curve curve u
#   scaling distance left_curve left_depth right_curve right_depth, then the
#     value and the value on the scale as double-doubles, the largest magnitude
#     of each one's values at whole distances and depths, and the slack of the
#     value's floors
#   velocity sensitivity velocity, then the largest magnitude of its values at
#     whole sensitivities and velocities and the slack of its floors
#   offset output_level value on_scale sensitivity velocity, then the largest
#     magnitudes of the velocity's values at whole inputs, at the sensitivity
#     and on its scale, and the slack of their floors
#   offset_silent output_level value on_scale sensitivity velocity (silent)
#   total envelope offset | gain level
# and with decimal numbers, exact:
#   steps shape u, then c at the depths 0 to 99 (shape 0 linear, 1 exponential)
#   velocity_steps sensitivity, then V at the velocities 0 to 127
#   operator key break_point left_curve left_depth right_curve right_depth
#     output_level sensitivity velocity offset | operator_silent and the same
#     inputs (silent)
"""

type Line = list[str | float | int]
type Exact = Fraction | mpmath.mpf
type Side = tuple[int, float]


def saturated(value: Exact) -> Exact:
    """The value, or binary64's largest finite number with its sign beyond it."""
    if abs(value) > LARGEST:
        return type(value)(LARGEST if value > 0 else -LARGEST)
    return value


def double_double(value: Exact) -> list[float]:
    """value as hi + lo, hi the nearest binary64 and lo the nearest to the rest."""
    hi = float(value)
    rest = value - (Fraction(hi) if isinstance(value, Fraction) else mpmath.mpf(hi))
    return [hi, float(rest)]


def floor(value: Exact) -> int:
    """The floor of an exact value, as an integer."""
    return int(mpmath.floor(value)) if isinstance(value, mpmath.mpf) else math.floor(value)


def clamp(value: Fraction, low: int, high: int) -> Fraction:
    """The value taken from low to high."""
    return min(max(value, Fraction(low)), Fraction(high))


def weights(x: Fraction) -> list[tuple[Fraction, int]]:
    """The whole inputs that keptone-engine interpolates between at x, with their weights."""
    whole = math.floor(x)
    part = x - whole
    return [(Fraction(1), whole)] if part == 0 else [(1 - part, whole), (part, whole + 1)]


def curve(level: Fraction) -> Fraction:
    """s(L): the steps, linear between them, 5L below 0 and 28 + L from 20 on."""
    if level >= 20:
        return 28 + level
    if level < 0:
        return 5 * level
    step = math.floor(level)
    above = 48 if step == 19 else STEPS[step + 1]
    return STEPS[step] + (level - step) * (above - STEPS[step])


def envelope_at(step: int) -> Fraction:
    """E at a step, 64 times the floor of half of s, as keptone-engine saturates it."""
    return saturated(Fraction(64 * math.floor(saturated(curve(Fraction(step))) / 2)))


def envelope(level: float) -> Exact | None:
    """E(L): linear between the steps as a whole, interpolated in the gain below 1."""
    if level >= 1:
        step = math.floor(level)
        below = envelope_at(step)
        return saturated(below + (Fraction(level) - step) * (envelope_at(step + 1) - below))
    if level > 0:
        return envelope_at(1) + 256 * mpmath.log(mpmath.mpf(level), 2)
    return None


def group_at(key: int) -> int:
    """G at a whole key, floor((x - 1)/3)."""
    return (key - 1) // 3


def group(key: Fraction) -> Fraction:
    """G(x): the group of a whole key, linear between keys."""
    whole = math.floor(key)
    below = group_at(whole)
    return below + (key - whole) * (group_at(whole + 1) - below)


def distance(key: float, break_point: float) -> Fraction:
    """xi = G(k) + 1 - G(BP + 20), BP + 20 taken at binary64's largest finite number beyond it."""
    return group(Fraction(key)) + 1 - group(saturated(Fraction(break_point) + 20))


def scaling_curve(shape: int, u: int) -> Exact:
    """C(u) at a whole u of 0 or more: 8u, or u up to 9 and 2.094*2^(u/4) from 10 on, each at most 255."""
    if shape in LINEAR:
        return min(Fraction(255), 8 * Fraction(u))
    if u < 10:
        return Fraction(u)
    if u >= 28:
        return Fraction(255)
    if u % 4 == 0:
        return Fraction(2094, 1000) * 2 ** (u // 4)
    return mpmath.mpf(2094) / 1000 * mpmath.power(2, mpmath.mpf(u) / 4)


def unbounded(value: Exact, depth: int) -> int:
    """floor(C*floor(165D/64)/256) at a whole depth, saturated as keptone-engine saturates it."""
    scaled_depth = floor(saturated(Fraction(165 * depth)) / 64)
    return floor(saturated(value * scaled_depth) / 256)


def unbounded_range(shape: int, u: int, depth: int) -> tuple[int, int]:
    """The least and the greatest floor(C*floor(165D/64)/256) that keptone-engine may give within
    the errors it documents: 165D within u of itself beyond 2^53, and C*floor(165D/64) within
    PRODUCT_ERROR of itself where it is not exact."""
    value = scaling_curve(shape, u)
    product = 165 * depth
    if abs(product) <= TWO_53:
        depths = [floor(Fraction(product, 64))]
    else:
        depths = [floor(saturated(product * (1 + sign * U)) / 64) for sign in (-1, 1)]
    whole = isinstance(value, Fraction) and value.denominator == 1
    floors = []
    for scaled_depth in depths:
        if whole and abs(value * scaled_depth) <= TWO_53:
            floors.append(floor(saturated(value * scaled_depth) / 256))
            continue
        for sign in (-1, 1):
            factor = 1 + sign * PRODUCT_ERROR
            if isinstance(value, mpmath.mpf):
                factor = mpmath.mpf(factor.numerator) / factor.denominator
            floors.append(floor(saturated(value * scaled_depth * factor) / 256))
    return min(floors), max(floors)


def check_scales() -> None:
    """Stops if any floor at a whole distance and depth, or sensitivity and velocity, of the
    scales could be any but the exact one in keptone-engine."""
    for shape in (0, 1):
        for u in range(41):
            for depth in range(100):
                low, high = unbounded_range(shape * 2, u, depth)
                if low != high or low != unbounded(scaling_curve(shape * 2, u), depth):
                    raise ValueError(f"the scaling's floor at u = {u}, D = {depth}")
    for sensitivity in range(8):
        for velocity in range(-1, 129):
            low, high = velocity_floor_range(sensitivity, velocity)
            if low != high:
                raise ValueError(f"the velocity's floor at KVS = {sensitivity}, v = {velocity}")


def side_scaling(side: Side, u: int) -> tuple[Fraction, Fraction, Fraction, int, int]:
    """+-c of one side at a whole distance u, as MATH §0.7 evaluates it: the value, the value on
    the scale, the slack of the value's floors, and the largest magnitudes of the values at
    whole depths that each interpolates."""
    shape, given = side
    value = scaling_curve(shape, u)
    depth = Fraction(given)
    nearer = clamp(depth, 0, 99)
    on_scale, on_largest = Fraction(0), 0
    for weight, whole in weights(nearer):
        bounded = min(127, unbounded(value, whole))
        on_scale += weight * bounded
        on_largest = max(on_largest, bounded)
    if 0 <= depth <= 99:
        first, slack, largest = on_scale, Fraction(0), on_largest
    else:
        end = int(nearer)
        taken = min(127, unbounded(value, end)) - unbounded(value, end)
        first, slack, largest = Fraction(0), Fraction(0), 0
        for weight, whole in weights(depth):
            corner = saturated(Fraction(unbounded(value, whole) + taken))
            low, high = unbounded_range(shape, u, whole)
            first += weight * corner
            slack += weight * (high - low)
            largest = max(largest, abs(int(corner)))
    sign = 1 if shape in POSITIVE else -1
    return sign * first, sign * on_scale, slack, largest, on_largest


def keyboard_scaling(xi: float, left: Side, right: Side) -> tuple[Fraction, Fraction, Fraction, int, int]:
    """+-c at a distance xi, as side_scaling gives it, interpolated between whole distances."""
    value = on_scale = slack = Fraction(0)
    largest = on_largest = 0
    for weight, whole in weights(Fraction(xi)):
        first, second, allowed, most, on_most = side_scaling(right, whole) if whole >= 1 else side_scaling(left, -whole)
        value += weight * first
        on_scale += weight * second
        slack += weight * allowed
        largest, on_largest = max(largest, most), max(on_largest, on_most)
    return value, on_scale, slack, largest, on_largest


def velocity_step(velocity: int) -> tuple[int, int, int, int, int]:
    """The step of B that a whole velocity lies on: its start, level, length and rise."""
    knot = next(i for i in (3, 2, 1, 0) if i == 0 or velocity >= VELOCITY[i][0])
    (start, level), (end, following) = VELOCITY[knot], VELOCITY[knot + 1]
    return start, level, end - start, following - level, level * (end - start)


def velocity_floor(sensitivity: int, velocity: int) -> int:
    """floor(K*B(v)/8) at whole K and v, saturated as keptone-engine saturates it."""
    if velocity >= 116:
        return 0
    start, _, length, rise, base = velocity_step(velocity)
    numerator = saturated(sensitivity * saturated(Fraction(base + (velocity - start) * rise)))
    return floor(numerator / (8 * length))


def velocity_floor_range(sensitivity: int, velocity: int) -> tuple[int, int]:
    """The least and the greatest floor(K*B(v)/8) that keptone-engine may give within the errors
    it documents: exact where K*B(v)*L and its factor lie within 2^53, and the quotient within
    QUOTIENT_ERROR of itself elsewhere."""
    if velocity >= 116:
        return 0, 0
    start, _, length, rise, base = velocity_step(velocity)
    factor = base + (velocity - start) * rise
    if abs(factor) <= TWO_53 and abs(sensitivity * factor) <= TWO_53:
        exact = velocity_floor(sensitivity, velocity)
        return exact, exact
    quotient = saturated(sensitivity * saturated(Fraction(factor))) / (8 * length)
    floors = [floor(saturated(quotient * (1 + sign * QUOTIENT_ERROR))) for sign in (-1, 1)]
    return min(floors), max(floors)


def velocity_at(sensitivity: int, velocity: int) -> Fraction:
    """V at whole K and v, 16(2K - floor(K*B(v)/8)), saturated as keptone-engine saturates it."""
    doubled = saturated(Fraction(2 * sensitivity))
    return saturated(16 * saturated(doubled - velocity_floor(sensitivity, velocity)))


def velocity_offset(sensitivity: Fraction, velocity: float) -> tuple[Fraction, Fraction, Fraction]:
    """V, bilinear between whole K and v: its value, the slack of its floors, and the largest
    magnitude of its values at whole sensitivities and velocities."""
    value = slack = largest = Fraction(0)
    for sensitivity_weight, whole_sensitivity in weights(sensitivity):
        for velocity_weight, whole_velocity in weights(Fraction(velocity)):
            weight = sensitivity_weight * velocity_weight
            corner = velocity_at(whole_sensitivity, whole_velocity)
            low, high = velocity_floor_range(whole_sensitivity, whole_velocity)
            value += weight * corner
            slack += weight * 16 * (high - low)
            largest = max(largest, abs(corner))
    return value, slack, largest


def offset(inputs: list[float]) -> tuple[Exact, Fraction, Fraction, Fraction] | None:
    """O of MATH §3.2 with the limits of §0.7, saturated as keptone-engine saturates it, or None
    where the operator is silent; with the largest magnitudes of the velocity's values at whole
    inputs, at the sensitivity and on its scale, and the slack of their floors."""
    output_level, value, on_scale, sensitivity, velocity = inputs
    level_on_scale = curve(clamp(Fraction(output_level), 0, 99)) + Fraction(on_scale)
    lambda_on_scale = clamp(level_on_scale, 0, 127)
    level = saturated(saturated(curve(Fraction(output_level))) + Fraction(value))
    lam = saturated(lambda_on_scale + saturated(level - level_on_scale))
    if lam <= 0:
        return None
    sensitivity_on_scale = clamp(Fraction(sensitivity), 0, 7)
    part_on_scale, slack_on_scale, largest_on_scale = velocity_offset(sensitivity_on_scale, velocity)
    part, slack, largest = velocity_offset(Fraction(sensitivity), velocity)
    total_on_scale = 32 * lambda_on_scale + part_on_scale
    taken = min(Fraction(0), 4240 - total_on_scale)
    if total_on_scale > 4239:
        slack += slack_on_scale
    if lam >= 1:
        return saturated(saturated(32 * lam + part) + taken), largest, largest_on_scale, slack
    o_1 = saturated(32 + part) + taken
    return o_1 + 256 * mpmath.log(mpmath.mpf(lam.numerator) / lam.denominator, 2), largest, largest_on_scale, slack


def lambda_of(inputs: list[float]) -> Fraction:
    """lambda of the first evaluation, as offset computes it."""
    output_level, value, on_scale = inputs[:3]
    level_on_scale = curve(clamp(Fraction(output_level), 0, 99)) + Fraction(on_scale)
    level = saturated(saturated(curve(Fraction(output_level))) + Fraction(value))
    return saturated(clamp(level_on_scale, 0, 127) + saturated(level - level_on_scale))


def draw(randoms: Iterator[int], low: float, high: float) -> float:
    """An input: a real of [low, high] or one of any binade up to binary64's largest."""
    return uniform(randoms, low, high) if next(randoms) % 3 else binades(randoms, -60, 1023)


def whole(randoms: Iterator[int], low: int, high: int) -> float:
    """A whole input of [low, high]."""
    return float(low + next(randoms) % (high - low + 1))


def pick(randoms: Iterator[int], choices: list[Callable[[], float]]) -> float:
    """One of the ways of drawing an input, each as likely."""
    return choices[next(randoms) % len(choices)]()


def levels(randoms: Iterator[int]) -> Iterator[Line]:
    """The curve and the envelope's part."""
    inputs = [x for step in range(-5, 131) for x in (*neighbours(float(step), 1), step + 0.5)]
    inputs += [2.0**-1074, 1e-300, 0.25, 0.999, 1e300, -1e300, LARGEST, -LARGEST]
    inputs += [draw(randoms, -10.0, 130.0) for _ in range(RANDOM)]
    for level in inputs:
        yield ["curve", level, *double_double(saturated(curve(Fraction(level))))]
        value = envelope(level)
        yield ["silent", level] if value is None else ["envelope", level, *double_double(value)]


def totals(randoms: Iterator[int]) -> Iterator[Line]:
    """The total level and the gain."""
    for _ in range(RANDOM):
        e, o = draw(randoms, -500.0, 5000.0), draw(randoms, -1000.0, 5000.0)
        yield ["total", e, o, *double_double(saturated(saturated(Fraction(e) + Fraction(o)) - 4256))]
    gains = [float(t) for t in range(-300, 301)] + [3840.0, 249_344.0, 249_345.0, 1e300, -258_048.0, -262_000.0]
    gains += [draw(randoms, -270_000.0, 260_000.0) for _ in range(RANDOM)]
    for t in gains:
        value = mpmath.power(2, min(mpmath.mpf(t), mpmath.mpf(249_344)) / 256 - 14)
        yield ["gain", t, *double_double(value)]


def distances(randoms: Iterator[int]) -> Iterator[Line]:
    """The groups, the distances and the curves of the keyboard scaling."""
    huge = [2.0**52, 2.0**52 + 1, 2.0**53, 2.0**53 + 2, 3.0 * 2**60, 1e300, LARGEST]
    keys = [x for key in range(-10, 141) for x in (float(key), key + 0.5, key + 0.25)]
    keys += [*neighbours(0.0, 2), *neighbours(-1.0, 2), *huge, *(-x for x in huge)]
    keys += [draw(randoms, -50.0, 200.0) for _ in range(RANDOM // 2)]
    for key in keys:
        yield ["group", key, *double_double(group(Fraction(key)))]
    for _ in range(RANDOM // 2):
        key = pick(randoms, [lambda: whole(randoms, 0, 127), lambda: draw(randoms, -20.0, 150.0)])
        break_point = pick(randoms, [lambda: whole(randoms, 0, 99), lambda: draw(randoms, -10.0, 110.0)])
        yield ["distance", key, break_point, *double_double(distance(key, break_point))]
    for shape in range(4):
        for u in [*range(61), 1e6, 2.0**60, 1e300, LARGEST]:
            yield ["scaling_curve", float(shape), float(u), *double_double(scaling_curve(shape, int(u)))]


def scalings(randoms: Iterator[int]) -> Iterator[Line]:
    """The keyboard scaling: its values at the whole distances and depths of the scales, and
    at random ones on the scales and beyond them."""
    for shape in (0, 1):
        for u in range(41):
            yield [
                "steps",
                shape,
                u,
                *(min(127, unbounded(scaling_curve(shape * 2, u), depth)) for depth in range(100)),
            ]

    def depth() -> float:
        return pick(
            randoms,
            [lambda: whole(randoms, 0, 99), lambda: uniform(randoms, 0.0, 99.0), lambda: draw(randoms, -60.0, 300.0)],
        )

    for _ in range(RANDOM):
        xi = pick(
            randoms,
            [
                lambda: whole(randoms, -45, 45),
                lambda: uniform(randoms, -45.0, 45.0),
                lambda: uniform(randoms, -2.0, 3.0),
                lambda: draw(randoms, -45.0, 45.0),
            ],
        )
        left = (next(randoms) % 4, depth())
        right = (next(randoms) % 4, depth())
        value, on_scale, slack, largest, on_largest = keyboard_scaling(xi, left, right)
        yield [
            "scaling",
            xi,
            float(left[0]),
            left[1],
            float(right[0]),
            right[1],
            *double_double(value),
            *double_double(on_scale),
            float(largest),
            float(on_largest),
            float(slack),
        ]


def sensitivity_of(randoms: Iterator[int]) -> float:
    """A velocity sensitivity: whole or between steps on the scale, or beyond it."""
    return pick(
        randoms,
        [lambda: whole(randoms, 0, 7), lambda: uniform(randoms, 0.0, 7.0), lambda: draw(randoms, -5.0, 20.0)],
    )


def velocity_of(randoms: Iterator[int]) -> float:
    """A velocity: whole or between steps from 0 to 127, or beyond them."""
    return pick(
        randoms,
        [lambda: whole(randoms, 0, 127), lambda: uniform(randoms, 0.0, 127.0), lambda: draw(randoms, -20.0, 200.0)],
    )


def velocities(randoms: Iterator[int]) -> Iterator[Line]:
    """The velocity's part: at every whole sensitivity and velocity of the scales, and at random
    ones on the scales and beyond them."""
    for sensitivity in range(8):
        yield ["velocity_steps", sensitivity, *(int(velocity_at(sensitivity, v)) for v in range(128))]
    for _ in range(RANDOM):
        sensitivity, velocity = sensitivity_of(randoms), velocity_of(randoms)
        value, slack, largest = velocity_offset(Fraction(sensitivity), velocity)
        yield ["velocity", sensitivity, velocity, *double_double(value), float(largest), float(slack)]


def offsets(randoms: Iterator[int]) -> Iterator[Line]:
    """The offset, at random inputs on the scales and beyond them, lambda kept away from 0 and 1
    but where it is one of them exactly, and some at which the limits act."""
    designed = [
        [99.0, 0.0, 0.0, 7.0, 127.0],
        [99.0, 0.0, 0.0, 0.0, 64.0],
        [50.0, -78.0, -78.0, 3.0, 100.0],
        [50.0, -77.0, -77.0, 3.0, 100.0],
        [120.0, -127.0, -127.0, 0.0, 64.0],
        [99.0, 127.0, 127.0, 7.0, 127.0],
        [99.0, 209.0, 127.0, 7.0, 127.0],
        [-25.0, 127.0, 127.0, 7.0, 127.0],
        [-25.3, 127.0, 127.0, 7.0, 127.0],
        [99.0, 0.0, 0.0, 12.0, 127.0],
        [99.0, 0.0, 0.0, -3.0, 10.0],
        [0.5, 0.0, 0.0, 0.0, 0.0],
        [0.1, 0.0, 0.0, 0.0, 0.0],
        [99.0, 0.0, 0.0, 1e300, 127.0],
        [LARGEST, LARGEST, 127.0, LARGEST, -LARGEST],
    ]
    drawn = []
    while len(drawn) < RANDOM:
        on_scale = pick(randoms, [lambda: whole(randoms, -127, 127), lambda: uniform(randoms, -127.0, 127.0)])
        value = on_scale if next(randoms) % 2 else on_scale + draw(randoms, -500.0, 500.0)
        output_level = pick(
            randoms,
            [lambda: whole(randoms, 0, 99), lambda: uniform(randoms, 0.0, 99.0), lambda: draw(randoms, -20.0, 150.0)],
        )
        inputs = [output_level, value, on_scale, sensitivity_of(randoms), velocity_of(randoms)]
        lam = lambda_of(inputs)
        if lam in (0, 1) or (abs(lam) > Fraction(1, 10**9) and abs(lam - 1) > Fraction(1, 10**9)):
            drawn.append(inputs)
    for inputs in designed + drawn:
        result = offset(inputs)
        if result is None:
            yield ["offset_silent", *inputs]
        else:
            value, largest, largest_on_scale, slack = result
            yield ["offset", *inputs, *double_double(value), float(largest), float(largest_on_scale), float(slack)]


def operators(randoms: Iterator[int]) -> Iterator[Line]:
    """Operators at whole values of the scales, from key to offset, which the DX7's laws give
    exactly."""
    for _ in range(RANDOM * 2):
        key, break_point = int(whole(randoms, 0, 127)), int(whole(randoms, 0, 99))
        left = (next(randoms) % 4, whole(randoms, 0, 99))
        right = (next(randoms) % 4, whole(randoms, 0, 99))
        output_level, sensitivity, velocity = whole(randoms, 0, 99), whole(randoms, 0, 7), whole(randoms, 0, 127)
        xi = distance(float(key), float(break_point))
        value, on_scale, *_ = keyboard_scaling(float(xi), left, right)
        result = offset([output_level, float(value), float(on_scale), sensitivity, velocity])
        inputs = [key, break_point, left[0], int(left[1]), right[0], int(right[1])]
        inputs += [int(output_level), int(sensitivity), int(velocity)]
        if result is None:
            yield ["operator_silent", *inputs]
        else:
            yield ["operator", *inputs, int(result[0])]


def lines() -> Iterator[Line]:
    """Every line of the file, as the law's name and its numbers."""
    mpmath.mp.prec = 400
    check_scales()
    randoms = splitmix64(SEED)
    yield from levels(randoms)
    yield from totals(randoms)
    yield from distances(randoms)
    yield from scalings(randoms)
    yield from velocities(randoms)
    yield from offsets(randoms)
    yield from operators(randoms)


def main() -> None:
    """Writes the file."""
    out = [HEADER.rstrip("\n")]
    for name, *numbers in lines():
        if name in DECIMAL:
            out.append(" ".join([str(name), *(str(x) for x in numbers)]))
        else:
            out.append(" ".join([str(name), *(f"{to_bits(x):016x}" for x in numbers)]))
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text("\n".join(out) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
