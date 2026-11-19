// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The logarithms of base 2 (docs/MATH.md §12.4).

use core::f64::consts::SQRT_2;

use crate::coefficients::{
    LOG2_0_HI, LOG2_0_LO, LOG2_1_HI, LOG2_1_LO, LOG2_2, LOG2_3, LOG2_4, LOG2_5, LOG2_6, LOG2_7, LOG2_8, LOG2_9,
    LOG2_10, LOG2_11, LOG2_12, LOG2_13, LOG2_14, LOG2_15, LOG2_16, LOG2_17, LOG2_18, LOG2_19, LOG2_20, LOG2_21,
    LOG2_22,
};
use crate::eft::{fast_two_sum, two_product, two_sum};

/// The mantissa's bits.
const MANTISSA: u64 = 0x000f_ffff_ffff_ffff;

/// The least t of the kernel, fl(√2)/2 − 1, which Sterbenz's lemma makes exact.
const LEAST: f64 = SQRT_2 / 2.0 - 1.0;

/// The bound above every t of the kernel, fl(√2) − 1, exact likewise.
const BEYOND: f64 = SQRT_2 - 1.0;

/// log₂ x: within 0.81 ulp of log₂ x, and so within 1 ulp of the correctly
/// rounded result (MATH §12.4, tested against CORE-MATH's `cr_log2`).
///
/// x splits exactly into 2^k m, m from fl(√2)/2 up to fl(√2), from the
/// exponent's bits, a subnormal x scaled by 2⁵⁴ first; t = m − 1 is exact
/// (Sterbenz), and log₂(1 + t) comes from the kernel below as a double-double,
/// within 0.31u of itself. Where k is 0 the result is that double-double
/// rounded once; otherwise it is k + log₂(1 + t), summed exactly but for a term
/// of the order of u² and rounded once. |log₂(1 + t)| is at most log₂ fl(√2), a
/// little above 1/2, and |k + log₂(1 + t)| at least a little below 1/2, where an
/// ulp of the result is u/2 or more, so that the kernel's error is below
/// 0.31 ulp of the result either way: 0.81 ulp after the rounding. log₂ 1 is
/// +0, log₂ of ±0 is −∞, of a negative x or NaN NaN, and of +∞ +∞, as C23 has
/// them.
#[must_use]
pub fn log2(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x == f64::INFINITY {
        return x;
    }
    let (k, m) = split(x);
    let (log, log_lo) = kernel(m - 1.0);
    if k == 0.0 {
        return log + log_lo;
    }
    let (total, total_lo) = fast_two_sum(k, log);
    total + (total_lo + log_lo)
}

/// log₂(1 + x): within 0.81 ulp of log₂(1 + x), and so within 1 ulp of the
/// correctly rounded result (MATH §12.4, tested against CORE-MATH's
/// `cr_log2p1`).
///
/// Where 1 + x lies from fl(√2)/2 up to fl(√2), the kernel takes x itself as
/// t, so that a small x loses nothing to the rounding of 1 + x: as log₂, within
/// 0.81 ulp. Elsewhere 1 + x is split as u + e exactly, u = fl(1 + x), and
/// log₂(1 + x) = log₂ u + log₂(1 + e/u), where |e/u| ≤ 2⁻⁵³ and the second term
/// is e/(u ln 2) but for a term below 2⁻¹⁰⁶, which the kernel's error far
/// exceeds. log₂ u is k + log₂(1 + t) as for log₂, and |log₂(1 + x)| is a
/// little below 1/2 or more there: within 0.81 ulp but for the correction's own
/// rounding, of the order of u². log₂p1 of ±0 is ±0, of −1 −∞, of x below −1
/// or NaN NaN, and of +∞ +∞, as C23 has them.
#[must_use]
pub fn log2p1(x: f64) -> f64 {
    if x.is_nan() || x < -1.0 {
        return f64::NAN;
    }
    if x <= -1.0 {
        return f64::NEG_INFINITY;
    }
    if x == 0.0 || x == f64::INFINITY {
        return x;
    }
    if (LEAST..BEYOND).contains(&x) {
        let (log, log_lo) = kernel(x);
        return log + log_lo;
    }
    let (sum, error) = two_sum(1.0, x);
    let (k, m) = split(sum);
    let (log, log_lo) = kernel(m - 1.0);
    let correction = error / sum * LOG2_0_HI;
    let (total, total_lo) = fast_two_sum(k, log);
    total + (total_lo + (log_lo + correction))
}

/// x > 0 finite as 2^k m, k an integer held as a binary64 and m from fl(√2)/2
/// up to fl(√2), exactly: from the bits, a subnormal x scaled by 2⁵⁴ first.
fn split(x: f64) -> (f64, f64) {
    let (bits, scaled) = if x < f64::MIN_POSITIVE {
        ((x * f64::from_bits(0x4350_0000_0000_0000)).to_bits(), 54.0)
    } else {
        (x.to_bits(), 0.0)
    };
    // The exponent's field of a finite binary64 is below 2047, which u16 holds.
    let field = f64::from(u16::try_from(bits >> 52).unwrap_or(0));
    let mantissa = bits & MANTISSA;
    if mantissa >= SQRT_2.to_bits() & MANTISSA {
        (
            field - 1022.0 - scaled,
            f64::from_bits(mantissa | 0x3fe0_0000_0000_0000),
        )
    } else {
        (
            field - 1023.0 - scaled,
            f64::from_bits(mantissa | 0x3ff0_0000_0000_0000),
        )
    }
}

/// log₂(1 + t) for t from fl(√2)/2 − 1 up to fl(√2) − 1, t exact: t times
/// c₀ + c₁t + t²·Q(t), as a double-double.
///
/// c₀ + c₁t is a double-double, its parts exact but for terms of the order of
/// u². What rounds at the order of u is t²Q, |t²Q| ≤ 0.053 of the sum, which
/// is at least 1.2: Q from Horner's fused steps within 2.85u, t² and the
/// product u each, and the one addition that brings it in, with the low
/// parts, u: within 0.31u of the sum, the polynomial's own bound, 2.19 · 10⁻¹⁹
/// relative, below 0.002u. The product by t is exact as a double-double but for
/// the rounding of its low part, of the order of u².
/// (`scripts/rounding_bounds.py` computes the numbers.)
#[inline]
fn kernel(t: f64) -> (f64, f64) {
    let mut tail = LOG2_22;
    for c in [
        LOG2_21, LOG2_20, LOG2_19, LOG2_18, LOG2_17, LOG2_16, LOG2_15, LOG2_14, LOG2_13, LOG2_12, LOG2_11, LOG2_10,
        LOG2_9, LOG2_8, LOG2_7, LOG2_6, LOG2_5, LOG2_4, LOG2_3, LOG2_2,
    ] {
        tail = tail.mul_add(t, c);
    }
    let second = t * t * tail;
    let (linear, linear_lo) = two_product(LOG2_1_HI, t);
    let linear_lo = LOG2_1_LO.mul_add(t, linear_lo);
    let (sum, sum_lo) = fast_two_sum(LOG2_0_HI, linear);
    let sum_lo = second + (sum_lo + (LOG2_0_LO + linear_lo));
    let (log, log_lo) = two_product(t, sum);
    (log, t.mul_add(sum_lo, log_lo))
}

#[cfg(test)]
mod tests {
    use super::{BEYOND, LEAST, log2, log2p1, split};

    #[test]
    fn exact_points() {
        // Every power of two, normal and subnormal, gives its exponent.
        for k in -1074_i32..1024 {
            let x = if k < -1022 {
                f64::from_bits(1_u64 << (k + 1074))
            } else {
                f64::from_bits(u64::from(k.wrapping_add(1023).cast_unsigned()) << 52)
            };
            assert_eq!(log2(x), f64::from(k), "log2(2^{k})");
        }
        assert_eq!(log2(1.0).to_bits(), 0.0_f64.to_bits());
        assert_eq!(log2(0.0), f64::NEG_INFINITY);
        assert_eq!(log2(-0.0), f64::NEG_INFINITY);
        assert!(log2(-1.0).is_nan() && log2(f64::NAN).is_nan() && log2(f64::NEG_INFINITY).is_nan());
        assert_eq!(log2(f64::INFINITY), f64::INFINITY);
        assert_eq!(log2p1(0.0).to_bits(), 0.0_f64.to_bits());
        assert_eq!(log2p1(-0.0).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(log2p1(1.0), 1.0);
        assert_eq!(log2p1(3.0), 2.0);
        assert_eq!(log2p1(-0.5), -1.0);
        assert_eq!(log2p1(-1.0), f64::NEG_INFINITY);
        assert!(log2p1(-2.0).is_nan() && log2p1(f64::NAN).is_nan());
        assert_eq!(log2p1(f64::INFINITY), f64::INFINITY);
    }

    #[test]
    fn the_split_is_exact_and_keeps_t_within_the_kernel() {
        for x in [
            1.0,
            0.75,
            core::f64::consts::SQRT_2,
            core::f64::consts::SQRT_2.next_down(),
            core::f64::consts::SQRT_2 / 2.0,
            f64::MIN_POSITIVE,
            f64::from_bits(1),
            f64::from_bits(0x000f_ffff_ffff_ffff),
            f64::MAX,
        ] {
            let (k, m) = split(x);
            let t = m - 1.0;
            assert!((LEAST..BEYOND).contains(&t), "{x:e}: t = {t:e}");
            // 2^k m is x again, exactly: exp2 of an integer is exact.
            let back = if k > 0.0 {
                (2.0 * m) * crate::exp2(k - 1.0)
            } else {
                m * crate::exp2(k + 54.0) * crate::exp2(-54.0)
            };
            assert_eq!(back.to_bits(), x.to_bits(), "{x:e}");
        }
    }
}
