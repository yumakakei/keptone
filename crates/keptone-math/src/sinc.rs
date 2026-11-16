// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! sinc u = sin(πu)/(πu) and its derivative (docs/MATH.md §12.2), which the
//! closed forms of antiderivative antialiasing take (MATH §10).

use crate::coefficients::{
    PI_0_HI, PI_0_LO, SINC_PRIME_1_HI, SINC_PRIME_1_LO, SINC_PRIME_3, SINC_PRIME_5, SINC_PRIME_7, SINC_PRIME_9,
    SINC_PRIME_11, SINC_PRIME_13, SINC_PRIME_15, SINC_PRIME_17, SINC_PRIME_19, SINC_TAYLOR_2, SINC_TAYLOR_4,
    SINC_TAYLOR_6, SINC_TAYLOR_8, SINC_TAYLOR_10, SINC_TAYLOR_12,
};
use crate::eft::two_product;
use crate::trig::{cos2pi, sin2pi};

/// 2⁻⁶, below which sinc takes its Taylor polynomial.
const SINC_SERIES_BELOW: f64 = 0.015_625;

/// 2⁵²: from there on every binary64 is an integer, where sinc is 0.
const INTEGERS_FROM: f64 = 4_503_599_627_370_496.0;

/// sinc u = sin(πu)/(πu), sinc 0 = 1: within 1.96 ulp of sinc u, and so within
/// 2 ulp of the correctly rounded result (MATH §12.2, §16).
///
/// For |u| < 2⁻⁶ the Taylor polynomial to degree 12, which is within 2⁻⁶⁶ of
/// sinc there with its coefficients as they are; the result, 1 + u²P(u²) with
/// |u²P| ≤ 4.1 · 10⁻⁴, is within 0.5 ulp + 2u · 4.1 · 10⁻⁴ ≤ 0.51 ulp. Above,
/// sin 2π(u/2), whose argument is exact and whose relative error is below
/// 1.46u ([`crate::sin2pi`]), divided by πu as a double-double: the quotient is
/// corrected by the remainder of the division, which the fused operation gives
/// exactly, so that before the last addition rounds it is within a relative
/// 1.46u and a few u² of sinc u. Error ≤ 0.5 ulp + 1.46u |sinc u| ≤ 1.96 ulp;
/// and as that relative error is below 1.5u, the result is within 2 ulp of the
/// correctly rounded one also where the two lie on either side of a power of
/// two. Integers give +0; ±∞ and NaN give NaN.
#[must_use]
pub fn sinc(u: f64) -> f64 {
    if !u.is_finite() {
        return f64::NAN;
    }
    let magnitude = u.abs();
    if magnitude < SINC_SERIES_BELOW {
        let square = u * u;
        let mut tail = SINC_TAYLOR_12;
        for c in [
            SINC_TAYLOR_10,
            SINC_TAYLOR_8,
            SINC_TAYLOR_6,
            SINC_TAYLOR_4,
            SINC_TAYLOR_2,
        ] {
            tail = tail.mul_add(square, c);
        }
        return square.mul_add(tail, 1.0);
    }
    if magnitude >= INTEGERS_FROM {
        return 0.0;
    }
    let sine = sin2pi(0.5 * u);
    let (pi_u, pi_u_lo) = two_product(PI_0_HI, u);
    let pi_u_lo = PI_0_LO.mul_add(u, pi_u_lo);
    let quotient = sine / pi_u;
    let remainder = (-quotient).mul_add(pi_u, sine);
    quotient + (-quotient).mul_add(pi_u_lo, remainder) / pi_u
}

/// The derivative of sinc, sinc′ u = (cos πu − sinc u)/u, sinc′ 0 = 0 (MATH
/// §12.2): within 2 ulp of the correctly rounded result where |u| < 1/2, and
/// within an absolute 9 · 2⁻⁵³ elsewhere.
///
/// Below 1/2 an odd minimax polynomial, its approximation error below 2⁻⁶³
/// relative and its leading term a double-double. Only the tail u³R(u²) rounds
/// at the order of u: at most 0.292 of the result, it is computed within 4.39u
/// (u² by u, Horner's fused steps within 1.39u, the rounding of u² included,
/// and the two fused steps that multiply by u and add, u each), 1.29u of the
/// result. Error ≤ 0.5 ulp + 1.29u |sinc′ u| ≤ 1.79 ulp, and as that relative
/// error is below 1.5u, within 2 ulp of the correctly rounded result.
///
/// Above, the closed form, whose subtraction cancels as u nears a zero of sinc′
/// (1.4303, 2.4590, …), where no bound relative to the result can hold. cos πu
/// = cos 2π(u/2) is within 0.96 ulp ≤ 0.96 · 2⁻⁵³ of its value, sinc u within
/// 1.96 ulp ≤ 1.96 · 2⁻⁵³ (|sinc u| ≤ 2/π), and the subtraction rounds by at
/// most 2⁻⁵³; the division by |u| ≥ 1/2 at most doubles the sum, 7.84 · 2⁻⁵³,
/// and rounds by at most 2⁻⁵³ (|sinc′ u| ≤ 1.371): 8.84 · 2⁻⁵³ in all, below
/// 9 · 2⁻⁵³. ±∞ and NaN give NaN.
#[must_use]
pub fn sinc_prime(u: f64) -> f64 {
    if !u.is_finite() {
        return f64::NAN;
    }
    if u.abs() < 0.5 {
        let v = u * u;
        let mut tail = SINC_PRIME_19;
        for c in [
            SINC_PRIME_17,
            SINC_PRIME_15,
            SINC_PRIME_13,
            SINC_PRIME_11,
            SINC_PRIME_9,
            SINC_PRIME_7,
            SINC_PRIME_5,
            SINC_PRIME_3,
        ] {
            tail = tail.mul_add(v, c);
        }
        let (hi, lo) = two_product(SINC_PRIME_1_HI, u);
        return hi + u.mul_add(v.mul_add(tail, SINC_PRIME_1_LO), lo);
    }
    (cos2pi(0.5 * u) - sinc(u)) / u
}

#[cfg(test)]
mod tests {
    use super::{sinc, sinc_prime};

    #[test]
    fn exact_points() {
        assert_eq!(sinc(0.0), 1.0);
        assert_eq!(sinc_prime(0.0), 0.0);
        for n in 1..=8 {
            assert_eq!(sinc(f64::from(n)), 0.0, "sinc({n})");
            assert_eq!(sinc(f64::from(-n)), 0.0, "sinc({})", -n);
        }
        assert_eq!(sinc(2.0_f64.powi(60)), 0.0);
        assert!(sinc(f64::INFINITY).is_nan() && sinc_prime(f64::NAN).is_nan());
    }
}
