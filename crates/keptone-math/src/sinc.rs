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

/// 1/4, below which [`sinc_derivatives`] takes the Taylor series of sinc″ and
/// sinc‴.
const DERIVATIVE_SERIES_BELOW: f64 = 0.25;

/// The Taylor coefficients of f″(z) = Σ cⱼ z²ʲ for f(z) = sin z / z, j = 0 to
/// 8, as `scripts/adaa_derivatives.py` derives them. Every denominator is an
/// integer below 2⁵³, so each coefficient is the quotient correctly rounded.
const SECOND_SERIES: [f64; 9] = [
    -1.0 / 3.0,
    1.0 / 10.0,
    -1.0 / 168.0,
    1.0 / 6_480.0,
    -1.0 / 443_520.0,
    1.0 / 47_174_400.0,
    -1.0 / 7_185_024_000.0,
    1.0 / 1_482_030_950_400.0,
    -1.0 / 397_533_007_872_000.0,
];

/// The Taylor coefficients of f‴(z) = z Σ eⱼ z²ʲ, j = 0 to 8, likewise.
const THIRD_SERIES: [f64; 9] = [
    1.0 / 5.0,
    -1.0 / 42.0,
    1.0 / 1_080.0,
    -1.0 / 55_440.0,
    1.0 / 4_717_440.0,
    -1.0 / 598_752_000.0,
    1.0 / 105_859_353_600.0,
    -1.0 / 24_845_812_992_000.0,
    1.0 / 7_469_435_990_016_000.0,
];

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

/// sinc u and its first three derivatives, for the expansion of ADAA-2 (MATH
/// §10.3), whose term in h² takes sinc″ and sinc‴.
///
/// sinc and sinc′ are those of [`sinc`] and [`sinc_prime`]. Below |u| = 1/4,
/// sinc″ u = π² f″(πu) and sinc‴ u = π³ f‴(πu) come from the Taylor series of
/// f(z) = sin z / z, whose first neglected terms are below 10⁻¹⁹ there; from
/// 1/4 on, from the equation u sinc″ + 2 sinc′ + π² u sinc = 0 and its
/// derivative, sinc‴ = −(3 sinc″ + π² sinc)/u − π² sinc′, which cancel less
/// the farther u is from 0. sinc″ is then within 2 · 10⁻¹⁴ absolute and sinc‴
/// within 3 · 10⁻¹³: the expansion multiplies them by h²/12, below 10⁻⁷.
pub(crate) fn sinc_derivatives(u: f64) -> (f64, f64, f64, f64) {
    let pi_squared = core::f64::consts::PI * core::f64::consts::PI;
    let (zeroth, first) = (sinc(u), sinc_prime(u));
    if u.abs() < DERIVATIVE_SERIES_BELOW {
        let z = core::f64::consts::PI * u;
        let w = z * z;
        let second = pi_squared * horner(&SECOND_SERIES, w);
        let third = pi_squared * core::f64::consts::PI * z * horner(&THIRD_SERIES, w);
        return (zeroth, first, second, third);
    }
    let second = (-pi_squared).mul_add(zeroth, -2.0 * first / u);
    let both = 3.0_f64.mul_add(second, pi_squared * zeroth);
    (zeroth, first, second, -both / u - pi_squared * first)
}

/// Σ cⱼ wʲ by Horner's scheme with fused steps, from the highest power.
fn horner(coefficients: &[f64], w: f64) -> f64 {
    coefficients.iter().rev().fold(0.0, |sum, &c| sum.mul_add(w, c))
}

#[cfg(test)]
mod tests {
    use super::{sinc, sinc_derivatives, sinc_prime};

    /// sinc″ and sinc‴ against the references that `scripts/adaa_references.py`
    /// computes with mpmath (`tests/data/sinc_derivatives.txt`), within the
    /// bounds of [`sinc_derivatives`], on both sides of the threshold of its
    /// series among them.
    #[test]
    fn derivatives_within_their_bounds() {
        let (mut second, mut third) = ((0.0_f64, 0.0_f64), (0.0_f64, 0.0_f64));
        for line in include_str!("../tests/data/sinc_derivatives.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
        {
            let row: Vec<f64> = line
                .split(' ')
                .map(|word| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits")))
                .collect();
            let (_, _, s2, s3) = sinc_derivatives(row[0]);
            let errors = (((s2 - row[1]) - row[2]).abs(), ((s3 - row[3]) - row[4]).abs());
            if errors.0 > second.0 {
                second = (errors.0, row[0]);
            }
            if errors.1 > third.0 {
                third = (errors.1, row[0]);
            }
        }
        println!(
            "sinc'': worst {:e} at {:e}; sinc''': worst {:e} at {:e}",
            second.0, second.1, third.0, third.1
        );
        assert!(second.0 <= 2e-14, "sinc'': {second:?}");
        assert!(third.0 <= 3e-13, "sinc''': {third:?}");
    }

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
