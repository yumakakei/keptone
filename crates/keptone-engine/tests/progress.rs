// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! An envelope segment's progress over a piece of a sample on which its rates'
//! key moves (docs/MATH.md §4.7): the progress over the piece and the instant
//! at which it reaches a target, under both rate laws, for keys that a line
//! and a glide move across ρ's corners, and the LFO's swing beside them, on the
//! scales and beyond them, against their exact values at binary64 inputs,
//! which `scripts/progress_references.py` computed with fractions and mpmath
//! (`tests/data/progress.txt`, `tests/data/progress_swung.txt`), within the
//! bounds that the functions document; a later key's law; and the budget of
//! parts that a piece spends.

use keptone_engine::eg::RateLaw;
use keptone_engine::eg::progress::{Key, Rates, Reach, Swing, advance};
use keptone_engine::pieces::Shape;

/// The references: each line's kind and its numbers.
fn references() -> Vec<(String, Vec<f64>)> {
    let number = |word: &str| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits"));
    include_str!("data/progress.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let mut words = line.split(' ');
            let kind = words.next().expect("a kind").to_owned();
            (kind, words.map(number).collect())
        })
        .collect()
}

/// The rates, key and length of a line's first ten numbers.
fn inputs(x: &[f64]) -> (Rates, Key, f64) {
    let [
        law,
        rate,
        sensitivity,
        clock_hz,
        sample_rate,
        key,
        slope,
        glide,
        glide_rate,
        length,
    ] = x[..10]
    else {
        panic!("ten inputs")
    };
    let rates = Rates {
        rate,
        sensitivity,
        clock_hz,
        law: if law == 0.0 { RateLaw::Dx7 } else { RateLaw::Exponential },
        sample_rate,
    };
    let key = Key {
        key,
        slope,
        glide,
        rate: glide_rate,
        swing: None,
    };
    (rates, key, length)
}

/// |y − (hi + lo)|.
fn error(y: f64, hi: f64, lo: f64) -> f64 {
    ((y - hi) - lo).abs()
}

#[test]
fn progresses_within_their_bound() {
    let (mut checked, mut worst, mut split) = (0, 0.0_f64, 0);
    for (kind, x) in references().into_iter().filter(|(kind, _)| kind == "progress") {
        let (rates, key, length) = inputs(&x);
        let [hi, lo, bound] = x[10..] else {
            panic!("{kind}: thirteen numbers")
        };
        let mut budget = 1e9;
        let Some(Reach::Beyond(progress)) = advance(&rates, &key, length, None, &mut budget) else {
            panic!("{x:?}: no progress within the budget")
        };
        let e = error(progress, hi, lo);
        assert!(
            e <= bound,
            "{x:?}: {progress:e}, exactly {hi:e}, error {e:e} > {bound:e}"
        );
        worst = worst.max(e / bound);
        checked += 1;
        if budget < 1e9 - 1.0 {
            split += 1;
        }
    }
    assert!(checked > 600 && split > 150, "{checked} pieces, {split} split");
    println!("progress: {checked} pieces, {split} of them split, the worst error {worst:.3} of its bound");
}

#[test]
fn reaches_within_their_bound() {
    let (mut checked, mut worst) = (0, 0.0_f64);
    for (_, x) in references().into_iter().filter(|(kind, _)| kind == "reach") {
        let (rates, key, length) = inputs(&x);
        let [target, hi, lo, bound] = x[10..] else {
            panic!("fourteen numbers")
        };
        let mut budget = 1e9;
        let Some(Reach::Within(at)) = advance(&rates, &key, length, Some(target), &mut budget) else {
            panic!("{x:?}: the target not reached")
        };
        let e = error(at, hi, lo);
        assert!(e <= bound, "{x:?}: {at:e}, exactly {hi:e}, error {e:e} > {bound:e}");
        worst = worst.max(e / bound);
        checked += 1;
    }
    assert!(checked > 600, "{checked} targets");
    println!("reach: {checked} targets, the worst error {worst:.3} of its bound");
}

#[test]
fn a_piece_spends_its_budget() {
    // A key across the ramps of keys 23 to 114 at KRS 7 under the dx7 law:
    // a stretch for each ramp's bends and for each multiple of 4 that q
    // crosses, and none past the budget.
    let rates = Rates {
        rate: 50.0,
        sensitivity: 7.0,
        clock_hz: 49_096.0,
        law: RateLaw::Dx7,
        sample_rate: 48_000.0,
    };
    let key = Key {
        key: 10.0,
        slope: 120.0,
        glide: 0.0,
        rate: 0.0,
        swing: None,
    };
    let mut budget = 1e9;
    let Some(Reach::Beyond(whole)) = advance(&rates, &key, 1.0, None, &mut budget) else {
        panic!("within the budget")
    };
    let spent = 1e9 - budget;
    assert!(spent > 40.0, "{spent} stretches");
    let mut short = spent - 1.0;
    assert_eq!(advance(&rates, &key, 1.0, None, &mut short), None);
    let mut exact = spent;
    assert_eq!(advance(&rates, &key, 1.0, None, &mut exact), Some(Reach::Beyond(whole)));
    assert_eq!(exact, 0.0);
    // Under the exponential law a glide's stretch takes the rule's parts, each
    // a part of the budget.
    let exponential = Rates {
        law: RateLaw::Exponential,
        ..rates
    };
    let glide = Key {
        key: 60.0,
        slope: 0.0,
        glide: 20.0,
        rate: 2.0,
        swing: None,
    };
    let mut budget = 1e9;
    assert!(advance(&exponential, &glide, 0.5, None, &mut budget).is_some());
    let spent = 1e9 - budget;
    assert!(spent > 50.0, "{spent} parts");
    let mut short = spent - 1.0;
    assert_eq!(advance(&exponential, &glide, 0.5, None, &mut short), None);
    // A key that holds still takes one stretch, and the target at 0 at once.
    let mut budget = 2.0;
    let still = Key::still(60.0);
    let Some(Reach::Beyond(steady)) = advance(&rates, &still, 1.0, None, &mut budget) else {
        panic!("one stretch")
    };
    assert_eq!((steady, budget), (rates.step(60.0), 1.0));
    let mut budget = 2.0;
    assert_eq!(
        advance(&rates, &still, 1.0, Some(0.0), &mut budget),
        Some(Reach::Within(0.0))
    );
    // A piece of no length makes no progress.
    let mut budget = 0.0;
    assert_eq!(advance(&rates, &key, 0.0, None, &mut budget), Some(Reach::Beyond(0.0)));
}

#[test]
fn a_later_key_follows_the_same_law() {
    // From any instant of a piece on, the key's law is the same: a piece that
    // starts τ samples in takes kᴿ(τ) at its start, bit for bit, and kᴿ(τ + σ)
    // at σ within the bound below; a glide keeps its sign until it arrives,
    // and then leaves a key that holds its line.
    let u = f64::EPSILON / 2.0;
    let keys = [
        Key::still(60.0),
        Key {
            key: 60.0,
            slope: 0.01,
            glide: 0.0,
            rate: 0.0,
            swing: None,
        },
        Key {
            key: 60.0,
            slope: 0.0,
            glide: 12.0,
            rate: 0.001,
            swing: None,
        },
        Key {
            key: 60.0,
            slope: 0.0,
            glide: -12.0,
            rate: 0.001,
            swing: None,
        },
        Key {
            key: 40.0,
            slope: -0.002,
            glide: -7.5,
            rate: 0.0003,
            swing: None,
        },
        Key {
            key: 90.0,
            slope: 0.003,
            glide: 24.0,
            rate: 0.01,
            swing: None,
        },
    ];
    for key in keys {
        assert_eq!(key.later(0.0), key);
        for tau in [0.5, 10.0, 300.0, 1e4] {
            let later = key.later(tau);
            assert_eq!(later.at(0.0).to_bits(), key.at(tau).to_bits(), "{key:?} at {tau}");
            assert_eq!(later.slope.to_bits(), key.slope.to_bits());
            assert!(
                later.glide == 0.0 || later.glide.signum() == key.glide.signum(),
                "{key:?} at {tau}: {later:?}"
            );
            // Every glide here arrives before 10⁴ samples.
            if tau >= 1e4 {
                assert_eq!((later.glide, later.rate), (0.0, 0.0), "{key:?} at {tau}");
            }
            for sigma in [0.25, 1.0, 100.0, 2e4] {
                // Key::at rounds within u(|k| + |b|t) + 4.35u(|d| + 3) + u|kᴿ|,
                // t the instant. The later key's start rounds within u(|k| +
                // |b|τ) and 4.35u(|d| + 3), which its glide's law carries at a
                // factor e^(−ρσ) ≤ 1, before its own evaluation; and τ + σ
                // rounds within u(τ + σ), which moves kᴿ by at most u(|b|t +
                // 0.37(|d| + 3)). Their sum lies below 3u|k| + 4u|b|t +
                // 17.8u(|d| + 3) + 2u|kᴿ|.
                let t = tau + sigma;
                let (a, b) = (later.at(sigma), key.at(t));
                let bound = 20.0 * u * (key.key.abs() + key.slope.abs() * t + key.glide.abs() + 3.0 + b.abs());
                assert!((a - b).abs() <= bound, "{key:?} at {tau} + {sigma}: {a} against {b}");
            }
        }
    }
}

/// The swinging keys' references: each line's kind and its numbers.
fn swung_references() -> Vec<(String, Vec<f64>)> {
    let number = |word: &str| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits"));
    include_str!("data/progress_swung.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let mut words = line.split(' ');
            let kind = words.next().expect("a kind").to_owned();
            (kind, words.map(number).collect())
        })
        .collect()
}

/// The rates, key and length of a swinging key's line's first fifteen
/// numbers: its line and glide as [`inputs`] reads them, and its swing.
fn swung_inputs(x: &[f64]) -> (Rates, Key, f64) {
    let [wave, depth, depth_slope, first, second, length] = x[9..15] else {
        panic!("fifteen inputs")
    };
    let mut line = x[..10].to_vec();
    line[9] = length;
    let (rates, key, _) = inputs(&line);
    let shape = if wave == 0.0 {
        Shape::Sine {
            turns: first,
            step: second,
        }
    } else {
        Shape::Linear {
            value: first,
            slope: second,
        }
    };
    let swing = Swing {
        depth,
        depth_slope,
        shape,
    };
    (
        rates,
        Key {
            swing: Some(swing),
            ..key
        },
        length,
    )
}

#[test]
fn swinging_keys_progress_and_reach_within_their_bounds() {
    // Keys that the LFO swings beside a line and a glide, as it moves the
    // level's key under `scaling.key = pitch`: the progress over the piece and
    // the instant at which it reaches a target, against their exact values at
    // binary64 inputs, within the relative bound that holds for any stretches
    // that the sweep takes (`scripts/progress_references.py`). The first eight
    // turn past a corner of ρ with both their ends below it, under each law:
    // one whose swing's depth a fade moves from 0 and one whose glide turns
    // it, which only the whole bound of their curvature finds, the second
    // again with the corner 10⁻⁶ keys below its peak, which no halving
    // reaches, so that only a range widened by all but the key's own rounding
    // holds the corner, and a sine turned against its line, which only the
    // sine's slope of the right sign finds.
    let (mut progresses, mut reaches, mut halved) = (0, 0, 0);
    let (mut worst_progress, mut worst_reach) = (0.0_f64, 0.0_f64);
    for (kind, x) in swung_references() {
        let (rates, key, length) = swung_inputs(&x);
        let mut budget = 1e9;
        if kind == "progress" {
            let [hi, lo, bound] = x[15..] else {
                panic!("eighteen numbers")
            };
            let Some(Reach::Beyond(progress)) = advance(&rates, &key, length, None, &mut budget) else {
                panic!("{x:?}: no progress within the budget")
            };
            let e = error(progress, hi, lo);
            assert!(
                e <= bound,
                "{x:?}: {progress:e}, exactly {hi:e}, error {e:e} > {bound:e}"
            );
            worst_progress = worst_progress.max(e / bound);
            progresses += 1;
            if budget < 1e9 - 2.0 {
                halved += 1;
            }
        } else {
            let [target, hi, lo, bound] = x[15..] else {
                panic!("nineteen numbers")
            };
            let Some(Reach::Within(at)) = advance(&rates, &key, length, Some(target), &mut budget) else {
                panic!("{x:?}: the target not reached")
            };
            let e = error(at, hi, lo);
            assert!(e <= bound, "{x:?}: {at:e}, exactly {hi:e}, error {e:e} > {bound:e}");
            worst_reach = worst_reach.max(e / bound);
            reaches += 1;
        }
    }
    assert!(
        progresses == 248 && reaches > 200 && halved > 40,
        "{progresses}, {reaches}, {halved}"
    );
    println!(
        "swinging keys: {progresses} pieces, {halved} of them in more than two stretches, the worst error \
         {worst_progress:.3} of its bound; {reaches} targets, the worst {worst_reach:.3}"
    );
}

#[test]
fn a_later_swing_follows_the_same_law() {
    // A swinging key from τ samples on: kᴿ(τ) at its start bit for bit, and
    // kᴿ(τ + σ) at σ within the bound below.
    let u = f64::EPSILON / 2.0;
    let swing = |shape: Shape, depth: f64, depth_slope: f64| Swing {
        depth,
        depth_slope,
        shape,
    };
    let keys = [
        (
            60.0,
            0.0,
            swing(
                Shape::Sine {
                    turns: 0.3,
                    step: 0.001,
                },
                12.0,
                0.0,
            ),
        ),
        (40.0, 0.002, swing(Shape::Sine { turns: 0.9, step: 0.25 }, -30.0, 0.01)),
        (
            70.0,
            -0.01,
            swing(
                Shape::Linear {
                    value: -0.5,
                    slope: 0.004,
                },
                6.0,
                0.002,
            ),
        ),
    ];
    for (start, slope, swing) in keys {
        let key = Key {
            key: start,
            slope,
            glide: 0.0,
            rate: 0.0,
            swing: Some(swing),
        };
        assert_eq!(key.later(0.0), key);
        for tau in [0.5, 10.0, 300.0] {
            let later = key.later(tau);
            assert_eq!(later.at(0.0).to_bits(), key.at(tau).to_bits(), "{key:?} at {tau}");
            for sigma in [0.25, 1.0, 100.0] {
                // Each phase lies within u(3 + st) turns of its place, so that
                // the two sines lie within 2π·2u(3 + st)|A| of each other with
                // their own 1.46u|A| each; their depths within 2u(|A| + |A′|t);
                // and the line's parts as Key::at's: within 20u(|k| + |b|t +
                // |kᴿ|) for the line and 8u|A|(2π(3 + st) + 3) + 4u|A′|t for
                // the swing, |A| at most |A₀| + |A′|t.
                let t = tau + sigma;
                let (a, b) = (later.at(sigma), key.at(t));
                let turns = match swing.shape {
                    Shape::Sine { step, .. } => step.abs() * t,
                    _ => 0.0,
                };
                let depth = swing.depth.abs() + swing.depth_slope.abs() * t;
                let bound = 20.0 * u * (start.abs() + slope.abs() * t + b.abs())
                    + 8.0 * u * depth * (core::f64::consts::TAU * (3.0 + turns) + 3.0)
                    + 4.0 * u * swing.depth_slope.abs() * t;
                assert!((a - b).abs() <= bound, "{key:?} at {tau} + {sigma}: {a} against {b}");
            }
        }
    }
}

#[test]
fn a_halving_takes_a_part_of_the_budget() {
    // k = K₀ − 0.05·sin 2πτ over a sample at KRS 7 and rate 50, where q is 42
    // at key 59 and 43 at 60, ρ's corners at 59 and 60 between: its curvature
    // is at most M = 0.05·(2π)², 1.97. At K₀ = 59.9 the piece's range, its
    // ends' 59.9 widened by M/8, 0.247, holds the corner at 60, and its slope
    // at the middle, 0.31, lies below M, so the sweep halves it; each half's
    // range, widened by M/32, 0.062, holds no corner, and each is taken whole:
    // a halving and two stretches, three parts. At K₀ = 59.5 the whole piece's
    // widened range holds none: one part.
    let rates = Rates {
        rate: 50.0,
        sensitivity: 7.0,
        clock_hz: 49_096.0,
        law: RateLaw::Dx7,
        sample_rate: 48_000.0,
    };
    let key = |start: f64| Key {
        key: start,
        slope: 0.0,
        glide: 0.0,
        rate: 0.0,
        swing: Some(Swing {
            depth: 0.05,
            depth_slope: 0.0,
            shape: Shape::Sine { turns: 0.0, step: 1.0 },
        }),
    };
    for (start, parts) in [(59.9, 3.0), (59.5, 1.0)] {
        let mut budget = 10.0;
        assert!(advance(&rates, &key(start), 1.0, None, &mut budget).is_some());
        assert_eq!(10.0 - budget, parts, "at {start}");
        let mut short = parts - 0.5;
        assert_eq!(advance(&rates, &key(start), 1.0, None, &mut short), None, "at {start}");
    }
}

#[test]
fn a_swing_s_sweep_spends_its_budget() {
    // A fast sine swinging the key over a few groups at KRS 7 crosses ρ's
    // corners many times within the piece: a stretch for each, and halvings
    // where it turns near them; past the budget, none.
    let rates = Rates {
        rate: 50.0,
        sensitivity: 7.0,
        clock_hz: 49_096.0,
        law: RateLaw::Dx7,
        sample_rate: 48_000.0,
    };
    let key = Key {
        key: 60.0,
        slope: 0.0,
        glide: 0.0,
        rate: 0.0,
        swing: Some(Swing {
            depth: 7.0,
            depth_slope: 0.0,
            shape: Shape::Sine { turns: 0.1, step: 3.0 },
        }),
    };
    let mut budget = 1e9;
    let Some(Reach::Beyond(whole)) = advance(&rates, &key, 1.0, None, &mut budget) else {
        panic!("progress within the budget")
    };
    let spent = 1e9 - budget;
    assert!(spent > 50.0 && whole > 0.0, "{spent} parts, {whole}");
    let mut short = spent - 1.0;
    assert_eq!(advance(&rates, &key, 1.0, None, &mut short), None);
}
