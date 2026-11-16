// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The sine and cosine of a phase in turns (docs/MATH.md §12.1).
//!
//! A phase in turns reduces exactly: r = x − round(x) is exact in binary64, and
//! so are the quarter turn q = round(4r) and the remainder t = r − q/4, with
//! |t| ≤ 1/8 (Sterbenz's lemma for both subtractions). The octant symmetries
//! then turn sin 2πr and cos 2πr into ± sin 2πt or ± cos 2πt, which two minimax
//! polynomials give on |t| ≤ 1/8, their approximation errors below 2⁻⁶³
//! ([`crate::coefficients`]).

use crate::coefficients::{
    COS_2PI_2_HI, COS_2PI_2_LO, COS_2PI_4, COS_2PI_6, COS_2PI_8, COS_2PI_10, COS_2PI_12, COS_2PI_14, COS_2PI_16,
    SIN_2PI_1_HI, SIN_2PI_1_LO, SIN_2PI_3, SIN_2PI_5, SIN_2PI_7, SIN_2PI_9, SIN_2PI_11, SIN_2PI_13, SIN_2PI_15,
};
use crate::eft::{fast_two_sum, two_product};

/// sin 2πx, x in turns: within 0.96 ulp of sin 2πx, with a relative error below
/// 1.46u (u = 2⁻⁵³; the two kernels below say why), and so within 1 ulp of the
/// correctly rounded result (MATH §12.1, tested against CORE-MATH's
/// `cr_sinpi(2x)`).
///
/// A zero result has the sign of x, as C23's `sinpi` gives it; ±∞ and NaN give
/// NaN. Every finite x reduces exactly, so the error does not grow with |x|: an
/// integer or a half-integer gives an exact zero, and |x| ≥ 2⁵² is one.
#[must_use]
pub fn sin2pi(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    let (quarter, t) = reduce(x);
    let value = match quarter {
        Quarter::Zero => sin_kernel(t),
        Quarter::One => cos_kernel(t),
        Quarter::Half => -sin_kernel(t),
        Quarter::MinusOne => -cos_kernel(t),
    };
    if is_zero(value) { 0.0_f64.copysign(x) } else { value }
}

/// cos 2πx, x in turns: within 0.96 ulp of cos 2πx, with a relative error below
/// 1.46u, and so within 1 ulp of the correctly rounded result (MATH §12.1,
/// tested against CORE-MATH's `cr_cospi(2x)`).
///
/// A zero result is +0, as C23's `cospi` gives it; ±∞ and NaN give NaN.
#[must_use]
pub fn cos2pi(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    let (quarter, t) = reduce(x);
    let value = match quarter {
        Quarter::Zero => cos_kernel(t),
        Quarter::One => -sin_kernel(t),
        Quarter::Half => -cos_kernel(t),
        Quarter::MinusOne => sin_kernel(t),
    };
    if is_zero(value) { 0.0 } else { value }
}

/// The quarter turn nearest to r = x − round(x), which takes q/4 with q in
/// {−2, …, 2}; ±1/2 are one half turn.
enum Quarter {
    Zero,
    One,
    Half,
    MinusOne,
}

/// The exact reduction of a finite x: the quarter turn and t = r − q/4, |t| ≤ 1/8,
/// so that 2πx = 2πt + qπ/2 modulo 2π.
fn reduce(x: f64) -> (Quarter, f64) {
    let r = x - x.round_ties_even();
    let q = (4.0 * r).round_ties_even();
    let t = r - 0.25 * q;
    let quarter = if q > 1.5 || q < -1.5 {
        Quarter::Half
    } else if q > 0.5 {
        Quarter::One
    } else if q < -0.5 {
        Quarter::MinusOne
    } else {
        Quarter::Zero
    };
    (quarter, t)
}

/// Whether v is +0 or −0.
fn is_zero(v: f64) -> bool {
    v.to_bits() << 1 == 0
}

/// sin 2πt for |t| ≤ 1/8: c₁t + t³·P(t²), the term c₁t as a double-double.
///
/// The result is hi + δ, hi = fl(c₁,hi·t), with δ the exact low part of that
/// product, c₁,lo·t and the odd tail t³P, |t³P| ≤ 0.111 |sin 2πt|. Only the
/// tail rounds at the order of u: t² by u, Horner's fused steps give P within
/// 1.13u (the rounding of t² included), and the two fused steps that multiply
/// by t and add round by u each, 4.13u of the tail, so 0.46u of the result; the
/// final addition rounds once. Error ≤ 0.5 ulp + 0.46u |sin 2πt| ≤ 0.96 ulp,
/// and the relative error is below 1.46u; the polynomial's own bound, 7.05 ·
/// 10⁻²⁰ relative, is below 0.001u.
#[inline]
fn sin_kernel(t: f64) -> f64 {
    let t2 = t * t;
    let mut tail = SIN_2PI_15;
    for c in [SIN_2PI_13, SIN_2PI_11, SIN_2PI_9, SIN_2PI_7, SIN_2PI_5, SIN_2PI_3] {
        tail = tail.mul_add(t2, c);
    }
    let (hi, lo) = two_product(SIN_2PI_1_HI, t);
    hi + t.mul_add(t2.mul_add(tail, SIN_2PI_1_LO), lo)
}

/// cos 2πt for |t| ≤ 1/8: 1 + c₂t² + t⁴·Q(t²), with t², c₂t² and 1 + c₂t² as
/// double-doubles.
///
/// The double-doubles leave errors of the order of u² only. What rounds at the
/// order of u is t⁴Q, |t⁴Q| ≤ 0.0156, computed within 5.09u (t² rounded and
/// squared, 2u, the two products, 2u, and Horner's fused steps, 1.09u with the
/// rounding of t² in their argument), and two additions below 2⁻⁶, which round
/// by u/128 each: 0.095u before the final addition, which rounds once. The
/// result is at least cos(π/4) > 0.70, where an ulp is u: error ≤ 0.6 ulp,
/// relative error below 0.85u; the polynomial's own bound is 5.09 · 10⁻²⁰
/// relative to cos 2πt − 1.
#[inline]
fn cos_kernel(t: f64) -> f64 {
    let (t2, t2_lo) = two_product(t, t);
    let mut tail = COS_2PI_16;
    for c in [COS_2PI_14, COS_2PI_12, COS_2PI_10, COS_2PI_8, COS_2PI_6, COS_2PI_4] {
        tail = tail.mul_add(t2, c);
    }
    let fourth = t2 * t2 * tail;
    let (a, a_lo) = two_product(COS_2PI_2_HI, t2);
    let a_lo = a_lo + COS_2PI_2_HI.mul_add(t2_lo, COS_2PI_2_LO * t2);
    let (s, s_lo) = fast_two_sum(1.0, a);
    s + (s_lo + (a_lo + fourth))
}

#[cfg(test)]
mod tests {
    use super::{cos2pi, sin2pi};

    #[test]
    fn exact_points() {
        for (x, s, c) in [
            (0.0, 0.0, 1.0),
            (0.25, 1.0, 0.0),
            (0.5, 0.0, -1.0),
            (0.75, -1.0, 0.0),
            (1.0, 0.0, 1.0),
        ] {
            assert_eq!(sin2pi(x).to_bits(), f64::to_bits(s), "sin2pi({x})");
            assert_eq!(cos2pi(x).to_bits(), f64::to_bits(c), "cos2pi({x})");
        }
        assert_eq!(sin2pi(-0.0).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(sin2pi(-0.5).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(sin2pi(-1.0).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(cos2pi(-0.25).to_bits(), 0.0_f64.to_bits());
        assert_eq!(sin2pi(2.0_f64.powi(60)).to_bits(), 0.0_f64.to_bits());
        assert!(sin2pi(f64::INFINITY).is_nan() && cos2pi(f64::NAN).is_nan());
    }
}
