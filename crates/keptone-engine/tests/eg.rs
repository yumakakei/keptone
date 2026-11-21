// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The laws of the operator envelope's rates and segments (docs/MATH.md §4.1,
//! §4.3, §4.4) against their exact values at binary64 inputs, which
//! `scripts/eg_references.py` computed with fractions and mpmath
//! (`tests/data/eg.txt`) after checking the attack's closed form against
//! mpmath's integration of its law, each within the bound that its function
//! documents; the rate at every whole value of its scales, and the values that
//! the laws give exactly; the rate's limit, which the parameters beyond their
//! scales pass continuously (MATH §0.7); and the values that VERIFICATION
//! keeps provisional, pinned.

use keptone_engine::eg::{JUMP, KNEE, RateLaw, WAIT, attack, attack_progress, jump, rate, speed, wait};

/// u = 2⁻⁵³, the unit roundoff of binary64.
const U: f64 = f64::EPSILON / 2.0;

/// The bounds below are first order in u; their terms of the order of u² are
/// far below this share of them.
const SECOND_ORDER: f64 = 1.0 + 1e-9;

/// Two steps of the subnormals, beside a relative bound, for a result below
/// the normal range, where the last product rounds at a fixed step.
const SUBNORMAL: f64 = f64::from_bits(2);

/// The references of a law: each line's numbers, the inputs, the exact value as
/// a double-double, and for some laws more numbers after it.
fn references(law: &str) -> Vec<Vec<f64>> {
    let number = |word: &str| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits"));
    include_str!("data/eg.txt")
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

/// Checks a law's references, `inputs` numbers and then the exact value, within
/// the bound that `bound` gives each from the line and the exact value, and
/// reports the worst error against it.
fn check(law: &str, inputs: usize, compute: impl Fn(&[f64]) -> f64, bound: impl Fn(&[f64], f64) -> f64) {
    let (mut count, mut worst) = (0, 0.0_f64);
    for x in references(law) {
        let (hi, lo) = (x[inputs], x[inputs + 1]);
        let y = compute(&x);
        let limit = bound(&x, hi);
        let e = error(y, hi, lo);
        assert!(e <= limit, "{law}{x:?}: {y:e}, exactly {hi:e}, error {e:e} > {limit:e}");
        if limit > 0.0 {
            worst = worst.max(e / limit);
        }
        count += 1;
    }
    assert!(count > 100, "{count} references of {law}");
    println!("{law}: {count} references, the worst error {worst:.3} of its bound");
}

#[test]
fn the_rate_within_its_bound() {
    // 12uM on the scales and 41uM beyond them, M the largest magnitude of the
    // values at whole inputs, beside the floors' slack.
    check(
        "rate",
        3,
        |x| rate(x[0], x[1], x[2]),
        |x, _| {
            let on_scales = (0.0..=99.0).contains(&x[0]) && (0.0..=7.0).contains(&x[1]);
            let factor = if on_scales { 12.0 } else { 41.0 };
            factor * U * x[5] * SECOND_ORDER + x[6]
        },
    );
}

#[test]
fn the_rate_is_the_dx7s_at_every_whole_value_of_its_scales() {
    // min(63, ⌊41R/64⌋ + ⌊KRS·clamp(⌊k/3⌋ − 7, 0, 31)/8⌋), in integers.
    for r in 0..=99_u32 {
        for sensitivity in 0..=7_u32 {
            for key in 0..=127_u32 {
                let group = (key / 3).saturating_sub(7).min(31);
                let q = (41 * r / 64 + sensitivity * group / 8).min(63);
                assert_eq!(
                    rate(f64::from(r), f64::from(sensitivity), f64::from(key)),
                    f64::from(q),
                    "R {r} KRS {sensitivity} k {key}"
                );
            }
        }
    }
}

#[test]
fn past_its_scales_the_rate_passes_its_limit_continuously() {
    // At the ends of the scales and just beyond them, the rate moves by its
    // slope times the step: ⌊41R/64⌋ by 1 a step of R at most, and RS by 4 a
    // step of KRS at most.
    let nudge = f64::from_bits((1023 - 24) << 52);
    for key in [0.0, 30.0, 60.0, 90.0, 127.0, 200.0] {
        for (r, sensitivity) in [(99.0, 7.0), (99.0, 0.0), (0.0, 7.0), (0.0, 0.0), (50.0, 7.0)] {
            let at = rate(r, sensitivity, key);
            let moved = [
                rate(if r > 0.0 { r + nudge } else { r - nudge }, sensitivity, key),
                rate(
                    r,
                    if sensitivity > 0.0 {
                        sensitivity + nudge
                    } else {
                        sensitivity - nudge
                    },
                    key,
                ),
            ];
            for value in moved {
                assert!(
                    (value - at).abs() <= 4.0 * nudge + 1e-12,
                    "R {r} KRS {sensitivity} k {key}: {at} to {value}"
                );
            }
        }
    }
    // Past the bound: a full rate of 99 at the top key is 63 on the scale, and
    // a rate of 120 passes it by what 21 more steps of R give, ⌊41·120/64⌋ −
    // ⌊41·99/64⌋ = 13; a sensitivity of 14 passes it by RS's 27 more.
    assert_eq!(rate(99.0, 7.0, 127.0), 63.0);
    assert_eq!(rate(120.0, 7.0, 127.0), 63.0 + 13.0);
    assert_eq!(rate(99.0, 14.0, 127.0), 63.0 + 27.0);
}

#[test]
fn the_speed_within_its_bound_and_the_dx7s_at_whole_rates() {
    check(
        "speed",
        3,
        |x| {
            let law = if x[2] > 0.0 { RateLaw::Exponential } else { RateLaw::Dx7 };
            speed(x[0], x[1], law)
        },
        |x, rho| {
            let factor = if x[2] > 0.0 { 2.86 } else { 2.5 };
            factor * U * rho * SECOND_ORDER + SUBNORMAL
        },
    );
    // At q = 4j + i: 49 096/4096 · 2^j · (1 + i/4), exactly, one step in 4096
    // samples of the clock at q = 0.
    assert_eq!(speed(0.0, 49_096.0, RateLaw::Dx7), 11.986_328_125);
    for q in 0..=80_u32 {
        let (octave, quarter) = (q / 4, q % 4);
        let exponent = u64::from(1023 + octave - 14) << 52;
        let expected = 49_096.0 * f64::from(4 + quarter) * f64::from_bits(exponent);
        assert_eq!(speed(f64::from(q), 49_096.0, RateLaw::Dx7), expected, "q {q}");
    }
    // The exponential law agrees at multiples of 4, and a speed beyond binary64
    // is taken at its largest.
    assert_eq!(
        speed(32.0, 44_100.0, RateLaw::Exponential),
        speed(32.0, 44_100.0, RateLaw::Dx7)
    );
    assert_eq!(speed(1e300, 49_096.0, RateLaw::Dx7), f64::MAX);
    assert_eq!(speed(-1e300, 49_096.0, RateLaw::Exponential), 0.0);
}

/// The progress of an attack from a start to the knee, to the first order.
fn to_knee(start: f64) -> f64 {
    attack_progress(start, KNEE)
}

#[test]
fn the_attack_within_its_bound() {
    check(
        "attack",
        2,
        |x| attack(x[0], x[1]),
        |x, level| {
            let (start, progress) = (x[0], x[1]);
            let below = U * level.abs() + 4.24 * U * (level - start).abs();
            if start >= KNEE {
                return U * level.abs() * SECOND_ORDER;
            }
            let knee = to_knee(start);
            let above = U * (level.abs() + (progress - knee).abs()) + 4.62 * U * knee;
            let bound = if progress < knee * (1.0 - 16.0 * U) {
                below
            } else if progress > knee * (1.0 + 16.0 * U) {
                above
            } else {
                below.max(above)
            };
            bound * SECOND_ORDER
        },
    );
}

#[test]
fn the_attacks_progress_within_its_bound() {
    check(
        "attack_progress",
        2,
        |x| attack_progress(x[0], x[1]),
        |x, progress| {
            let (start, target) = (x[0], x[1]);
            let bound = if target <= start {
                0.0
            } else if start >= KNEE {
                U * progress
            } else if target <= KNEE {
                6.62 * U * progress
            } else {
                U * (progress + (target - KNEE)) + 4.62 * U * to_knee(start)
            };
            bound * SECOND_ORDER
        },
    );
}

#[test]
fn the_segments_laws_give_these_exactly() {
    // The attack starts where it is, and from the knee on rises as its
    // progress does; its progress to a target that does not lie above it is
    // none.
    for start in [-5000.0, 0.0, 16.0, JUMP, 3840.0, KNEE, 5000.0] {
        assert_eq!(attack(start, 0.0), start);
        assert_eq!(attack_progress(start, start), 0.0);
        assert_eq!(attack_progress(start, start - 1.0), 0.0);
    }
    assert_eq!(attack(KNEE, 100.0), KNEE + 100.0);
    assert_eq!(attack_progress(KNEE, KNEE + 100.0), 100.0);
    // From 3840, T* − T₀ = 512: the knee, 256 below the asymptote, at 256 ln 2.
    let knee_from_full_scale = to_knee(3840.0);
    assert!((knee_from_full_scale - 256.0 * std::f64::consts::LN_2).abs() <= 8.0 * U * knee_from_full_scale);
    // The jump: a start below 1716 jumps to it, or to a target below it; a
    // start at or above it, or not below its target, is kept. The knee lies
    // 256 steps above full scale.
    assert_eq!(JUMP, 1716.0);
    assert_eq!(KNEE, 4096.0);
    assert_eq!(jump(0.0, 3000.0), JUMP);
    assert_eq!(jump(16.0, 1000.0), 1000.0);
    assert_eq!(jump(-300.0, JUMP), JUMP);
    assert_eq!(jump(2000.0, 3000.0), 2000.0);
    assert_eq!(jump(100.0, 50.0), 100.0);
    // The wait: W for a segment that does not move, less as it moves up to a
    // step, none from a step on, and a twentieth in segment 0.
    assert_eq!(wait(false, 0.0), WAIT);
    assert_eq!(wait(true, 0.0), WAIT / 20.0);
    assert_eq!(wait(false, 0.5), WAIT / 2.0);
    assert_eq!(wait(false, -0.5), WAIT / 2.0);
    assert_eq!(wait(true, 0.25), 18.0);
    assert_eq!(wait(false, 1.0), 0.0);
    assert_eq!(wait(false, 16.0), 0.0);
}

#[test]
fn the_attack_reaches_its_targets_where_its_progress_says() {
    // T(Φ*) is the target, within the two functions' bounds: Φ*'s error moves
    // the level by its slope, at most 17 below the knee and 1 above it.
    for start in [-5000.0, -1.0, 0.0, 16.0, JUMP, 3000.0, 3840.0, 4095.0] {
        for target in [start + 1e-9, start + 1.0, 2000.0, 3000.0, 4000.0, KNEE, 4200.0, 6000.0] {
            if target <= start {
                continue;
            }
            let progress = attack_progress(start, target);
            let level = attack(start, progress);
            let tolerance = 64.0 * U * (target.abs() + start.abs() + 17.0 * progress);
            assert!((level - target).abs() <= tolerance, "{start} to {target}: {level}");
        }
    }
}

#[test]
fn the_provisional_values_stay_pinned() {
    // VERIFICATION V8: a wait of 480 steps at the segment's speed, a twentieth
    // in segment 0; V20: the rate scaling RS = ⌊KRS·clamp(⌊k/3⌋ − 7, 0, 31)/8⌋,
    // here at KRS 7: 0 up to key 26, then rising by 7/8 a group of three keys,
    // up to 27 from key 114 on. A test that fails here marks a value verified
    // and entered.
    assert_eq!(WAIT, 480.0);
    assert_eq!(wait(true, 0.0), 24.0);
    let scaling = |key: f64| rate(0.0, 7.0, key);
    assert_eq!(scaling(23.0), 0.0);
    assert_eq!(scaling(26.0), 0.0);
    assert_eq!(scaling(27.0), 1.0);
    assert_eq!(scaling(30.0), 2.0);
    assert_eq!(scaling(60.0), 11.0);
    assert_eq!(scaling(113.0), 26.0);
    assert_eq!(scaling(114.0), 27.0);
    assert_eq!(scaling(127.0), 27.0);
}
