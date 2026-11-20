// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The phase (docs/MATH.md §2.5): the increments over pieces on which the pitch
//! moves linearly, in closed form, and smoothly, by the Gauss–Legendre rule,
//! against their exact values at binary64 inputs, which
//! `scripts/phase_references.py` computed with mpmath (`tests/data/phase.txt`),
//! within the bounds that their functions document; and the accumulator
//! against the exact sum of its increments, which integers hold.

use core::f64::consts::{LN_2, PI};

use keptone_engine::pitch::phase::{Accumulator, Phase, gauss_increment, gauss_instants, linear_increment};

/// u = 2⁻⁵³, the unit roundoff of binary64.
const U: f64 = f64::EPSILON / 2.0;

/// The bounds below are first order in u; their terms of the order of u² are
/// far below this share of them.
const SECOND_ORDER: f64 = 1.0 + 1e-9;

/// 2⁵².
const BIG: f64 = 4_503_599_627_370_496.0;

/// The references: each line's kind and its numbers, the inputs and then the
/// exact values as double-doubles.
fn references(kind: &str) -> Vec<Vec<f64>> {
    let number = |word: &str| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits"));
    include_str!("data/phase.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let mut words = line.split(' ');
            (words.next() == Some(kind)).then(|| words.map(number).collect())
        })
        .collect()
}

/// |y − (hi + lo)|: y − hi is exact where the two are close (Sterbenz), and
/// the subtraction of lo rounds at the order of u² of the result.
fn error(y: f64, hi: f64, lo: f64) -> f64 {
    ((y - hi) - lo).abs()
}

/// Two steps of the subnormals, beside a relative bound, for a result below
/// the normal range, where the last product rounds at a fixed step.
const SUBNORMAL: f64 = f64::from_bits(2);

#[test]
fn linear_increments_within_their_bound() {
    let (mut checked, mut worst) = (0, 0.0_f64);
    for x in references("linear") {
        let [ratio, pitch, slope, length, hi, lo] = x[..] else {
            panic!("six numbers")
        };
        let y = linear_increment(ratio, pitch, slope, length);
        assert!(y.is_finite(), "{x:?}: {y}");
        // The conditions of the bound: the parts of the exponents exact, no
        // product on the way overflowing or leaving the normal range.
        let rise = slope * length;
        let mean = if rise == 0.0 {
            1.0
        } else {
            1.0 / (rise.abs() * LN_2).max(1.0)
        };
        if pitch.abs() >= BIG
            || rise.abs() >= BIG
            || (ratio * length).abs() >= f64::MAX / 4.0
            || (ratio * length * mean).abs() < f64::MIN_POSITIVE * 4.0
        {
            continue;
        }
        let end = slope.mul_add(length, pitch);
        let bound = if (pitch <= 1023.0) == (end <= 1023.0) {
            12.0 * U
        } else {
            // The crossing's instant within 2u of itself: the integrand being
            // continuous there, the split errs by r·2¹⁰²³·|b| ln 2·δτ², at most
            // 8u²(w² + w) of the increment, w = |b|τ ln 2, beside the parts' 12u
            // and the sum's rounding.
            let w = ((1023.0 - pitch) / slope * slope).abs() * LN_2;
            13.0 * U + 8.0 * U * U * w.mul_add(w, w)
        };
        let limit = bound * hi.abs() * SECOND_ORDER + SUBNORMAL;
        let e = error(y, hi, lo);
        assert!(
            e <= limit,
            "linear{x:?}: {y:e}, exactly {hi:e}, error {e:e} > {limit:e}"
        );
        worst = worst.max(e / limit);
        checked += 1;
    }
    assert!(
        checked > 1000,
        "only {checked} references within the bound's conditions"
    );
    println!("linear: {checked} references, the worst error {worst:.3} of its bound");
}

#[test]
fn gauss_increments_within_their_bound() {
    let (mut checked, mut worst) = (0, 0.0_f64);
    for x in references("gauss") {
        let [ratio, length, speed, depth, _, _, first, middle, last, hi, lo] = x[..] else {
            panic!("eleven numbers")
        };
        let y = gauss_increment(ratio, length, [first, middle, last]);
        // The rule errs by h⁷/2016000 times a sixth derivative of 2^p at an
        // instant of the piece. With p = a + A sin(ωt + φ), 2^p = 2^a·e^(c sin x),
        // c = A ln 2, x = ωt + φ, whose sixth derivative in x is e^(c sin x)
        // times the complete Bell polynomial of the derivatives of c sin x, each
        // at most c in magnitude (Faà di Bruno), so at most e^c·T₆(c), T₆ the
        // Touchard polynomial c + 31c² + 90c³ + 65c⁴ + 15c⁵ + c⁶. The integral
        // is at least h·2^a·e^(−c): relatively, (hω)⁶·T₆(c)·e^(2c)/2016000.
        let c = depth.abs() * LN_2;
        let touchard = c * (1.0 + c * (31.0 + c * (90.0 + c * (65.0 + c * (15.0 + c)))));
        let turn = length * 2.0 * PI * speed;
        let sixth = turn * turn * turn * turn * turn * turn;
        let truncation = sixth * touchard * keptone_math::exp2(2.0 * depth.abs()) / 2_016_000.0;
        // The rule's own rounding, 8u, and the pitches' at the instants, each
        // rounded once from its exact value: ln 2·u of the largest at most.
        let largest = first.abs().max(middle.abs()).max(last.abs());
        let rounding = 8.0 * U + LN_2 * U * largest;
        let limit = (rounding + truncation) * hi.abs() * SECOND_ORDER;
        let e = error(y, hi, lo);
        assert!(e <= limit, "gauss{x:?}: {y:e}, exactly {hi:e}, error {e:e} > {limit:e}");
        worst = worst.max(e / limit);
        checked += 1;
    }
    assert_eq!(checked, 500);
    println!("gauss: {checked} references, the worst error {worst:.3} of its bound");
}

#[test]
fn the_gauss_instants_within_2u() {
    for x in references("instants") {
        let instants = gauss_instants(x[0]);
        for (i, instant) in instants.into_iter().enumerate() {
            let (hi, lo) = (x[1 + 2 * i], x[2 + 2 * i]);
            assert!(error(instant, hi, lo) <= 2.0 * U * hi, "{x:?}: instant {i} {instant:e}");
        }
    }
}

#[test]
fn increments_at_these_points() {
    // A still pitch: r·h·2^p, the frequency times the length.
    assert_eq!(linear_increment(1.0, 0.0, 0.0, 0.5), 0.5);
    assert_eq!(linear_increment(3.0, 4.0, 0.0, 0.25), 12.0);
    // A rise of one octave over the piece: (2 − 1)/ln 2 of the start's.
    let octave = linear_increment(1.0, 0.0, 1.0, 1.0);
    assert!((octave - 1.0 / LN_2).abs() <= 12.0 * U * octave);
    // The pitch held at 1023 octaves, finite beyond.
    assert_eq!(linear_increment(1.0, 2000.0, 0.0, 0.5), f64::from_bits(2045 << 52));
    assert_eq!(linear_increment(4.0, 1023.0, 1e300, 1.0), f64::MAX);
    assert!(linear_increment(1.0, -1e300, 1e300, 1.0).is_finite());
    // A constant pitch integrates exactly under the Gauss rule, and a ratio of
    // 0 stands still.
    assert_eq!(gauss_increment(1.0, 0.5, [3.0, 3.0, 3.0]), 4.0);
    assert_eq!(gauss_increment(0.0, 0.5, [3.0, 4.0, 5.0]), 0.0);
    assert_eq!(
        gauss_increment(1.0, 1.0, [5000.0, 5000.0, 5000.0]),
        f64::from_bits(2046 << 52)
    );
}

/// A binary64 of magnitude below 2³⁰ and a multiple of 2⁻⁸⁰ as that multiple.
fn units(x: f64) -> i128 {
    if x == 0.0 {
        return 0;
    }
    let bits = x.to_bits();
    let exponent = i32::try_from((bits >> 52) & 0x7ff).expect("eleven bits") - 1075 + 80;
    let mantissa = i128::from((bits & 0x000f_ffff_ffff_ffff) | (1 << 52));
    let magnitude = if exponent >= 0 {
        mantissa << exponent
    } else {
        assert_eq!(mantissa & ((1 << -exponent) - 1), 0, "{x:e} is not a multiple of 2^-80");
        mantissa >> -exponent
    };
    if bits >> 63 == 1 { -magnitude } else { magnitude }
}

/// One turn, in units of 2⁻⁸⁰.
const TURN: i128 = 1 << 80;

/// A count of units of 2⁻⁸⁰ below 2⁵³ in magnitude, in turns, exactly.
fn turns_of(units: i128) -> f64 {
    let magnitude = units.unsigned_abs();
    let high = u32::try_from(magnitude >> 32).expect("below 2^64");
    let low = u32::try_from(magnitude & 0xffff_ffff).expect("32 bits");
    let value = f64::from(high).mul_add(f64::from_bits((1023 + 32) << 52), f64::from(low));
    value * f64::from_bits((1023 - 80) << 52) * if units < 0 { -1.0 } else { 1.0 }
}

/// How far a phase lies from an exact one on the circle, in turns. Every
/// increment here is a multiple of 2⁻⁸⁰ of magnitude 2⁻²⁸ or more, and so is
/// every sum that either accumulator rounds, the sums below 2⁻²⁸ being exact,
/// and every part that the compensated one keeps: each phase is a multiple of
/// 2⁻⁸⁰, and the distance is exact.
fn distance(phase: Phase, exact: i128) -> f64 {
    let d = (units(phase.turns()) - exact).rem_euclid(TURN);
    turns_of(d.min(TURN - d))
}

/// Advances phases by these increments under both accumulators, and checks
/// them against the exact phase after N samples: within N·2⁻⁵³ under
/// `Plain`, and under `Compensated` within its low part, 2⁻⁵⁴ at most, and the
/// roundings of its small parts, N·4u² at most, none here, all of them exact.
fn accumulate(start: f64, increments: impl Iterator<Item = f64>) {
    let mut plain = Phase::new(start);
    let mut compensated = Phase::new(start);
    let mut exact = units(start);
    let mut count = 0_u32;
    for increment in increments {
        plain.advance(increment, Accumulator::Plain);
        compensated.advance(increment, Accumulator::Compensated);
        exact += units(increment);
        count += 1;
        assert!((0.0..1.0).contains(&plain.turns()) && (0.0..1.0).contains(&compensated.turns()));
    }
    let n = f64::from(count);
    let plain_error = distance(plain, exact);
    let compensated_error = distance(compensated, exact);
    assert!(plain_error <= n * 2.0 * U, "plain: {plain_error:e} after {count}");
    assert!(
        compensated_error <= U + n * 4.0 * U * U,
        "compensated: {compensated_error:e} after {count}"
    );
    println!("{count} increments: plain {plain_error:e}, compensated {compensated_error:e} turns from the exact phase");
}

#[test]
fn the_accumulators_against_the_exact_phase() {
    // A note at 440 Hz for 20 s at 48 kHz, in steps of 440/48000 turns.
    let step = 440.0 / 48_000.0;
    accumulate(0.0, std::iter::repeat_n(step, 960_000));
    // Random increments of either sign below 1 turn, multiples of 2⁻⁸⁰.
    let mut state = 0x6163_6375_6d75_6c61_u64;
    let mut next = move || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    let increments: Vec<f64> = (0..200_000)
        .map(|_| {
            let r = next();
            // A mantissa of 52 bits at an exponent from 2⁻²⁸ up to 2⁻¹.
            let exponent = 1023 - 28 + (r % 28);
            let magnitude = f64::from_bits((exponent << 52) | (next() >> 12));
            if r & (1 << 40) == 0 { magnitude } else { -magnitude }
        })
        .collect();
    accumulate(0.25, increments.into_iter());
}

#[test]
fn a_phase_stays_within_a_turn() {
    for turns in [0.0, 0.5, 1.0, -0.25, 2.75, -1e-20, 1e15 + 0.5] {
        let phase = Phase::new(turns);
        assert!((0.0..1.0).contains(&phase.turns()), "{turns}: {}", phase.turns());
    }
    assert_eq!(Phase::new(-0.25).turns(), 0.75);
    assert_eq!(Phase::new(-1e-20).turns(), 0.0);
    // Just below a whole turn, 1 − 2⁻⁶⁰, a sum rounds to 1, which is 0.
    let tiny = f64::from_bits((1023 - 60) << 52);
    let mut phase = Phase::new(tiny);
    phase.advance(-2.0 * tiny, Accumulator::Plain);
    assert_eq!(phase.turns(), 0.0);
    let mut phase = Phase::new(tiny);
    phase.advance(-2.0 * tiny, Accumulator::Compensated);
    assert_eq!(phase.turns(), 0.0);
    // Compensated keeps −2⁻⁶⁰ beside the 0; an increment of 0 then brings the
    // phase below 0, and a turn added brings it back.
    phase.advance(0.0, Accumulator::Compensated);
    assert_eq!(phase.turns(), 0.0);
    phase.advance(0.5, Accumulator::Compensated);
    assert_eq!(phase.turns(), 0.5);
    // An increment of 2⁵² turns or more: Plain's phase becomes 0, and
    // Compensated keeps it.
    for big in [BIG, 1e20, f64::MAX] {
        let mut plain = Phase::new(0.25);
        plain.advance(big, Accumulator::Plain);
        assert_eq!(plain.turns(), 0.0);
        let mut compensated = Phase::new(0.25);
        compensated.advance(big, Accumulator::Compensated);
        assert_eq!(compensated.turns(), 0.25, "{big:e}");
    }
}
