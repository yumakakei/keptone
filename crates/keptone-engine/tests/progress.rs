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
//! and a glide move across ρ's corners, on the scales and beyond them, against
//! their exact values at binary64 inputs, which `scripts/progress_references.py`
//! computed with fractions and mpmath (`tests/data/progress.txt`), within the
//! bounds that the functions document; and the budget of parts that a piece
//! spends.

use keptone_engine::eg::RateLaw;
use keptone_engine::eg::progress::{Key, Rates, Reach, advance};

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
        },
        Key {
            key: 60.0,
            slope: 0.0,
            glide: 12.0,
            rate: 0.001,
        },
        Key {
            key: 60.0,
            slope: 0.0,
            glide: -12.0,
            rate: 0.001,
        },
        Key {
            key: 40.0,
            slope: -0.002,
            glide: -7.5,
            rate: 0.0003,
        },
        Key {
            key: 90.0,
            slope: 0.003,
            glide: 24.0,
            rate: 0.01,
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
