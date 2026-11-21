// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The laws of level (docs/MATH.md §3.1, §3.3, §3.4) against their exact values
//! at binary64 inputs, which `scripts/level_references.py` computed with
//! fractions and mpmath (`tests/data/level.txt`), each within the bound that
//! its function documents; and the values that the laws give exactly.

use keptone_engine::level::{FULL_SCALE, HIGHEST_LEVEL, curve, envelope, gain, total};

/// u = 2⁻⁵³, the unit roundoff of binary64.
const U: f64 = f64::EPSILON / 2.0;

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
