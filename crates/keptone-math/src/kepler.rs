// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Kepler's equation Θ − β sin Θ = Φ for β < 1 (docs/MATH.md §9.4), whose
//! solution is the delay-free feedback operator, y = g sin Θ (§9.2).
//!
//! The solver works in turns, as the phases of the engine are: Θ = 2πτ and
//! Φ = 2πφ, so that τ − ε sin 2πτ = φ with ε = β/(2π), where sin2pi and cos2pi
//! reduce their arguments exactly. Newton's iteration is the same in either
//! unit, and the stopping rule of §9.4, |ΔΘ| ≤ 2u max(1, |Θ|), is
//! |Δτ| ≤ 2u max(1/(2π), |τ|) here, u = 2⁻⁵³.
//!
//! φ is reduced exactly to r = φ − round(φ), |r| ≤ 1/2, and the solution keeps
//! r's sign, the equation being odd: the solver works on |r| and gives back the
//! sign, so that the results for r and −r are each other's negatives bit for
//! bit. For r in [0, 1/2] the solution lies in [r, min(r + ε, 1/2)], where
//! 0 ≤ sin 2πτ ≤ 1, and there τ − ε sin 2πτ is increasing and convex: a Newton
//! step from a point where the residual is positive goes down to the root and
//! no further. Every solve starts from \[MARKLEY\]'s starter. A Newton step
//! that leaves the bracket \[ODELL-GOODING\] goes to its upper end instead, from
//! where Newton converges so, while that end is the bound r + ε (or 1/2) that
//! nothing has replaced; otherwise it bisects.
//!
//! Near the cusp, β near 1 and Θ small, τ − ε sin 2πτ − r cancels: its terms are
//! of the size of τ, its value of the size of r, which can be far smaller. As
//! \[MARKLEY\] does, the residual is then written (1 − β)τ + ε(2πτ − sin 2πτ) − r,
//! whose terms are no larger than r, with 2πτ − sin 2πτ from its own polynomial
//! below τ = 1/4, and summed in double-double; the derivative is
//! (1 − β) + 2β sin² πτ. From 1/4 on, (τ − r) − ε sin 2πτ is exact but for the
//! sine's error, and 1 − β cos 2πτ ≥ 1. Either way the residual errs by less
//! than u|r| near the root, which both lets the stopping rule be met and keeps
//! |Θ − β sin Θ − Φ| within 4u max(1, |Φ|) (MATH §16).

use core::f64::consts::{PI, TAU};

use crate::coefficients::{
    KEPLER_0_HI, KEPLER_0_LO, KEPLER_2, KEPLER_4, KEPLER_6, KEPLER_8, KEPLER_10, KEPLER_12, KEPLER_14, KEPLER_16,
    KEPLER_18, KEPLER_20,
};
use crate::eft::{two_product, two_sum};
use crate::trig::{cos2pi, sin2pi};

/// The default of `feedback.max_iterations` (MATH §17).
pub const DEFAULT_MAX_ITERATIONS: u32 = 16;

/// u = 2⁻⁵³, the unit roundoff of binary64.
const U: f64 = f64::EPSILON / 2.0;

/// 1/(2π): where |Θ| = 1 in turns, below which the stopping rule is absolute.
const ONE_RADIAN: f64 = 1.0 / TAU;

/// 1/4, below which the residual takes the polynomial of 2πτ − sin 2πτ.
const CUSP_BELOW: f64 = 0.25;

/// A solution of Kepler's equation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Solution {
    /// τ = Θ/(2π), in turns, for the reduced phase r: in [−1/2, 1/2], of r's
    /// sign. The operator's output is g · sin2pi(τ).
    pub turns: f64,
    /// The iterations taken, each one evaluation of the residual: 0 where the
    /// solution is exact without one.
    pub iterations: u32,
}

/// Solves Θ − β sin Θ = Φ for Θ = 2πτ and Φ = 2πφ, φ = `phase` in turns, with
/// 0 ≤ β < 1 (MATH §9.4), from \[MARKLEY\]'s starter.
///
/// The iteration stops when a step is at most 2u max(1/(2π), |τ|), or after
/// `max_iterations` (`feedback.max_iterations`, [`DEFAULT_MAX_ITERATIONS`]).
/// β = 0, r = 0 and r = ±1/2 are solved exactly, without an iteration. For
/// β ≥ 1 the equation has more than one solution (MATH §9.3); the result is
/// one of them, which §9.6 does not take. A non-finite input, or β < 0, gives
/// NaN.
#[must_use]
pub fn solve(phase: f64, beta: f64, max_iterations: u32) -> Solution {
    let r = phase - phase.round_ties_even();
    if !r.is_finite() || !beta.is_finite() || beta < 0.0 {
        return Solution {
            turns: f64::NAN,
            iterations: 0,
        };
    }
    if r < 0.0 {
        let mirrored = solve_reduced(-r, beta, max_iterations);
        return Solution {
            turns: -mirrored.turns,
            iterations: mirrored.iterations,
        };
    }
    solve_reduced(r, beta, max_iterations)
}

/// [`solve`] for r in [0, 1/2].
fn solve_reduced(r: f64, beta: f64, max_iterations: u32) -> Solution {
    #[expect(
        clippy::float_cmp,
        reason = "Φ = π is an exact root, Θ = π, which an iteration would only perturb"
    )]
    let half_turn = r == 0.5;
    if beta == 0.0 || r == 0.0 || half_turn {
        return Solution {
            turns: r,
            iterations: 0,
        };
    }
    let epsilon = beta / TAU;
    let complement = 1.0 - beta;
    let (floor, ceiling) = (r, (r + epsilon).min(0.5));
    // max and min, not clamp, which would panic on a NaN.
    let mut tau = (markley(TAU * r, beta) / TAU).max(floor).min(ceiling);
    let (mut low, mut high) = (floor, ceiling);
    // Whether the bracket's top is still the bound r + ε (or 1/2), which no
    // evaluation has replaced: the residual there is not negative.
    let mut top_is_bound = true;
    let mut iterations = 0;
    while iterations < max_iterations {
        iterations += 1;
        let (f, slope) = residual(tau, r, beta, epsilon, complement);
        if f > 0.0 {
            high = tau;
            top_is_bound = false;
        } else if f < 0.0 {
            low = tau;
        } else {
            break;
        }
        let step = -f / slope;
        if step.abs() <= 2.0 * U * tau.abs().max(ONE_RADIAN) {
            tau = (tau + step).max(floor).min(ceiling);
            break;
        }
        let next = tau + step;
        tau = if low < next && next < high {
            next
        } else if next >= high && top_is_bound {
            high
        } else {
            f64::midpoint(low, high)
        };
    }
    Solution { turns: tau, iterations }
}

/// τ − ε sin 2πτ − r and its derivative 1 − β cos 2πτ, for τ in [0, 1/2], each
/// in the form that does not cancel there (the module's documentation).
fn residual(tau: f64, reduced: f64, beta: f64, epsilon: f64, complement: f64) -> (f64, f64) {
    if tau >= CUSP_BELOW {
        let (gap, gap_lo) = two_sum(tau, -reduced);
        let value = (-epsilon).mul_add(sin2pi(tau), gap) + gap_lo;
        return (value, (-beta).mul_add(cos2pi(tau), 1.0));
    }
    // 2πτ − sin 2πτ = τ³Q(τ²), Q's constant term a double-double; τ³, τ³Q and
    // ετ³Q as double-doubles, then (1 − β)τ − r + ετ³Q summed exactly but for
    // the low parts, which are a few hundredths of the result at most.
    let (square, square_lo) = two_product(tau, tau);
    let mut tail = KEPLER_20;
    for c in [
        KEPLER_18, KEPLER_16, KEPLER_14, KEPLER_12, KEPLER_10, KEPLER_8, KEPLER_6, KEPLER_4, KEPLER_2,
    ] {
        tail = tail.mul_add(square, c);
    }
    let series_lo = tail.mul_add(square, KEPLER_0_LO);
    let (cube, cube_lo) = two_product(tau, square);
    let cube_lo = tau.mul_add(square_lo, cube_lo);
    let (bend, bend_lo) = two_product(KEPLER_0_HI, cube);
    let bend_lo = bend_lo + KEPLER_0_HI.mul_add(cube_lo, series_lo * cube);
    let (lift, lift_lo) = two_product(epsilon, bend);
    let lift_lo = epsilon.mul_add(bend_lo, lift_lo);
    let (linear, linear_lo) = two_product(complement, tau);
    let (gap, gap_lo) = two_sum(linear, -reduced);
    let (sum, sum_lo) = two_sum(gap, lift);
    let value = sum + (sum_lo + (gap_lo + (linear_lo + lift_lo)));
    let half = sin2pi(0.5 * tau);
    (value, (2.0 * beta * half).mul_add(half, complement))
}

/// \[MARKLEY\]'s starter for E − e sin E = M, 0 < M ≤ π and 0 < e < 1, in
/// radians: the root of the cubic that a Padé approximant of sin E gives, with
/// α of his equation (20) and d, q, r, w and E₁ of (5), (9), (10), (14) and
/// (15), within a relative 2.8 · 10⁻⁴ of the solution (his Figure 4).
fn markley(mean: f64, eccentricity: f64) -> f64 {
    let e = eccentricity;
    let pi_squared = PI * PI;
    let alpha = (3.0 * pi_squared + 1.6 * PI * (PI - mean) / (1.0 + e)) / (pi_squared - 6.0);
    let d = 3.0 * (1.0 - e) + alpha * e;
    let q = 2.0 * alpha * d * (1.0 - e) - mean * mean;
    let cubic = 3.0 * alpha * d * (d - 1.0 + e) * mean + mean * mean * mean;
    let root = cube_root(cubic.abs() + (q * q * q + cubic * cubic).sqrt());
    let w = root * root;
    (2.0 * cubic * w / (w * w + w * q + q * q) + mean) / d
}

/// ∛x for x > 0 from basic operations alone, so that the starter is the same on
/// every system, which the system's cbrt need not be: a first guess from a third
/// of the bits, about 3 % off, and four Newton steps, each squaring the relative
/// error, to well below what a starter needs.
fn cube_root(x: f64) -> f64 {
    let mut y = f64::from_bits(x.to_bits() / 3 + 0x2a9f_7893_782d_a1ce);
    for _ in 0..4 {
        y = (2.0 * y + x / (y * y)) / 3.0;
    }
    y
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_MAX_ITERATIONS, cube_root, solve};

    #[test]
    fn exact_cases() {
        for phase in [0.0, 0.5, -0.5, 3.0, -7.5] {
            let s = solve(phase, 0.9, DEFAULT_MAX_ITERATIONS);
            assert_eq!(s.iterations, 0, "{phase}");
            assert_eq!(s.turns, phase - phase.round_ties_even(), "{phase}");
        }
        let s = solve(0.3, 0.0, DEFAULT_MAX_ITERATIONS);
        assert_eq!((s.turns, s.iterations), (0.3, 0));
        assert!(solve(f64::NAN, 0.5, 8).turns.is_nan());
        assert!(solve(0.1, -0.5, 8).turns.is_nan());
        assert!(solve(0.1, f64::NAN, 8).turns.is_nan());
    }

    #[test]
    fn odd_and_periodic() {
        for (phase, beta) in [(0.1, 0.5), (0.37, 0.95), (0.01, 0.999), (1.0e-12, 0.999)] {
            let up = solve(phase, beta, DEFAULT_MAX_ITERATIONS);
            let down = solve(-phase, beta, DEFAULT_MAX_ITERATIONS);
            assert_eq!(up.turns.to_bits(), (-down.turns).to_bits(), "{phase}, {beta}");
            let far = solve(phase + 1.0e6, beta, DEFAULT_MAX_ITERATIONS);
            let near = solve((phase + 1.0e6) - 1.0e6, beta, DEFAULT_MAX_ITERATIONS);
            assert_eq!(far, near, "{phase}, {beta}");
        }
    }

    #[test]
    fn cube_roots() {
        for x in [1.0e-30, 1.0e-6, 0.5, 1.0, 2.0, 27.0, 1.0e20] {
            let y: f64 = cube_root(x);
            assert!((y * y * y - x).abs() <= 4.0 * f64::EPSILON * x, "{x}: {y}");
        }
    }
}
