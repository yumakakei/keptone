// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The solver of Kepler's equation against docs/MATH.md §16: over a β × Φ grid,
//! β in [0, 0.999] and Φ in [−π, π), 10⁶ points in the release profile, the
//! residual |Θ − β sin Θ − Φ| is within 4u max(1, |Φ|), u = 2⁻⁵³, and no point
//! takes more than 8 iterations, from \[MARKLEY\]'s starter. The residual is
//! computed in double-double, with a sine of its own (`common::sin2pi_dd`),
//! so that its own error, about 10⁻³⁰, does not count. The same holds beyond
//! the grid, nearer the cusp, up to β = 1 − 2⁻⁵², and at its smallest phases.

mod common;

use common::{Dd, SplitMix64, TWO_PI, sin2pi_dd};
use keptone_math::kepler::{DEFAULT_MAX_ITERATIONS, Solution, solve};
use keptone_math::sin2pi;

/// u = 2⁻⁵³.
const U: f64 = f64::EPSILON / 2.0;

/// |Θ − β sin Θ − Φ| for Θ = 2πτ and Φ = 2πr, r the phase reduced, in
/// double-double, against 4u max(1, |Φ|): the ratio of the two.
fn residual_ratio(phase: f64, beta: f64, solution: Solution) -> f64 {
    let r = phase - phase.round_ties_even();
    let gap = Dd(solution.turns, 0.0).add(Dd(-r, 0.0));
    let value = TWO_PI.mul(gap).add(Dd(beta, 0.0).mul(sin2pi_dd(solution.turns)).neg());
    let bound = 4.0 * U * (core::f64::consts::TAU * r.abs()).max(1.0);
    value.value().abs() / bound
}

/// The largest residual ratio and iteration count found, and a histogram of
/// the counts.
#[derive(Debug, Default)]
struct Record {
    ratio: f64,
    at: (f64, f64),
    iterations: u32,
    counts: [u64; 17],
}

impl Record {
    fn take(&mut self, phase: f64, beta: f64, solution: Solution) {
        let ratio = residual_ratio(phase, beta, solution);
        if ratio > self.ratio {
            (self.ratio, self.at) = (ratio, (phase, beta));
        }
        self.iterations = self.iterations.max(solution.iterations);
        let index = usize::try_from(solution.iterations.min(16)).expect("at most 16");
        self.counts[index] += 1;
    }
}

#[test]
fn the_double_double_sine() {
    let mut rng = SplitMix64(0x6b65_706c_6572);
    for _ in 0..10_000 {
        let x = 2.0 * rng.unit() - 1.0;
        let s = sin2pi_dd(x);
        assert!(common::ulp_error(sin2pi(x), s.value()) <= 1.0, "sin2pi_dd({x})");
        let c = sin2pi_dd(x + 0.25);
        let one = s.mul(s).add(c.mul(c)).add(Dd(-1.0, 0.0));
        assert!(one.value().abs() <= 1e-30, "sin² + cos² − 1 at {x}: {one:?}");
    }
    let half_sqrt2 = {
        let s = 2.0_f64.sqrt();
        Dd(s, (-s).mul_add(s, 2.0) / (2.0 * s)).div(2.0)
    };
    let eighth = sin2pi_dd(0.125).add(half_sqrt2.neg());
    assert!(eighth.value().abs() <= 1e-31, "sin(π/4): {eighth:?}");
}

#[test]
fn grid_within_the_bound_and_8_iterations() {
    // 10⁶ points in the release profile, as MATH §16 asks; fewer in the dev one.
    let n: u32 = if cfg!(debug_assertions) { 200 } else { 1000 };
    let mut record = Record::default();
    for i in 0..n {
        let beta = 0.999 * f64::from(i) / f64::from(n - 1);
        for j in 0..n {
            let phase = -0.5 + f64::from(j) / f64::from(n);
            record.take(phase, beta, solve(phase, beta, DEFAULT_MAX_ITERATIONS));
        }
    }
    println!(
        "grid of {}: worst residual {:.3} of the bound at (phase, beta) = {:?}; iterations at most {}, counts {:?}",
        n * n,
        record.ratio,
        record.at,
        record.iterations,
        &record.counts[..=usize::try_from(record.iterations).expect("small")]
    );
    assert!(record.ratio <= 1.0, "{record:?}");
    assert!(record.iterations <= 8, "{record:?}");
}

#[test]
fn near_the_cusp_within_the_bound_and_8_iterations() {
    let mut record = Record::default();
    let mut rng = SplitMix64(0x6375_7370);
    let mut phases = vec![
        1e-300, 1e-100, 1e-30, 1e-20, 1e-15, 1e-10, 1e-8, 1e-6, 1e-4, 0.001, 0.01, 0.1,
    ];
    phases.extend([0.2499, 0.25, 0.2501, 0.3, 0.49]);
    phases.extend((0..2000).map(|_| 0.01 * rng.unit() * rng.unit()));
    for beta in [
        0.999,
        0.9999,
        1.0 - 2.0_f64.powi(-20),
        1.0 - 2.0_f64.powi(-40),
        1.0 - 2.0_f64.powi(-52),
    ] {
        for &phase in &phases {
            record.take(phase, beta, solve(phase, beta, DEFAULT_MAX_ITERATIONS));
            record.take(-phase, beta, solve(-phase, beta, DEFAULT_MAX_ITERATIONS));
        }
    }
    println!(
        "near the cusp: worst residual {:.3} of the bound at {:?}; iterations at most {}, counts {:?}",
        record.ratio,
        record.at,
        record.iterations,
        &record.counts[..=usize::try_from(record.iterations).expect("small")]
    );
    assert!(record.ratio <= 1.0, "{record:?}");
    assert!(record.iterations <= 8, "{record:?}");
}
