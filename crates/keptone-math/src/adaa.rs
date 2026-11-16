// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Antiderivative antialiasing of the sine of a phase (docs/MATH.md §10): the
//! average of sin 2πx over one sample, x moving linearly between samples, in
//! the closed forms of the first order \[PARKER-ADAA\] and the second
//! \[BILBAO-ADAA\].
//!
//! The inputs are the phase at the previous sample, x = xₙ₋₁ in turns, and the
//! increments of the phase over the sample and over the one before it,
//! Δₙ = xₙ − xₙ₋₁ and Δₙ₋₁ = xₙ₋₁ − xₙ₋₂, which the engine computes directly
//! (MATH §10.1). Both functions first reduce x exactly to r = x − round(x),
//! |r| ≤ 1/2, so that their errors do not grow with |x|. The bounds are
//! absolute, an output being an average of a sine, at most 1 in size, and u is
//! 2⁻⁵³; `scripts/rounding_bounds.py` computes their constants from the bounds
//! of sin2pi, cos2pi, sinc and sinc′.

use core::f64::consts::PI;

use crate::sinc::{sinc, sinc_derivatives};
use crate::trig::{cos2pi, sin2pi};

/// 2⁻¹⁰ turns, the default of `adaa.tau2` (MATH §17): below it in
/// |xₙ − xₙ₋₂|, [`second_order`] takes its expansion.
pub const DEFAULT_TAU2: f64 = 0.000_976_562_5;

/// ADAA-1 (MATH §10.2): ∫₀¹ sin 2π(x + τΔ) dτ = sinc(Δ) sin 2π(x + Δ/2), with
/// x = xₙ₋₁ and Δ = Δₙ, within 9 · 2⁻⁵³ absolute.
///
/// The midpoint r + Δ/2 rounds once, by at most u/2 where |Δ| < 1, and the
/// sine moves by at most 2π that far; sinc's relative error is below 2.46u,
/// sin2pi's below 1.46u, and the product rounds once: error ≤ πu + 4.92u =
/// 8.06u where |Δ| < 1. From |Δ| = 1 on, |sinc Δ| ≤ 1/(π|Δ|) shrinks both
/// parts, to at most 2.4u. Δ = 0 gives sin2pi(x).
#[must_use]
pub fn first_order(x: f64, delta: f64) -> f64 {
    let r = x - x.round_ties_even();
    sinc(delta) * sin2pi(0.5_f64.mul_add(delta, r))
}

/// ADAA-2 (MATH §10.3): twice the second divided difference of
/// F₂(x) = −sin(2πx)/(4π²) at xₙ = x + Δ, xₙ₋₁ = x and xₙ₋₂ = x − Δ′, with
/// Δ = Δₙ, Δ′ = Δₙ₋₁ and `tau2` the setting `adaa.tau2` (MATH §17); within
/// max(4.81u/τ₂ + 4.35u, 0.271τ₂⁴ + 14u) absolute, 5.5 · 10⁻¹³ for the
/// default, [`DEFAULT_TAU2`].
///
/// Where h = Δ + Δ′ is at least τ₂ in size, the quotient of the difference of
/// the two first differences, each −cos π(a + b) sinc(a − b)/(2π) in closed
/// form, by h. The difference cancels as h shrinks: the products in it are
/// within 4.92u and 3.92u of their values, and 2π · u/2 each for the roundings
/// of their arguments, 15.11u in all, which the division by πh turns into
/// 4.81u/|h|; with the roundings of h, π, their product, the division and the
/// difference, error ≤ 4.81u/|h| + 4.35u, largest at |h| = τ₂, 5.47 · 10⁻¹³
/// for the default.
///
/// Below τ₂, the expansion about a = (xₙ + xₙ₋₂)/2 and b = xₙ₋₁ of MATH §10.3,
/// 2∂ₐF₂[a, b] + (h²/12) ∂ₐ³F₂[a, b]. Its remainder is at most
/// h⁴ (2π)⁴/5760 ≤ 0.271h⁴, as ∂ₐ⁵F₂[a, b] is the mean of t⁵ F₂⁽⁶⁾ over t in
/// [0, 1] and |F₂⁽⁶⁾| ≤ (2π)⁴, and its evaluation rounds by at most 14u, as the
/// comment on the private function that evaluates it shows: error ≤
/// 0.271τ₂⁴ + 14u, 2.48 · 10⁻¹³ for the default. h = 0 takes the expansion
/// whatever τ₂ is, and three equal points, Δ = Δ′ = 0, give sin2pi(x).
#[must_use]
pub fn second_order(x: f64, delta: f64, previous: f64, tau2: f64) -> f64 {
    let r = x - x.round_ties_even();
    let h = delta + previous;
    if h.abs() < tau2 || h == 0.0 {
        return expansion(r, delta, previous, h);
    }
    let later = cos2pi(0.5_f64.mul_add(delta, r)) * sinc(delta);
    let numerator = cos2pi((-0.5_f64).mul_add(previous, r)).mul_add(sinc(previous), -later);
    numerator / (PI * h)
}

/// The expansion of ADAA-2 for small h, in r, Δ and Δ′ (MATH §10.3). With
/// d = a − b = (Δ − Δ′)/2 and a + b = 2x + d, so that sin π(a + b) is
/// sin 2πm for m = r + d/2,
///
/// - 2∂ₐF₂ = sin 2πm · sinc d − cos 2πm · sinc′ d/π, and
/// - ∂ₐ³F₂ = sin 2πm · (3/2 · sinc″ d − π²/2 · sinc d)
///   + cos 2πm · (3π/2 · sinc′ d − sinc‴ d/(2π)),
///
/// as `scripts/adaa_derivatives.py` derives them. The first term rounds by at
/// most 13.62u, over all d, of which at most 2.92u comes from Δ − Δ′, which d
/// and m share and which rounds by u|d|; 3.60u from the rounding of m; 1.67u
/// from the sine and cosine; 2.46u from sinc; 2.81u from sinc′; and 2.0u from
/// the operations. The second term is below π²h²/12 < 0.823h² in size, and its
/// errors, below 10⁻¹³ h², are negligible.
fn expansion(r: f64, delta: f64, previous: f64, h: f64) -> f64 {
    let difference = delta - previous;
    let d = 0.5 * difference;
    let m = 0.25_f64.mul_add(difference, r);
    let (sine, cosine) = (sin2pi(m), cos2pi(m));
    let (s0, s1, s2, s3) = sinc_derivatives(d);
    let first = sine.mul_add(s0, -(cosine * s1) / PI);
    let sine_part = 1.5_f64.mul_add(s2, -0.5 * PI * PI * s0);
    let cosine_part = (1.5 * PI).mul_add(s1, -s3 / (2.0 * PI));
    let third = sine.mul_add(sine_part, cosine * cosine_part);
    (h * h / 12.0).mul_add(third, first)
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_TAU2, first_order, second_order};
    use crate::sin2pi;

    #[test]
    fn a_still_phase_gives_its_sine() {
        for x in [0.0, 0.1, 0.25, -0.3, 0.5, 0.75, 12.375, -1.0e6 - 0.125] {
            let sine = sin2pi(x);
            assert_eq!(first_order(x, 0.0), sine, "first_order({x}, 0)");
            assert_eq!(second_order(x, 0.0, 0.0, DEFAULT_TAU2), sine, "second_order({x}, 0, 0)");
            assert_eq!(
                second_order(x, 0.0, 0.0, 0.0),
                sine,
                "second_order({x}, 0, 0) with tau2 = 0"
            );
        }
    }

    #[test]
    fn non_finite_inputs_give_nan() {
        assert!(first_order(f64::NAN, 0.1).is_nan() && first_order(f64::INFINITY, 0.1).is_nan());
        assert!(second_order(0.1, f64::NAN, 0.1, DEFAULT_TAU2).is_nan());
        assert!(second_order(f64::INFINITY, 0.1, 0.1, DEFAULT_TAU2).is_nan());
    }
}
