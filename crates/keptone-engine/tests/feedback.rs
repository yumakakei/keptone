// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The feedback (docs/MATH.md §9) against the exact values of its laws at
//! binary64 inputs that `scripts/feedback_references.py` computed
//! (`tests/data/feedback.txt`): κ, β, the point solution and the clamped one
//! within the bounds that their functions document; the DX7's recurrence and
//! the relaxation's samples, in the order of their time, within the bounds of
//! their errors that the script followed through them; the relaxation's order
//! 2 as its substeps double; and the values that they give exactly.

use std::f64::consts::TAU;

use keptone_engine::feedback::{Discrete, Relaxation, beta, clamped, kappa, lag, point, substeps};
use keptone_math::sin2pi;

/// u = 2⁻⁵³, the unit roundoff of binary64.
const U: f64 = f64::EPSILON / 2.0;

/// The bounds below are first order in u; their terms of the order of u² are
/// far below this share of them.
const SECOND_ORDER: f64 = 1.0 + 1e-6;

/// Two steps of the subnormals, beside a bound, for values in the subnormal
/// range.
const SUBNORMAL: f64 = f64::from_bits(2);

/// `feedback.max_iterations`'s default (MATH §17).
const ITERATIONS: u32 = 16;

/// The default of `feedback.tau_s`, 1.5/49096 s (MATH §17).
const TAU_S: f64 = 1.5 / 49_096.0;

/// The lines of the references: each a word and its numbers.
fn lines() -> Vec<(String, Vec<f64>)> {
    let number = |word: &str| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits"));
    include_str!("data/feedback.txt")
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

/// A whole count of substeps from a line.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole binary64 of [1, 64] converts exactly"
)]
fn count(value: f64) -> u32 {
    assert!(
        (1.0..=64.0).contains(&value) && value.fract() == 0.0,
        "a count: {value}"
    );
    value as u32
}

/// Asserts that a value lies within its bound, and gives its share of it.
fn within(name: &str, y: f64, hi: f64, lo: f64, bound: f64) -> f64 {
    let limit = bound * SECOND_ORDER + SUBNORMAL;
    let e = error(y, hi, lo);
    assert!(e <= limit, "{name}: {y:e}, exactly {hi:e}: {e:e} > {limit:e}");
    e / limit
}

#[test]
fn the_laws_within_their_bounds() {
    let (mut counted, mut worst) = ([0_usize; 4], [0.0_f64; 4]);
    for (word, x) in lines() {
        let (index, share) = match word.as_str() {
            "kappa" => (0, within("kappa", kappa(x[0]), x[1], x[2], x[3])),
            "beta" => (1, within("beta", beta(x[0], x[1]), x[2], x[3], x[4])),
            "point" => (
                2,
                within("point", point(x[0], x[1], x[2], ITERATIONS), x[3], x[4], x[5]),
            ),
            "clamped" => {
                let y = clamped(x[0], x[1], x[2], x[3], ITERATIONS);
                (3, within("clamped", y, x[4], x[5], x[6]))
            }
            _ => continue,
        };
        counted[index] += 1;
        worst[index] = worst[index].max(share);
    }
    assert!(counted.iter().all(|&n| n >= 400), "{counted:?}");
    println!("κ, β, point and clamped: {counted:?} lines, the worst errors {worst:.3?} of their bounds");
}

/// A scenario's state as the test drives it.
enum Run {
    /// The DX7's recurrence at a κ.
    Discrete(Discrete, f64),
    /// The relaxation at κ, τ and the engine's rate.
    Relaxation(Relaxation, f64, f64, f64),
}

#[test]
fn the_recurrence_and_the_relaxation_run_as_the_references_do() {
    let (mut steps, mut samples, mut worst_step, mut worst_output, mut worst_lag) = (0, 0, 0.0_f64, 0.0_f64, 0.0_f64);
    let mut run: Option<Run> = None;
    for (word, x) in lines() {
        match word.as_str() {
            "kappa" | "beta" | "point" | "clamped" => {}
            "discrete" => run = Some(Run::Discrete(Discrete::new(), x[0])),
            "relaxation" => run = Some(Run::Relaxation(Relaxation::new(), x[0], x[1], x[2])),
            "step" => {
                let Some(Run::Discrete(state, kappa)) = run.as_mut() else {
                    panic!("a recurrence first");
                };
                let y = state.output(x[0], x[1], *kappa);
                worst_step = worst_step.max(within("step", y, x[2], x[3], x[4]));
                steps += 1;
            }
            "sample" => {
                let Some(Run::Relaxation(state, kappa, tau, rate)) = run.as_mut() else {
                    panic!("a relaxation first");
                };
                let m = count(x[0]);
                let end = 1 + 2 * (usize::try_from(m).expect("a count") + 1);
                let pairs = &x[1..end];
                let at = |j: u32| {
                    let j = usize::try_from(j).expect("an index");
                    (pairs[2 * j], pairs[2 * j + 1])
                };
                let y = state.output(at, m, *kappa, *tau, *rate, ITERATIONS);
                let rest = &x[end..];
                worst_output = worst_output.max(within("output", y, rest[0], rest[1], rest[2]));
                worst_lag = worst_lag.max(within("lag", state.lagged(), rest[3], rest[4], rest[5]));
                samples += 1;
            }
            other => panic!("a line of {other}"),
        }
    }
    assert!(steps >= 700 && samples >= 500, "{steps} steps, {samples} samples");
    println!(
        "{steps} steps of the recurrence, {samples} samples of the relaxation; the worst errors {worst_step:.3}, {worst_output:.3} and {worst_lag:.3} of their bounds"
    );
}

/// The relaxation's Y after a count of samples at a phase rising at a
/// frequency and a gain falling at a slope, with each sample in a count of
/// substeps.
fn relaxed(samples: u32, substeps: u32, frequency: f64, gain: f64, slope: f64, kappa: f64) -> f64 {
    let rate = 48_000.0;
    let mut state = Relaxation::new();
    for n in 1..=samples {
        let at = |j: u32| {
            let t = (f64::from(n - 1) + f64::from(j) / f64::from(substeps)) / rate;
            ((0.125 + frequency * t).fract(), gain + slope * t)
        };
        state.output(at, substeps, kappa, TAU_S, rate, ITERATIONS);
    }
    state.lagged()
}

#[test]
fn the_relaxation_converges_with_the_order_two() {
    // Halving the substeps' length divides the trapezoidal rule's error by 4:
    // the differences of solutions with 1, 2, 4 and 8 substeps fall so, as
    // MATH §16 asks, at loop gains on either side of 1.
    for (frequency, gain, kappa) in [
        (110.0, 1.0, 0.15),
        (110.0, 1.2, 0.15),
        (220.0, 1.0, 0.2),
        (55.0, 1.6, 0.11),
    ] {
        let solutions: Vec<f64> = [1, 2, 4, 8]
            .into_iter()
            .map(|m| relaxed(1500, m, frequency, gain, -2.0, kappa))
            .collect();
        let differences: Vec<f64> = solutions.windows(2).map(|pair| (pair[0] - pair[1]).abs()).collect();
        for pair in differences.windows(2) {
            let order = pair[0] / pair[1];
            assert!(
                (3.5..=4.6).contains(&order),
                "{frequency} Hz, β {}: order {order}",
                TAU * kappa * gain
            );
        }
        println!(
            "{frequency} Hz at β {:.2}: differences {differences:?}",
            TAU * kappa * gain
        );
    }
}

#[test]
fn the_feedback_gives_these_exactly() {
    // κ: 2^(FB − 8) at the steps from 1, linear between them from 0 at FB 0, and
    // along its first step below 0; binary64's largest finite number far
    // beyond the scale.
    assert_eq!(kappa(0.0), 0.0);
    assert_eq!(kappa(1.0), 1.0 / 128.0);
    assert_eq!(kappa(7.0), 0.5);
    assert_eq!(kappa(8.0), 1.0);
    assert_eq!(kappa(0.5), 1.0 / 256.0);
    assert_eq!(kappa(1.5), 1.5 / 128.0);
    assert_eq!(kappa(6.5), 0.375);
    assert_eq!(kappa(-1.0), -1.0 / 128.0);
    assert_eq!(kappa(2000.0), f64::MAX);
    // β at full scale, g = 2: FB 7 gives 2π.
    assert_eq!(beta(0.5, 2.0), TAU);
    // The point solution: 0 at a phase of 0 and of a half turn, the sine without
    // feedback, a negative β half a turn away.
    assert_eq!(point(0.0, 1.5, 0.8, ITERATIONS), 0.0);
    assert_eq!(point(0.5, 1.5, 0.8, ITERATIONS).abs(), 0.0);
    assert_eq!(point(0.25, 1.5, 0.0, ITERATIONS), 1.5);
    for phase in [0.1, 0.3, 0.77, 0.9] {
        assert_eq!(
            point(phase, 1.5, -0.6, ITERATIONS),
            -point(phase - 0.5, 1.5, 0.6, ITERATIONS)
        );
    }
    assert_eq!(
        clamped(0.2, 1.0, 5.0, 0.999, ITERATIONS),
        point(0.2, 1.0, 0.999, ITERATIONS)
    );
    assert_eq!(
        clamped(0.2, 1.0, -5.0, 0.999, ITERATIONS),
        point(0.2, 1.0, -0.999, ITERATIONS)
    );
    // The recurrence starts from no earlier outputs, then feeds back the mean of
    // the two before.
    let mut discrete = Discrete::new();
    let first = discrete.output(0.1, 1.0, 0.25);
    assert_eq!(first, sin2pi(0.1));
    let second = discrete.output(0.2, 1.0, 0.25);
    assert_eq!(second, sin2pi(0.25f64.mul_add(first / 2.0, 0.2)));
    // The lag: none up to the onset, τ from |β| = 1 on, linear between.
    assert_eq!(lag(0.5, 0.9, TAU_S), 0.0);
    assert_eq!(lag(0.9, 0.9, TAU_S), 0.0);
    assert_eq!(lag(1.0, 0.9, TAU_S), TAU_S);
    assert_eq!(lag(1.05, 0.9, TAU_S), TAU_S);
    assert_eq!(lag(-3.0, 0.9, TAU_S), TAU_S);
    assert!((lag(0.95, 0.9, TAU_S) - TAU_S / 2.0).abs() <= 8.0 * U * TAU_S);
    assert_eq!(lag(-0.95, 0.9, TAU_S), lag(0.95, 0.9, TAU_S));
    // The substeps: one at a loop gain of 1 and below, the fewest shorter than
    // 2τ/(β − 1) above, and the cap.
    let rate = 48_000.0;
    assert_eq!(substeps(0.5, TAU_S, rate, 64), 1);
    assert_eq!(substeps(1.0, TAU_S, rate, 64), 1);
    assert_eq!(substeps(3.0, TAU_S, rate, 64), 1);
    assert_eq!(substeps(20.0, TAU_S, rate, 64), 7);
    assert_eq!(substeps(-20.0, TAU_S, rate, 64), 7);
    assert_eq!(substeps(1e6, TAU_S, rate, 64), 64);
    assert_eq!(substeps(1e6, 0.0, rate, 64), 1);
    // With no lag the relaxation is the point solution, and Y follows it.
    let mut relaxation = Relaxation::new();
    assert_eq!(relaxation.lagged(), 0.0);
    let y = relaxation.output(|_| (0.3, 1.2), 1, 0.05, 0.0, rate, ITERATIONS);
    assert_eq!(y, point(0.3, 1.2, beta(0.05, 1.2), ITERATIONS));
    assert_eq!(relaxation.lagged(), y);
    // Without feedback the lag follows the output: from Y = 0 one substep of the
    // trapezoidal rule takes Y to a(y₀ + y₁)/(1 + a).
    let mut plain = Relaxation::new();
    let output = plain.output(
        |j| if j == 0 { (0.1, 1.0) } else { (0.2, 1.0) },
        1,
        0.0,
        TAU_S,
        rate,
        ITERATIONS,
    );
    assert_eq!(output, sin2pi(0.2));
    let a = 1.0 / (2.0 * TAU_S * rate);
    let expected = a * (sin2pi(0.1) + sin2pi(0.2)) / (1.0 + a);
    assert!((plain.lagged() - expected).abs() <= 8.0 * U);
    // Tiny states are flushed to 0.
    let mut tiny = Relaxation::new();
    tiny.output(|_| (0.25, 1e-200), 1, 0.0, TAU_S, rate, ITERATIONS);
    assert!(tiny.lagged() > 0.0);
    tiny.output(|_| (0.25, 0.0), 1, 0.0, TAU_S, rate, ITERATIONS);
    tiny.flush();
    assert_eq!(tiny.lagged(), 0.0);
    let mut faint = Discrete::new();
    faint.output(0.25, 1e-160, 0.0);
    faint.output(0.25, 1e-160, 0.0);
    faint.flush();
    // κ large enough that the outputs before would turn the phase by a quarter
    // had they stayed.
    let after = faint.output(0.25, 1.0, 2.5e159);
    assert_eq!(after, sin2pi(0.25));
}
