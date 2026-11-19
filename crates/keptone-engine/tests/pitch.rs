// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The laws of pitch (docs/MATH.md §2.1–§2.3) against their exact values at
//! binary64 inputs, which `scripts/pitch_references.py` computed with fractions
//! and mpmath (`tests/data/pitch.txt`), each within the bound that its
//! function documents, from rounding analysis; and the values that the laws
//! give exactly.

use core::f64::consts::LOG2_10;

use keptone_engine::pitch::{
    FixedDetune, HIGHEST_PITCH, TransposeOctave, TwelveTet, detune, fixed_detune, fixed_pitch, frequency, ratio, shift,
    transposed_key, transposition,
};

/// u = 2⁻⁵³, the unit roundoff of binary64.
const U: f64 = f64::EPSILON / 2.0;

/// 12·2⁵², beyond which a shift's twelfth has a step of 1 or more.
const WHOLE_TWELFTHS: f64 = 54_043_195_528_445_952.0;

/// The bounds below are first order in u; their terms of the order of u² are
/// far below this share of them.
const SECOND_ORDER: f64 = 1.0 + 1e-9;

/// 2^k for k in the normal range, from its bits.
fn power_of_two(k: i32) -> f64 {
    f64::from_bits(u64::try_from(1023 + k).expect("a normal power of two") << 52)
}

/// The references: each line's law and its numbers, the inputs and then the
/// exact value as a double-double.
fn references() -> Vec<(&'static str, Vec<f64>)> {
    let number = |word: &str| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits"));
    include_str!("data/pitch.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let mut words = line.split(' ');
            let law = words.next().expect("a law");
            (law, words.map(number).collect())
        })
        .collect()
}

/// |y − (hi + lo)|: y − hi is exact where the two are close (Sterbenz), and
/// the subtraction of lo rounds at the order of u² of the result.
fn error(y: f64, hi: f64, lo: f64) -> f64 {
    ((y - hi) - lo).abs()
}

/// The error of P − 12 in a shift: 0 from 6 to 24 (Sterbenz), u|P − 12| else.
fn period_error(period: f64) -> f64 {
    if (6.0..=24.0).contains(&period) {
        0.0
    } else {
        U * (period - 12.0).abs()
    }
}

/// The bound that [`shift`] documents for τ(σ) under the period, with the
/// twelfths `octaves` that it takes.
fn shift_bound(sigma: f64, period: f64, octaves: f64, tau: f64) -> f64 {
    let beyond = if sigma.abs() >= WHOLE_TWELFTHS {
        (period - 12.0).abs() * U * sigma.abs() / 4.0
    } else {
        0.0
    };
    (U * tau.abs() + octaves.abs() * period_error(period) + beyond) * SECOND_ORDER
}

/// The rounding of TRANSPOSE − 24: none from 12 to 48 (Sterbenz).
fn transpose_error(transpose: f64) -> f64 {
    if (12.0..=48.0).contains(&transpose) {
        0.0
    } else {
        U * (transpose - 24.0).abs()
    }
}

/// The bound that [`transposition`] documents.
fn transposition_bound(transpose: f64, period: f64, tau: f64) -> f64 {
    let octaves = ((transpose - 24.0) / 12.0).trunc();
    shift_bound(transpose, period, octaves, tau) + transpose_error(transpose) * SECOND_ORDER
}

/// Checks every line of one law against the bound that `bound` gives it,
/// `law` computing the result from the inputs: the bound must hold, and the
/// error is reported against it at its worst.
fn check(name: &str, law: impl Fn(&[f64]) -> f64, bound: impl Fn(&[f64], f64) -> f64) {
    let mut count = 0;
    let mut worst = 0.0_f64;
    for (_, numbers) in references().into_iter().filter(|(law, _)| *law == name) {
        let (inputs, value) = numbers.split_at(numbers.len() - 2);
        let y = law(inputs);
        let limit = bound(inputs, value[0]);
        let e = error(y, value[0], value[1]);
        assert!(
            e <= limit,
            "{name}{inputs:?}: {y:e}, exactly {:e}, error {e:e} > {limit:e}",
            value[0]
        );
        if limit > 0.0 {
            worst = worst.max(e / limit);
        }
        count += 1;
    }
    assert!(count > 0, "no reference of {name}");
    println!("{name}: {count} references, the worst error {worst:.3} of its bound");
}

#[test]
fn shifts_within_their_bounds() {
    check(
        "shift",
        |x| shift(x[0], TransposeOctave::Period, x[1]),
        |x, tau| shift_bound(x[0], x[1], (x[0] / 12.0).trunc(), tau),
    );
    check(
        "transposition",
        |x| transposition(x[0], TransposeOctave::Period, x[1]),
        |x, tau| transposition_bound(x[0], x[1], tau),
    );
}

#[test]
fn transposed_keys_within_their_bounds() {
    check(
        "transposed-keys",
        |x| transposed_key(x[0], x[1], x[2], TransposeOctave::Keys, 12.0),
        |x, key| {
            let first = x[0] + (x[1] - 24.0);
            (transpose_error(x[1]) + U * (first.abs() + key.abs())) * SECOND_ORDER
        },
    );
    check(
        "transposed-period",
        |x| transposed_key(x[0], x[1], x[2], TransposeOctave::Period, x[3]),
        |x, key| {
            let transposed = transposition(x[1], TransposeOctave::Period, x[3]);
            let shifted = shift(x[2], TransposeOctave::Period, x[3]);
            let first = x[0] + transposed;
            transposition_bound(x[1], x[3], transposed)
                + shift_bound(x[2], x[3], (x[2] / 12.0).trunc(), shifted)
                + U * (first.abs() + key.abs()) * SECOND_ORDER
        },
    );
}

#[test]
fn twelve_tet_within_its_bounds() {
    check(
        "pitch",
        |x| TwelveTet::new(x[0]).pitch(x[1]),
        |x, pitch| {
            let a4 = TwelveTet::new(x[0]).pitch(69.0).abs();
            U * (pitch.abs() + 1.62 * a4 + (x[1] - 69.0).abs() / 6.0) * SECOND_ORDER
        },
    );
    check(
        "key",
        |x| TwelveTet::new(x[0]).key(x[1]),
        |x, key| {
            let a4 = TwelveTet::new(x[0]).pitch(69.0);
            U * (key.abs() + 12.0 * (x[1] - a4).abs() + 19.44 * a4.abs()) * SECOND_ORDER
        },
    );
}

#[test]
fn ratios_and_detunes_within_their_bounds() {
    let gamma_4 = 4.0 * U / (1.0 - 4.0 * U);
    check("ratio", |x| ratio(x[0], x[1]), |_, r| gamma_4 * r.abs() * SECOND_ORDER);
    check(
        "detune",
        |x| detune(x[0], x[1]),
        |_, delta| 18.0 * U * delta.abs() * SECOND_ORDER,
    );
}

#[test]
fn fixed_pitches_within_their_bounds() {
    for (name, law) in [
        ("fixed-msfa", FixedDetune::Msfa),
        ("fixed-symmetric", FixedDetune::Symmetric),
        ("fixed-none", FixedDetune::None),
    ] {
        check(
            name,
            |x| fixed_pitch(x[0], x[1], x[2], law),
            |x, pitch| {
                // φ lies from 0 to 3.
                let sum = 3.0 + (x[1] / 100.0).abs();
                let detune = fixed_detune(x[2], law).abs();
                U * (pitch.abs() + LOG2_10 * (2.0 * sum + 3.0 + x[1].abs() / 100.0) + 2.0 * detune) * SECOND_ORDER
            },
        );
    }
}

#[test]
fn frequencies_within_their_bound() {
    // Two steps of the subnormals beside the relative bound, for results
    // below the normal range, where the products round at a fixed step.
    let subnormal = f64::from_bits(2);
    check(
        "frequency",
        |x| frequency(x[0], x[1]),
        |_, f| 2.86 * U * f.abs() * SECOND_ORDER + subnormal,
    );
}

#[test]
fn the_laws_give_these_exactly() {
    for sigma in [0.0, -12.0, 7.5, 1e300] {
        assert_eq!(shift(sigma, TransposeOctave::Keys, 5.0).to_bits(), sigma.to_bits());
        assert_eq!(shift(sigma, TransposeOctave::Period, 12.0).to_bits(), sigma.to_bits());
    }
    // An octave down under a period of seven keys spans seven keys, and the
    // whole octaves of a transposition are counted from TRANSPOSE itself.
    assert_eq!(shift(-12.0, TransposeOctave::Period, 7.0), -7.0);
    assert_eq!(shift(-13.0, TransposeOctave::Period, 7.0), -8.0);
    assert_eq!(transposition(0.0, TransposeOctave::Period, 7.0), -14.0);
    assert_eq!(transposition(36.0, TransposeOctave::Period, 19.0), 19.0);
    assert_eq!(transposition(24.0, TransposeOctave::Period, 7.0), 0.0);
    assert_eq!(transposition(0.0, TransposeOctave::Keys, 7.0), -24.0);
    let just_above = power_of_two(-60);
    assert_eq!(transposition(just_above, TransposeOctave::Period, 7.0), -24.0 + 5.0);
    assert_eq!(transposed_key(60.0, 24.0, 0.0, TransposeOctave::Period, 31.0), 60.0);
    // The ratios of the DX7's steps, and between them.
    assert_eq!(ratio(0.0, 0.0), 0.5);
    for coarse in 1..=31 {
        assert_eq!(ratio(f64::from(coarse), 0.0), f64::from(coarse));
    }
    assert_eq!(ratio(0.5, 0.0), 0.75);
    assert_eq!(ratio(-1.0, 0.0), 0.0);
    assert_eq!(ratio(1.0, 50.0), 1.5);
    // The fixed mode's decades, and φ falling from 3 to 0 between 3 and 4.
    let decades = [0.0, LOG2_10, 2.0 * LOG2_10, 3.0 * LOG2_10, 0.0];
    for (coarse, pitch) in decades.into_iter().enumerate() {
        let coarse = f64::from(u8::try_from(coarse).expect("small"));
        assert_eq!(
            fixed_pitch(coarse, 0.0, 7.0, FixedDetune::Msfa),
            pitch,
            "COARSE {coarse}"
        );
    }
    assert_eq!(fixed_pitch(3.5, 0.0, 7.0, FixedDetune::None), 1.5 * LOG2_10);
    assert_eq!(fixed_pitch(-1.0, 0.0, 7.0, FixedDetune::None), 3.0 * LOG2_10);
    assert_eq!(fixed_pitch(-0.5, 0.0, 7.0, FixedDetune::None), 1.5 * LOG2_10);
    // Just below 0, where C − ⌊C⌋ would round to 1 and φ to 0, it is exact.
    let phi = 3.0 * 1e-20;
    assert_eq!(fixed_pitch(-1e-20, 0.0, 7.0, FixedDetune::None), LOG2_10 * phi);
    // The detunes at and around 7.
    for pitch in [-100.0, 3.0, 8.78, 20.0] {
        assert_eq!(detune(7.0, pitch), 0.0);
    }
    assert_eq!(fixed_detune(3.0, FixedDetune::Msfa), 0.0);
    assert_eq!(fixed_detune(14.0, FixedDetune::Msfa), 7.0 * 13457.0 / 16_777_216.0);
    assert_eq!(fixed_detune(0.0, FixedDetune::Symmetric), -7.0 * 13457.0 / 16_777_216.0);
    assert_eq!(fixed_detune(14.0, FixedDetune::None), 0.0);
    // 12-TET at A4 and back, and the pitch held at 1023 octaves.
    let tuning = TwelveTet::new(440.0);
    assert_eq!(tuning.key(tuning.pitch(69.0)), 69.0);
    assert_eq!(tuning.pitch(81.0) - tuning.pitch(69.0), 1.0);
    assert_eq!(frequency(1.0, 2000.0), frequency(1.0, HIGHEST_PITCH));
    assert_eq!(frequency(2.5, HIGHEST_PITCH), f64::MAX);
    assert_eq!(frequency(-3.0, 1100.0), -f64::MAX);
    assert_eq!(frequency(1.0, -1100.0), 0.0);
    assert_eq!(ratio(1e300, 1e300), f64::MAX);
    // A large ratio brings a low pitch back into the normal range.
    assert_eq!(frequency(power_of_two(70), -1076.0), power_of_two(-1006));
}
