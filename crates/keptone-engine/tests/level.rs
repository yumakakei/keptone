// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The laws of level (docs/MATH.md §3) against their exact values at binary64
//! inputs, which `scripts/level_references.py` computed with fractions and
//! mpmath (`tests/data/level.txt`), each within the bound that its function
//! documents; the values that the laws give exactly, at every whole value of
//! the keyboard level scaling's and the velocity's scales among them; and the
//! limits of the instrument, which the parameters beyond their scales pass
//! continuously (MATH §0.7).

use keptone_engine::level::{
    FULL_SCALE, HIGHEST_LEVEL, HIGHEST_OFFSET, Scaling, Side, curve, distance, envelope, gain, group, keyboard_scaling,
    offset, scaling_curve, total, velocity_offset,
};
use keptone_math::log2;
use keptone_model::voice::Curve;

/// u = 2⁻⁵³, the unit roundoff of binary64.
const U: f64 = f64::EPSILON / 2.0;

/// 2⁵³, from which every binary64 is an even integer.
const TWO_TO_53: f64 = 9_007_199_254_740_992.0;

/// The bounds below are first order in u; their terms of the order of u² are
/// far below this share of them.
const SECOND_ORDER: f64 = 1.0 + 1e-9;

/// Two steps of the subnormals, beside a relative bound, for a result below
/// the normal range, where the last product rounds at a fixed step.
const SUBNORMAL: f64 = f64::from_bits(2);

/// The references of a law: each line's numbers, the inputs and then the exact
/// value as a double-double.
fn references(law: &str) -> Vec<Vec<f64>> {
    let number = |word: &str| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits"));
    include_str!("data/level.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let mut words = line.split(' ');
            (words.next() == Some(law)).then(|| words.map(number).collect())
        })
        .collect()
}

/// |y − (hi + lo)|: y − hi is exact where the two are close (Sterbenz), and
/// the subtraction of lo rounds at the order of u² of the result.
fn error(y: f64, hi: f64, lo: f64) -> f64 {
    ((y - hi) - lo).abs()
}

/// Checks a law's references within the bound that `bound` gives each, from
/// the inputs and the exact value, and reports the worst error against it.
fn check(law: &str, compute: impl Fn(&[f64]) -> f64, bound: impl Fn(&[f64], f64) -> f64) {
    let (mut count, mut worst) = (0, 0.0_f64);
    for numbers in references(law) {
        let (inputs, value) = numbers.split_at(numbers.len() - 2);
        let y = compute(inputs);
        let limit = bound(inputs, value[0]) * SECOND_ORDER;
        let e = error(y, value[0], value[1]);
        assert!(
            e <= limit,
            "{law}{inputs:?}: {y:e}, exactly {:e}, error {e:e} > {limit:e}",
            value[0]
        );
        if limit > 0.0 {
            worst = worst.max(e / limit);
        }
        count += 1;
    }
    assert!(count > 100, "{count} references of {law}");
    println!("{law}: {count} references, the worst error {worst:.3} of its bound");
}

#[test]
fn the_curve_within_u() {
    check("curve", |x| curve(x[0]), |_, s| U * s.abs());
}

#[test]
fn the_envelope_within_its_bound() {
    check(
        "envelope",
        |x| envelope(x[0]).expect("a level above 0"),
        |x, e| {
            if x[0] >= 1.0 {
                U * e.abs()
            } else {
                // E = 128 + 256 log₂ L: log₂'s 1.62u of itself, times 256.
                U * e.abs() + 415.0 * U * ((e - 128.0) / 256.0).abs()
            }
        },
    );
    let silent = references("silent");
    assert!(silent.len() > 10);
    for x in silent {
        assert_eq!(envelope(x[0]), None, "{:e}", x[0]);
    }
}

#[test]
fn the_total_level_and_the_gain_within_their_bounds() {
    check(
        "total",
        |x| total(x[0], x[1]),
        |x, t| U * ((x[0] + x[1]).abs() + t.abs()),
    );
    check("gain", |x| gain(x[0]), |_, g| 1.86 * U * g + SUBNORMAL);
}

#[test]
fn the_laws_give_these_exactly() {
    // The steps of the curve, and 28 + L from 20 on.
    let steps = [
        0, 5, 9, 13, 17, 20, 23, 25, 27, 29, 31, 33, 35, 37, 39, 41, 42, 43, 45, 46,
    ];
    for (level, step) in (0_u8..).zip(steps) {
        assert_eq!(curve(f64::from(level)), f64::from(step));
    }
    assert_eq!(curve(99.0), 127.0);
    assert_eq!(curve(19.5), 47.0);
    assert_eq!(curve(-1.0), -5.0);
    // Full scale: E(99) = 64·⌊127/2⌋ = 4032, and the offset of OL 99 without
    // scaling or velocity, 4064, give 3840, whose gain is 2.
    assert_eq!(envelope(99.0), Some(4032.0));
    assert_eq!(total(4032.0, 4064.0), FULL_SCALE);
    assert_eq!(gain(FULL_SCALE), 2.0);
    // Between steps as a whole: E(98) = 64·⌊126/2⌋ = 4032 as well, and E(0.5)
    // halves E(1)'s gain.
    assert_eq!(envelope(98.5), Some(4032.0));
    assert_eq!(envelope(1.0), Some(128.0));
    assert_eq!(envelope(0.5), Some(128.0 - 256.0));
    assert_eq!(envelope(0.0), None);
    assert_eq!(envelope(-3.0), None);
    // Each 256 steps an octave of gain, up to 2⁹⁶⁰.
    assert_eq!(gain(FULL_SCALE - 256.0), 1.0);
    assert_eq!(gain(HIGHEST_LEVEL), f64::from_bits((1023 + 960) << 52));
    assert_eq!(gain(f64::MAX), gain(HIGHEST_LEVEL));
    assert_eq!(gain(-1e300), 0.0);
}

/// The curve of keyboard level scaling of a step, which the references write
/// as a binary64 of 0 to 3.
fn shape(step: f64) -> Curve {
    let step = (0..4_u8)
        .find(|&s| f64::from(s).to_bits() == step.to_bits())
        .expect("a curve's step");
    Curve::from_step(step).expect("a curve")
}

/// Whether a curve raises the level.
fn positive(curve: Curve) -> bool {
    matches!(curve, Curve::PositiveExponential | Curve::PositiveLinear)
}

/// The references of a law whose numbers are written in decimal.
fn decimal(law: &str) -> Vec<Vec<f64>> {
    include_str!("data/level.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let mut words = line.split(' ');
            (words.next() == Some(law)).then(|| {
                words
                    .map(|word| word.parse::<f64>().expect("a decimal number"))
                    .collect()
            })
        })
        .collect()
}

/// The error of G that [`group`] documents: none at a whole key below 2⁵³,
/// u|G| between keys and 2u|x| beyond 2⁵³.
fn group_error(key: f64, value: f64) -> f64 {
    if key.abs() >= TWO_TO_53 {
        2.0 * U * key.abs()
    } else if key.fract() == 0.0 {
        0.0
    } else {
        U * value.abs()
    }
}

#[test]
fn the_groups_the_distances_and_the_curves_within_their_bounds() {
    check("group", |x| group(x[0]), |x, g| group_error(x[0], g));
    check(
        "distance",
        |x| distance(x[0], x[1]),
        |x, xi| {
            let (key, place) = (x[0], x[1] + 20.0);
            let (at_key, at_place) = (group(key), group(place));
            U * (place.abs() + (at_key + 1.0).abs() + xi.abs())
                + group_error(key, at_key)
                + group_error(place, at_place)
        },
    );
    check(
        "scaling_curve",
        |x| scaling_curve(shape(x[0]), x[1]),
        |x, c| {
            let exponential = matches!(shape(x[0]), Curve::NegativeExponential | Curve::PositiveExponential);
            if exponential && x[1] >= 10.0 { 3.82 * U * c } else { 0.0 }
        },
    );
}

#[test]
fn the_keyboard_scaling_within_its_bound() {
    let (mut count, mut worst) = (0, 0.0_f64);
    for x in references("scaling") {
        let left = Side {
            curve: shape(x[1]),
            depth: x[2],
        };
        let right = Side {
            curve: shape(x[3]),
            depth: x[4],
        };
        let scaling = keyboard_scaling(x[0], left, right);
        // Each evaluation within 9uM of its exact value, M the largest
        // magnitude of its values at whole distances and depths, and the
        // first beside its floors' slack.
        for (y, hi, lo, largest, slack) in [
            (scaling.value, x[5], x[6], x[9], x[11]),
            (scaling.on_scale, x[7], x[8], x[10], 0.0),
        ] {
            let limit = 9.0 * U * largest * SECOND_ORDER + slack;
            let e = error(y, hi, lo);
            assert!(
                e <= limit,
                "scaling{x:?}: {y:e}, exactly {hi:e}, error {e:e} > {limit:e}"
            );
            if limit > 0.0 {
                worst = worst.max(e / limit);
            }
        }
        count += 1;
    }
    assert!(count > 100, "{count} references of the scaling");
    println!("scaling: {count} references, the worst error {worst:.3} of its bound");
}

#[test]
fn the_keyboard_scaling_is_the_dx7s_at_every_step_of_its_scales() {
    let steps = decimal("steps");
    assert_eq!(steps.len(), 82);
    let none = Side {
        curve: Curve::PositiveLinear,
        depth: 0.0,
    };
    for line in steps {
        let (linear, u) = (line[0] == 0.0, line[1]);
        let curves = if linear {
            [Curve::NegativeLinear, Curve::PositiveLinear]
        } else {
            [Curve::NegativeExponential, Curve::PositiveExponential]
        };
        assert_eq!(line.len(), 102);
        for (depth, &c) in (0_u8..).zip(&line[2..]) {
            for curve in curves {
                let side = Side {
                    curve,
                    depth: f64::from(depth),
                };
                let signed = if positive(curve) { c } else { -c };
                // The left side at ξ = −u, and from u = 1 on the right at ξ = u.
                let mut cases = vec![keyboard_scaling(-u, side, none)];
                if u >= 1.0 {
                    cases.push(keyboard_scaling(u, none, side));
                }
                for scaling in cases {
                    assert_eq!(
                        scaling,
                        Scaling {
                            value: signed,
                            on_scale: signed
                        },
                        "{curve:?} u {u} D {depth}"
                    );
                }
            }
        }
    }
}

#[test]
fn the_velocity_within_its_bound_and_the_dx7s_at_every_step_of_its_scales() {
    let (mut count, mut worst) = (0, 0.0_f64);
    for x in references("velocity") {
        let y = velocity_offset(x[0], x[1]);
        let limit = 9.0 * U * x[4] * SECOND_ORDER + x[5];
        let e = error(y, x[2], x[3]);
        assert!(
            e <= limit,
            "velocity{x:?}: {y:e}, exactly {:e}, error {e:e} > {limit:e}",
            x[2]
        );
        if limit > 0.0 {
            worst = worst.max(e / limit);
        }
        count += 1;
    }
    assert!(count > 100, "{count} references of the velocity");
    println!("velocity: {count} references, the worst error {worst:.3} of its bound");
    let steps = decimal("velocity_steps");
    assert_eq!(steps.len(), 8);
    for line in steps {
        assert_eq!(line.len(), 129);
        for (velocity, &value) in (0_u8..).zip(&line[1..]) {
            assert_eq!(
                velocity_offset(line[0], f64::from(velocity)),
                value,
                "KVS {} v {velocity}",
                line[0]
            );
        }
    }
}

/// 256/ln 2, rounded up: the slope of 256 log₂ λ at λ = 1.
const OCTAVE_STEPS_PER_NEPER: f64 = 369.33;

/// The bound that [`offset`] documents for the inputs of an offset's
/// reference, from the magnitudes of its intermediate values, which the
/// engine's own give to first order, and the largest magnitudes of the
/// velocity's values at whole inputs and the floors' slack that the reference
/// gives.
fn offset_bound(x: &[f64]) -> f64 {
    let (level, value, on_scale, sensitivity, velocity) = (x[0], x[1], x[2], x[3], x[4]);
    let (largest, largest_on_scale, slack) = (x[7], x[8], x[9]);
    let (shown, shown_on_scale) = (curve(level), curve(level.clamp(0.0, 99.0)));
    let (sum, sum_on_scale) = (shown + value, shown_on_scale + on_scale);
    let lambda_on_scale = sum_on_scale.clamp(0.0, 127.0);
    let lambda = lambda_on_scale + (sum - sum_on_scale);
    let error_sum_on_scale = U * (shown_on_scale.abs() + sum_on_scale.abs());
    let agree = (0.0..=99.0).contains(&level) && value.to_bits() == on_scale.to_bits();
    let error_lambda = if agree {
        error_sum_on_scale
    } else {
        2.0 * error_sum_on_scale + U * (shown.abs() + sum.abs()) + U * ((sum - sum_on_scale).abs() + lambda.abs())
    };
    let part = velocity_offset(sensitivity, velocity);
    let part_on_scale = velocity_offset(sensitivity.clamp(0.0, 7.0), velocity);
    let (error_part, error_part_on_scale) = (9.0 * U * largest, 9.0 * U * largest_on_scale);
    let total_on_scale = 32.0_f64.mul_add(lambda_on_scale, part_on_scale);
    let taken = (HIGHEST_OFFSET - total_on_scale).min(0.0);
    let error_taken = if total_on_scale > HIGHEST_OFFSET - 1.0 {
        32.0 * error_sum_on_scale + error_part_on_scale + U * total_on_scale.abs()
    } else {
        0.0
    };
    let total = 32.0_f64.mul_add(lambda, part);
    let linear = 32.0 * error_lambda + error_part + U * (total.abs() + (total + taken).abs()) + error_taken;
    let at_one = 32.0 + part + taken;
    let logarithm = log2(lambda.max(f64::MIN_POSITIVE));
    let logarithmic = error_part
        + U * ((32.0 + part).abs() + at_one.abs())
        + error_taken
        + OCTAVE_STEPS_PER_NEPER * error_lambda / lambda
        + 415.0 * U * logarithm.abs()
        + U * 256.0_f64.mul_add(logarithm, at_one).abs();
    let bound = if lambda >= 1.0 + 2.0 * error_lambda {
        linear
    } else if lambda <= 1.0 - 2.0 * error_lambda {
        logarithmic
    } else {
        linear.max(logarithmic)
    };
    bound * SECOND_ORDER + slack
}

#[test]
fn the_offset_within_its_bound() {
    let (mut count, mut worst) = (0, 0.0_f64);
    for x in references("offset") {
        let scaling = Scaling {
            value: x[1],
            on_scale: x[2],
        };
        let y = offset(x[0], scaling, x[3], x[4]).expect("an operator that sounds");
        let limit = offset_bound(&x);
        let e = error(y, x[5], x[6]);
        assert!(
            e <= limit,
            "offset{x:?}: {y:e}, exactly {:e}, error {e:e} > {limit:e}",
            x[5]
        );
        if limit > 0.0 {
            worst = worst.max(e / limit);
        }
        count += 1;
    }
    assert!(count > 100, "{count} references of the offset");
    println!("offset: {count} references, the worst error {worst:.3} of its bound");
    let silent = references("offset_silent");
    assert!(silent.len() > 10);
    for x in silent {
        assert_eq!(
            offset(
                x[0],
                Scaling {
                    value: x[1],
                    on_scale: x[2]
                },
                x[3],
                x[4]
            ),
            None,
            "{x:?}"
        );
    }
}

/// The offset of an operator of a line of whole values: key, break point, the
/// left curve and depth, the right curve and depth, output level, velocity
/// sensitivity and velocity.
fn offset_of_operator(x: &[f64]) -> Option<f64> {
    let left = Side {
        curve: shape(x[2]),
        depth: x[3],
    };
    let right = Side {
        curve: shape(x[4]),
        depth: x[5],
    };
    offset(x[6], keyboard_scaling(distance(x[0], x[1]), left, right), x[7], x[8])
}

#[test]
fn an_operator_at_whole_values_of_the_scales_is_the_dx7s() {
    let operators = decimal("operator");
    assert!(operators.len() > 500);
    for x in operators {
        assert_eq!(offset_of_operator(&x), Some(x[9]), "{x:?}");
    }
    let silent = decimal("operator_silent");
    assert!(silent.len() > 10);
    for x in silent {
        assert_eq!(offset_of_operator(&x), None, "{x:?}");
    }
}

#[test]
fn the_offsets_laws_give_these_exactly() {
    // The groups of three keys, rising from each key 3j to the next.
    assert_eq!(group(0.0), -1.0);
    assert_eq!(group(0.5), -0.5);
    assert_eq!(group(1.0), 0.0);
    assert_eq!(group(2.5), 0.0);
    assert_eq!(group(3.0), 0.0);
    assert_eq!(group(4.0), 1.0);
    // Break point 39 lies in the group of keys 58 to 60, and keys 55 to 57 take
    // no scaling.
    assert_eq!(distance(58.0, 39.0), 1.0);
    assert_eq!(distance(60.0, 39.0), 1.0);
    assert_eq!(distance(57.0, 39.0), 0.0);
    assert_eq!(distance(55.0, 39.0), 0.0);
    assert_eq!(distance(54.0, 39.0), -1.0);
    // The curves: 176 where the firmware's table holds 178, and 255 from 32
    // and from 28 on.
    assert_eq!(scaling_curve(Curve::PositiveLinear, 22.0), 176.0);
    assert_eq!(scaling_curve(Curve::NegativeLinear, 31.0), 248.0);
    assert_eq!(scaling_curve(Curve::PositiveLinear, 32.0), 255.0);
    assert_eq!(scaling_curve(Curve::PositiveExponential, 9.0), 9.0);
    assert_eq!(scaling_curve(Curve::NegativeExponential, 28.0), 255.0);
    assert_eq!(scaling_curve(Curve::PositiveExponential, 1e300), 255.0);
    // The velocity: −224·KVS at the softest, 32·KVS from 116 on, nothing at
    // KVS 0.
    assert_eq!(velocity_offset(7.0, 0.0), -1568.0);
    assert_eq!(velocity_offset(7.0, 116.0), 224.0);
    assert_eq!(velocity_offset(3.0, 127.0), 96.0);
    assert_eq!(velocity_offset(0.0, 1.0), 0.0);
    assert_eq!(velocity_offset(3.5, 0.0), -784.0);
    // Full scale, OL 99 without scaling or velocity, and the ceiling that a
    // loud note reaches, 176 steps above it.
    let none = Scaling {
        value: 0.0,
        on_scale: 0.0,
    };
    assert_eq!(offset(99.0, none, 0.0, 64.0), Some(4064.0));
    assert_eq!(offset(99.0, none, 7.0, 127.0), Some(HIGHEST_OFFSET));
    // λ = 0 is silent, and λ = 1/2, from OL 0.1, halves the gain at λ = 1.
    let lowered = Scaling {
        value: -78.0,
        on_scale: -78.0,
    };
    assert_eq!(offset(50.0, lowered, 3.0, 100.0), None);
    assert_eq!(offset(0.1, none, 0.0, 0.0), Some(32.0 - 256.0));
    // Past the limits: OL 120 raises a λ that a negative curve takes to 0 on
    // the scale by 21, 672 steps; a depth beyond its scale adds to c past 127;
    // KVS 12 takes a loud note past 4240.
    let lowest = Scaling {
        value: -127.0,
        on_scale: -127.0,
    };
    assert_eq!(offset(120.0, lowest, 0.0, 64.0), Some(672.0));
    let past = Scaling {
        value: 209.0,
        on_scale: 127.0,
    };
    assert_eq!(offset(99.0, past, 0.0, 64.0), Some(4064.0 + 32.0 * 82.0));
    assert_eq!(offset(99.0, none, 12.0, 127.0), Some(4064.0 + 32.0 * 12.0 - 48.0));
}

/// `SplitMix64`, the generator kiral's tests use: a 64-bit state advanced by a
/// constant and mixed.
struct SplitMix64(u64);

impl SplitMix64 {
    /// The next 64 bits.
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A real of [low, high), on the grid of 2⁻⁵² of the interval.
    fn uniform(&mut self, low: f64, high: f64) -> f64 {
        let unit = f64::from_bits(0x3ff0_0000_0000_0000 | (self.next_u64() >> 12)) - 1.0;
        (high - low).mul_add(unit, low)
    }

    /// A whole number of [low, high].
    fn whole(&mut self, low: i32, high: i32) -> f64 {
        let span = u64::from(low.abs_diff(high)) + 1;
        f64::from(low) + f64::from(u32::try_from(self.next_u64() % span).expect("a span of 32 bits"))
    }

    /// One of four curves.
    fn curve(&mut self) -> Curve {
        let step = u8::try_from(self.next_u64() % 4).expect("below 4");
        Curve::from_step(step).expect("a curve")
    }
}

#[test]
fn past_the_scales_the_limits_pass_continuously() {
    // Operators whose levels are 2 at least, with whole parameters of the
    // scales and the magnitudes at the scales' ends: each magnitude nudged past
    // its end moves the offset by its slope times the nudge, the scaling's c by
    // 3 a depth's step at most, the level by 5 an output level's and the
    // velocity's part by 288 a sensitivity's.
    let mut random = SplitMix64(0x6c69_6d69_7473);
    let nudge = f64::from_bits((1023 - 24) << 52);
    let mut count = 0;
    for _ in 0..4000 {
        let xi = random.whole(-40, 40);
        let ends = [0.0, 99.0];
        let left = Side {
            curve: random.curve(),
            depth: ends[usize::from(random.next_u64() & 1 == 1)],
        };
        let right = Side {
            curve: random.curve(),
            depth: ends[usize::from(random.next_u64() & 1 == 1)],
        };
        let choice = usize::try_from(random.next_u64() % 3).expect("below 3");
        let (level, at_end) = ([0.0, 99.0, random.whole(0, 99)][choice], choice < 2);
        let sensitivity = [0.0, 7.0][usize::from(random.next_u64() & 1 == 1)];
        let velocity = random.whole(0, 127);
        let at = |left: Side, right: Side, level: f64, sensitivity: f64| {
            offset(level, keyboard_scaling(xi, left, right), sensitivity, velocity)
        };
        let Some(start) = at(left, right, level, sensitivity) else {
            continue;
        };
        // λ of 2 at least, (O − V)/32 with nothing taken, and more where O's
        // bound takes some, so that a nudge keeps it from 1 and below.
        if start < 64.0 + velocity_offset(sensitivity, velocity) {
            continue;
        }
        let beyond = |end: f64| if end > 0.0 { end + nudge } else { end - nudge };
        let mut moved = vec![
            at(
                Side {
                    depth: beyond(left.depth),
                    ..left
                },
                right,
                level,
                sensitivity,
            ),
            at(
                left,
                Side {
                    depth: beyond(right.depth),
                    ..right
                },
                level,
                sensitivity,
            ),
        ];
        if at_end {
            moved.push(at(left, right, beyond(level), sensitivity));
        }
        moved.push(at(left, right, level, beyond(sensitivity)));
        for value in moved {
            let value = value.expect("a level of 1 at least, nudged");
            assert!(
                (value - start).abs() <= 288.0 * nudge + 1e-9,
                "ξ {xi} {left:?} {right:?} OL {level} KVS {sensitivity} v {velocity}: {start} to {value}"
            );
        }
        count += 1;
    }
    assert!(count > 1000, "{count} operators");
}

/// ⌊C(u)·⌊165D/64⌋/256⌋ at a whole distance and depth: the scaling without its
/// bound, written apart from the engine's.
fn unbounded(curve: Curve, u: f64, depth: f64) -> f64 {
    (scaling_curve(curve, u) * (165.0 * depth / 64.0).floor() / 256.0).floor()
}

/// A law at whole inputs, linear between them, written apart from the
/// engine's.
fn linear_between(x: f64, at: impl Fn(f64) -> f64) -> f64 {
    let whole = x.floor();
    let part = x - whole;
    if part > 0.0 {
        at(whole) + part * (at(whole + 1.0) - at(whole))
    } else {
        at(whole)
    }
}

/// The side and the distance u of a whole distance ξ.
fn side_at(xi: f64, left: Side, right: Side) -> (Side, f64) {
    if xi >= 1.0 { (right, xi) } else { (left, -xi) }
}

#[test]
fn past_the_scales_the_law_is_unbounded_where_the_scales_reach_no_limit() {
    // Random operators with parameters beyond their scales: where the second
    // evaluation, the parameters at their scales' nearer ends, reaches none of
    // the limits of c, λ and O, the offset is the law without its limits,
    // 32(s(OL) ± c) + V with c unbounded (MATH §0.7).
    let mut random = SplitMix64(0x756e_626f_756e_6473);
    let mut count = 0;
    for _ in 0..20_000 {
        let xi = if random.next_u64() & 1 == 1 {
            random.whole(-30, 30)
        } else {
            random.uniform(-30.0, 30.0)
        };
        let mut depth = || match random.next_u64() % 3 {
            0 => random.uniform(0.0, 99.0),
            1 => random.uniform(99.0, 400.0),
            _ => random.uniform(-100.0, 0.0),
        };
        let (left_depth, right_depth) = (depth(), depth());
        let left = Side {
            curve: random.curve(),
            depth: left_depth,
        };
        let right = Side {
            curve: random.curve(),
            depth: right_depth,
        };
        let level = match random.next_u64() % 3 {
            0 => random.uniform(0.0, 99.0),
            1 => random.uniform(99.0, 200.0),
            _ => random.uniform(-20.0, 0.0),
        };
        let sensitivity = match random.next_u64() % 3 {
            0 => random.uniform(0.0, 7.0),
            1 => random.uniform(7.0, 30.0),
            _ => random.uniform(-10.0, 0.0),
        };
        let velocity = random.uniform(0.0, 127.0);
        // The second evaluation's c, at the depths' nearer ends, reaches its
        // bound where a whole distance's and depth's value passes 127.
        let corners: Vec<f64> = if xi.fract() == 0.0 {
            vec![xi]
        } else {
            vec![xi.floor(), xi.floor() + 1.0]
        };
        let reaches = corners.iter().any(|&n| {
            let (side, u) = side_at(n, left, right);
            let end = side.depth.clamp(0.0, 99.0);
            [end.floor(), end.floor() + 1.0]
                .iter()
                .any(|&d| d <= 99.0 && unbounded(side.curve, u, d) > 127.0)
        });
        let on_scale = keyboard_scaling(xi, left, right).on_scale;
        let sum_on_scale = curve(level.clamp(0.0, 99.0)) + on_scale;
        let part_on_scale = velocity_offset(sensitivity.clamp(0.0, 7.0), velocity);
        if reaches
            || !(0.0..=127.0).contains(&sum_on_scale)
            || 32.0_f64.mul_add(sum_on_scale, part_on_scale) > HIGHEST_OFFSET
        {
            continue;
        }
        let scaling = linear_between(xi, |n| {
            let (side, u) = side_at(n, left, right);
            let c = linear_between(side.depth, |d| unbounded(side.curve, u, d));
            if positive(side.curve) { c } else { -c }
        });
        let lambda = curve(level) + scaling;
        if lambda < 1.0 {
            continue;
        }
        let unbounded_offset = 32.0_f64.mul_add(lambda, velocity_offset(sensitivity, velocity));
        let value =
            offset(level, keyboard_scaling(xi, left, right), sensitivity, velocity).expect("a level of 1 at least");
        let tolerance = 64.0 * U * (32.0 * (curve(level).abs() + scaling.abs()) + unbounded_offset.abs() + 4096.0);
        assert!(
            (value - unbounded_offset).abs() <= tolerance,
            "ξ {xi} {left:?} {right:?} OL {level} KVS {sensitivity} v {velocity}: {value} against {unbounded_offset}"
        );
        count += 1;
    }
    assert!(count > 1000, "{count} operators");
}
