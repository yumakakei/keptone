// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The controllers, the amplitude modulation and the bend (docs/MATH.md §7.1,
//! §7.2, §2.4) against the exact values of their laws at binary64 inputs that
//! `scripts/control_references.py` computed (`tests/data/control.txt`), each
//! within the bound that its function documents, which the script computed
//! beside it from the exact values; and the values that they give exactly,
//! the limits of MATH §0.7 and the provisional values of VERIFICATION V14
//! among them.

use std::collections::BTreeMap;

use keptone_engine::control::{
    Limited, Source, amplitude_sensitivity, attenuation, bend, bend_position, bias, controllers, depth, kept,
    lfo_pitch, modulation, pitch_sensitivity,
};
use keptone_model::settings::{self, SPECS};

/// The bounds below are first order in u; their terms of the order of u² are
/// far below this share of them.
const SECOND_ORDER: f64 = 1.0 + 1e-6;

/// Two steps of the subnormals, beside a bound, for values in the subnormal
/// range, where each rounding adds up to half a step.
const SUBNORMAL: f64 = f64::from_bits(2);

/// The lines of the references: each a word and its numbers' bits, u64 values
/// or binary64 ones as the file's header says.
fn lines() -> Vec<(String, Vec<u64>)> {
    let number = |word: &str| u64::from_str_radix(word, 16).expect("hexadecimal digits");
    include_str!("data/control.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let mut words = line.split(' ');
            let word = words.next().expect("a word").to_owned();
            (word, words.map(number).collect())
        })
        .collect()
}

/// |y − (hi + lo)|: y − hi is exact where the two are close (Sterbenz), and
/// the subtraction of lo rounds at the order of u² of the result.
fn error(y: f64, hi: f64, lo: f64) -> f64 {
    ((y - hi) - lo).abs()
}

/// The count of controllers that a line gives first, as a binary64.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole binary64 of [0, 4] converts exactly"
)]
fn count(value: f64) -> usize {
    assert!((0.0..=4.0).contains(&value) && value.fract() == 0.0, "a count: {value}");
    value as usize
}

/// The controllers of a line: their count, then each one's position and range.
fn sources(x: &[f64]) -> (Vec<Source>, &[f64]) {
    let n = count(x[0]);
    let sources = x[1..=2 * n]
        .chunks(2)
        .map(|pair| Source {
            position: pair[0],
            range: pair[1],
        })
        .collect();
    (sources, &x[1 + 2 * n..])
}

#[test]
fn the_laws_within_their_bounds() {
    let mut counts: BTreeMap<String, (usize, f64)> = BTreeMap::new();
    for (word, bits) in lines() {
        let x: Vec<f64> = bits.iter().copied().map(f64::from_bits).collect();
        // Each check: the engine's value, the exact one as a double-double,
        // and the bound of its error.
        let checks: Vec<(f64, f64, f64, f64)> = match word.as_str() {
            "depth" => vec![(depth(x[0]), x[1], x[2], x[3])],
            "controllers" | "bias" => {
                let (sources, rest) = sources(&x);
                let law = if word == "bias" {
                    bias(&sources)
                } else {
                    controllers(&sources)
                };
                vec![
                    (law.value, rest[0], rest[1], rest[4]),
                    (law.on_scale, rest[2], rest[3], rest[5]),
                ]
            }
            "modulation" => {
                let depth = Limited {
                    value: x[2],
                    on_scale: x[3],
                };
                let law = modulation(x[0], x[1], depth);
                vec![(law.value, x[4], x[5], x[8]), (law.on_scale, x[6], x[7], x[9])]
            }
            "pitch_sensitivity" => vec![(pitch_sensitivity(x[0]), x[1], x[2], x[3])],
            "amplitude_sensitivity" => vec![(amplitude_sensitivity(x[0]), x[1], x[2], x[3])],
            "lfo_pitch" => vec![(lfo_pitch(x[0], x[1], x[2]), x[3], x[4], x[5])],
            "kept" => {
                let bias = Limited {
                    value: x[0],
                    on_scale: x[1],
                };
                let depth = Limited {
                    value: x[2],
                    on_scale: x[3],
                };
                vec![(kept(bias, depth, x[4]), x[5], x[6], x[7])]
            }
            "attenuation" => vec![(attenuation(x[0], x[1], x[2]), x[3], x[4], x[5])],
            "position" => {
                let bits_of = u32::try_from(bits[1]).expect("a count of bits");
                let position = bend_position(bits[0], bits_of).expect("a position");
                vec![(position, x[2], x[3], x[4])]
            }
            "noposition" => {
                let bits_of = u32::try_from(bits[1]).expect("a count of bits");
                assert_eq!(bend_position(bits[0], bits_of), None, "{} of {} bits", bits[0], bits[1]);
                continue;
            }
            "bend" => vec![(bend(x[0], x[1], x[2]), x[3], x[4], x[5])],
            other => panic!("a line of {other}"),
        };
        let entry = counts.entry(word.clone()).or_insert((0, 0.0));
        entry.0 += 1;
        for (y, hi, lo, limit) in checks {
            let limit = limit * SECOND_ORDER + SUBNORMAL;
            let e = error(y, hi, lo);
            assert!(
                e <= limit,
                "{word}{:?}: {y:e}, exactly {hi:e}, error {e:e} > {limit:e}",
                &x[..x.len().min(9)]
            );
            entry.1 = entry.1.max(e / limit);
        }
    }
    for (word, least) in [
        ("depth", 1000),
        ("controllers", 600),
        ("bias", 600),
        ("modulation", 600),
        ("pitch_sensitivity", 600),
        ("amplitude_sensitivity", 600),
        ("lfo_pitch", 600),
        ("kept", 600),
        ("attenuation", 600),
        ("position", 300),
        ("bend", 1200),
    ] {
        let (n, worst) = counts.get(word).copied().unwrap_or_default();
        assert!(n >= least, "{n} lines of {word}");
        println!("{word}: {n} lines, the worst error {worst:.3} of its bound");
    }
}

#[test]
fn the_laws_give_these_exactly() {
    // A step scaled as the firmware scales it: ⌊165v/64⌋/256, 255/256 at 99,
    // linear between the steps and as written beyond the scale.
    assert_eq!(depth(0.0), 0.0);
    assert_eq!(depth(1.0), 2.0 / 256.0);
    assert_eq!(depth(50.0), 128.0 / 256.0);
    assert_eq!(depth(99.0), 255.0 / 256.0);
    assert_eq!(depth(0.5), 1.0 / 256.0);
    assert_eq!(depth(-1.0), -3.0 / 256.0);
    assert_eq!(depth(150.0), 386.0 / 256.0);
    assert!(depth(1e300).is_finite() && depth(-f64::MAX).is_finite());
    // The controllers' depth saturates at 255/256 on the scales; a range beyond
    // its scale passes the limit by what it adds (MATH §0.7).
    let wheel = |position: f64, range: f64| Source { position, range };
    let none = controllers(&[]);
    assert_eq!((none.value, none.on_scale), (0.0, 0.0));
    let full = controllers(&[wheel(1.0, 99.0)]);
    assert_eq!((full.value, full.on_scale), (255.0 / 256.0, 255.0 / 256.0));
    let two = controllers(&[wheel(1.0, 99.0), wheel(1.0, 99.0)]);
    assert_eq!((two.value, two.on_scale), (255.0 / 256.0, 255.0 / 256.0));
    let beyond = controllers(&[wheel(1.0, 150.0)]);
    assert_eq!((beyond.value, beyond.on_scale), (386.0 / 256.0, 255.0 / 256.0));
    let half = controllers(&[wheel(0.5, 99.0)]);
    assert_eq!(half.value, 255.0 / 512.0);
    // The modulation's depth: the LFO's, faded, and the controllers'.
    let depth_of = |parameter: f64, fade: f64, controllers: Limited| modulation(parameter, fade, controllers);
    assert_eq!(depth_of(99.0, 1.0, none).value, 255.0 / 256.0);
    assert_eq!(depth_of(99.0, 0.5, none).value, 255.0 / 512.0);
    assert_eq!(depth_of(99.0, 1.0, full).value, 255.0 / 256.0);
    assert_eq!(depth_of(0.0, 1.0, half).value, 255.0 / 512.0);
    let deep = depth_of(150.0, 1.0, none);
    assert_eq!((deep.value, deep.on_scale), (386.0 / 256.0, 255.0 / 256.0));
    // The EG bias: a controller at rest attenuates by its whole range, one at
    // its far end not at all.
    let rest = bias(&[wheel(0.0, 99.0)]);
    assert_eq!((rest.value, rest.on_scale), (255.0 / 256.0, 255.0 / 256.0));
    assert_eq!(bias(&[wheel(1.0, 99.0)]).value, 0.0);
    assert_eq!(bias(&[wheel(0.5, 99.0)]).value, 255.0 / 512.0);
    assert_eq!(bias(&[]).value, 0.0);
    // The sensitivities, along their end steps beyond the scales.
    assert_eq!(pitch_sensitivity(3.0), 33.0 / 256.0);
    assert_eq!(pitch_sensitivity(7.0), 255.0 / 256.0);
    assert_eq!(pitch_sensitivity(-1.0), -10.0 / 256.0);
    assert_eq!(pitch_sensitivity(8.0), 357.0 / 256.0);
    assert_eq!(pitch_sensitivity(2.5), 26.5 / 256.0);
    assert_eq!(amplitude_sensitivity(0.0), 0.0);
    assert_eq!(amplitude_sensitivity(3.0), 1.0);
    assert_eq!(amplitude_sensitivity(4.0), 401.0 / 255.0);
    assert_eq!(amplitude_sensitivity(-1.0), -66.0 / 255.0);
    // The part kept, 1/256 at the deepest on the scales and 1 at the wave's top,
    // and the level by which it lowers an operator: Aₘₐₓ·α at 1/256; silent at
    // 0 and below for AMS above 0, the largest gain below 0, none at AMS 0.
    let deepest = Limited {
        value: 255.0 / 256.0,
        on_scale: 255.0 / 256.0,
    };
    let unbiased = Limited {
        value: 0.0,
        on_scale: 0.0,
    };
    assert_eq!(kept(unbiased, deepest, -1.0), 1.0 / 256.0);
    assert_eq!(kept(unbiased, deepest, 1.0), 1.0);
    assert_eq!(kept(rest, deepest, -1.0), 1.0 / 256.0);
    assert_eq!(attenuation(2048.0, 3.0, 1.0 / 256.0), 2048.0);
    assert_eq!(attenuation(2048.0, 3.0, 1.0), 0.0);
    assert_eq!(attenuation(2048.0, 3.0, 0.0), f64::MAX);
    assert_eq!(attenuation(2048.0, 3.0, -0.5), f64::MAX);
    assert_eq!(attenuation(2048.0, -1.0, 0.0), -f64::MAX);
    assert_eq!(attenuation(2048.0, 0.0, 0.0), 0.0);
    assert_eq!(attenuation(0.0, 3.0, 0.0), 0.0);
    // The bend's position: both ends reach the range.
    assert_eq!(bend_position(0, 14), Some(-1.0));
    assert_eq!(bend_position(8192, 14), Some(0.0));
    assert_eq!(bend_position(16383, 14), Some(1.0));
    assert_eq!(bend_position(0, 7), Some(-1.0));
    assert_eq!(bend_position(64, 7), Some(0.0));
    assert_eq!(bend_position(127, 7), Some(1.0));
    assert_eq!(bend_position(128, 7), None);
    assert_eq!(bend_position(1, 1), None);
    assert_eq!(bend_position(1, 54), None);
    // The bend: x·RANGE at STEP 0; a staircase of whole steps from STEP 1 on,
    // whatever the range, none above 12; linear between whole STEPs, and below
    // 0 along its first step (MATH §2.4).
    assert_eq!(bend(1.0, 2.0, 0.0), 2.0);
    assert_eq!(bend(-1.0, 2.0, 0.0), -2.0);
    assert_eq!(bend(0.5, 12.0, 0.0), 6.0);
    assert_eq!(bend(1.0, 2.0, 1.0), 12.0);
    assert_eq!(bend(-1.0, 2.0, 1.0), -12.0);
    assert_eq!(bend(0.04, 2.0, 1.0), 0.0);
    assert_eq!(bend(0.1, 2.0, 1.0), 1.0);
    assert_eq!(bend(1.0, 2.0, 12.0), 12.0);
    assert_eq!(bend(0.6, 2.0, 12.0), 0.0);
    assert_eq!(bend(1.0, 2.0, 13.0), 0.0);
    assert_eq!(bend(1.0, 2.0, 0.5), 7.0);
    assert_eq!(bend(1.0, 2.0, -0.5), -3.0);
    assert_eq!(bend(1.0, 2.0, -1.0), -8.0);
    // The staircase's floor is of the exact product: 12.5 times the binary64
    // just below 0.4 lies below 5, which the product rounded to binary64
    // reaches, and 1.5 times 2/3 in binary64 below 1.
    let below = f64::from_bits(0x3fd9_9999_9999_9999);
    assert!(below < 0.4);
    assert_eq!(bend(below, 2.0, 1.0), 4.0);
    assert_eq!(bend(0.4, 2.0, 1.0), 5.0);
    assert_eq!(bend(2.0 / 3.0, 2.0, 12.0), 0.0);
    assert_eq!(bend(-(2.0 / 3.0), 2.0, 7.0), 0.0);
}

#[test]
fn the_provisional_values_stay_pinned() {
    // VERIFICATION V14: α_AMS of MSFA's table, and Aₘₐₓ, `am.max_steps`, 2048,
    // the firmware's 8 octaves taken whole at AMS 3. A test that fails here
    // marks a value verified and entered.
    assert_eq!(amplitude_sensitivity(1.0), 66.0 / 255.0);
    assert_eq!(amplitude_sensitivity(2.0), 109.0 / 255.0);
    assert_eq!(amplitude_sensitivity(3.0), 1.0);
    let number = settings::number_of("am.max_steps").expect("the setting");
    assert_eq!(
        SPECS[usize::from(number)].default,
        settings::Value::Number(2048.0),
        "am.max_steps"
    );
}
