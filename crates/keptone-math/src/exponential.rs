// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The exponential of base 2, and 2^z − 1 (docs/MATH.md §12.3, §12.5).

use crate::coefficients::{
    EXP2_1_HI, EXP2_1_LO, EXP2_2, EXP2_3, EXP2_4, EXP2_5, EXP2_6, EXP2_7, EXP2_8, EXP2_9, EXP2_10, EXP2_11, EXP2_12,
    EXP2M1_0_HI, EXP2M1_0_LO, EXP2M1_1_HI, EXP2M1_1_LO, EXP2M1_2, EXP2M1_3, EXP2M1_4, EXP2M1_5, EXP2M1_6, EXP2M1_7,
    EXP2M1_8, EXP2M1_9, EXP2M1_10, EXP2M1_11,
};
use crate::eft::{fast_two_sum, two_product, two_sum};

/// 2^z: within 0.93 ulp of 2^z, and so within 1 ulp of the correctly rounded
/// result (MATH §12.3, tested against CORE-MATH's `cr_exp2`).
///
/// z splits exactly into k + r, k = round(z) and |r| ≤ 1/2; 2^r comes from a
/// minimax polynomial within 0.86 ulp (the kernel below says why), and the
/// result is scaled by 2^k built from exponent bits. The scaling is exact in
/// the normal range. A subnormal result is rounded once more, at the last
/// product, where the error of 2^r is at most 0.43 of the subnormal ulp (its
/// largest when k = −1022 and 2^r < 1): error ≤ 0.93 ulp. Results above the
/// largest binary64 give +∞, below half the least subnormal +0, and NaN gives
/// NaN. Keptone's gains never come near either end (MATH §13.4).
#[must_use]
pub fn exp2(z: f64) -> f64 {
    if z.is_nan() {
        return z;
    }
    if z >= 1024.0 {
        return f64::INFINITY;
    }
    if z <= -1076.0 {
        return 0.0;
    }
    let k = z.round_ties_even();
    let p = exp2_kernel(z - k);
    // k is an integer of [-1076, 1024], which i64 holds exactly.
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "k is an integer within ±2^11, which i64 holds exactly"
    )]
    let k = k as i64;
    if k > 1023 {
        p * 2.0 * power_of_two(1023)
    } else if k < -1022 {
        p * power_of_two(k + 64) * power_of_two(-64)
    } else {
        p * power_of_two(k)
    }
}

/// 2^z − 1: within 0.62 ulp of 2^z − 1, and so within 1 ulp of the correctly
/// rounded result (MATH §12.5, tested against CORE-MATH's `cr_exp2m1`).
///
/// For |z| ≤ 1/2 the kernel below gives 2^z − 1 as a double-double, within
/// 0.096u of itself, rounded once: 0.60 ulp. Beyond, z splits exactly into
/// k + r, k = round(z) and |r| ≤ 1/2, and the kernel gives D = 2^r − 1, so that
/// 2^z − 1 = (2^k − 1) + 2^k·D: 2^k − 1 is the sum of two binary64 exactly, as
/// is that and the high part of 2^k·D, an exact scaling; the low parts, below
/// an ulp of the result, are summed apart and the whole rounded once. D's error
/// is below 1.18 times its own beside an ulp of the result, the worst at k = 1,
/// where 2D reaches −0.59 and the result 0.41: 0.62 ulp. Where k is 1024, the
/// same is done with 2^1023 and 1/2, and the result doubled, exactly but where
/// it overflows to +∞ as it should. 2^z − 1 is −1 from z = −54 down, within half
/// an ulp of it, a tie at −54 rounding to −1, which is even. ±0 gives ±0, +∞
/// gives +∞, −∞ gives −1 and NaN gives NaN, as C23 has them.
#[must_use]
pub fn exp2m1(z: f64) -> f64 {
    if z.is_nan() || z == 0.0 {
        return z;
    }
    if z >= 1024.0 {
        return f64::INFINITY;
    }
    if z <= -54.0 {
        return -1.0;
    }
    if z.abs() <= 0.5 {
        let (excess, excess_lo) = exp2m1_kernel(z);
        return excess + excess_lo;
    }
    let k = z.round_ties_even();
    let (excess, excess_lo) = exp2m1_kernel(z - k);
    // k is an integer of [−54, 1024], which i64 holds exactly.
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "k is an integer within ±2^11, which i64 holds exactly"
    )]
    let k = k as i64;
    let (scale, one, twice) = if k > 1023 {
        (power_of_two(1023), 0.5, true)
    } else {
        (power_of_two(k), 1.0, false)
    };
    let (base, base_lo) = two_sum(scale, -one);
    let (sum, sum_lo) = two_sum(base, scale * excess);
    let result = sum + (sum_lo + (base_lo + scale * excess_lo));
    if twice { 2.0 * result } else { result }
}

/// 2^k for k in [−1022, 1023], from its exponent bits.
fn power_of_two(k: i64) -> f64 {
    f64::from_bits((k + 1023).cast_unsigned() << 52)
}

/// 2^r for |r| ≤ 1/2: 1 + c₁r + r²·Q(r), with c₁r and 1 + c₁r as double-doubles.
///
/// The double-doubles leave errors of the order of u² only. What rounds at the
/// order of u is r²Q, |r²Q| ≤ 0.068, computed within 3.42u (r² and the product,
/// u each, and Horner's fused steps, 1.42u; r itself is exact), and two
/// additions below 1/8, which round by u/16 each: 0.36u before the final
/// addition, which rounds once. An ulp of the result is u below 1 and 2u from 1
/// on: error ≤ 0.86 ulp; the polynomial's own bound, 9.31 · 10⁻²⁰ absolute, is
/// below 0.001u.
#[inline]
fn exp2_kernel(r: f64) -> f64 {
    let mut tail = EXP2_12;
    for c in [
        EXP2_11, EXP2_10, EXP2_9, EXP2_8, EXP2_7, EXP2_6, EXP2_5, EXP2_4, EXP2_3, EXP2_2,
    ] {
        tail = tail.mul_add(r, c);
    }
    let second = r * r * tail;
    let (a, a_lo) = two_product(EXP2_1_HI, r);
    let a_lo = EXP2_1_LO.mul_add(r, a_lo);
    let (s, s_lo) = fast_two_sum(1.0, a);
    s + (s_lo + (a_lo + second))
}

/// 2^r − 1 for |r| ≤ 1/2, r exact: r times c₀ + c₁r + r²·Q(r), as a
/// double-double.
///
/// c₀ + c₁r is a double-double, its parts exact but for terms of the order of
/// u². What rounds at the order of u is r²Q, |r²Q| ≤ 0.022 of the sum, which is
/// at least 0.58: Q from Horner's fused steps within 1.3u, r² and the product u
/// each, and the one addition that brings it in, with the low parts, u: within
/// 0.094u of the sum, the polynomial's own bound, 2.41 · 10⁻¹⁹ relative, below
/// 0.003u. The product by r is exact as a double-double but for the rounding of
/// its low part, of the order of u². (`scripts/rounding_bounds.py` computes the
/// numbers.)
#[inline]
fn exp2m1_kernel(r: f64) -> (f64, f64) {
    let mut tail = EXP2M1_11;
    for c in [
        EXP2M1_10, EXP2M1_9, EXP2M1_8, EXP2M1_7, EXP2M1_6, EXP2M1_5, EXP2M1_4, EXP2M1_3, EXP2M1_2,
    ] {
        tail = tail.mul_add(r, c);
    }
    let second = r * r * tail;
    let (linear, linear_lo) = two_product(EXP2M1_1_HI, r);
    let linear_lo = EXP2M1_1_LO.mul_add(r, linear_lo);
    let (sum, sum_lo) = fast_two_sum(EXP2M1_0_HI, linear);
    let sum_lo = second + (sum_lo + (EXP2M1_0_LO + linear_lo));
    let (excess, excess_lo) = two_product(r, sum);
    (excess, r.mul_add(sum_lo, excess_lo))
}

#[cfg(test)]
mod tests {
    use super::{exp2, exp2m1};

    #[test]
    fn exact_points() {
        // Every power of two, normal and subnormal, built from its bits.
        for k in -1074_i32..1024 {
            let bits = if k < -1022 {
                1_u64 << (k + 1074)
            } else {
                u64::from(k.wrapping_add(1023).cast_unsigned()) << 52
            };
            assert_eq!(exp2(f64::from(k)).to_bits(), bits, "exp2({k})");
        }
        assert_eq!(exp2(0.5).to_bits(), core::f64::consts::SQRT_2.to_bits());
        assert_eq!(exp2(1024.0), f64::INFINITY);
        assert_eq!(exp2(-1075.0).to_bits(), 0.0_f64.to_bits());
        assert_eq!(exp2(f64::NEG_INFINITY).to_bits(), 0.0_f64.to_bits());
        assert!(exp2(f64::NAN).is_nan());
    }

    #[test]
    fn exact_points_less_one() {
        // 2^k − 1 for every integer k that binary64 holds it at, and its
        // rounding beyond: to 2^k from k = 54 up, to −1 from k = −54 down.
        for k in -60_i32..1024 {
            let expected = if k >= 54 {
                exp2(f64::from(k))
            } else if k <= -54 {
                -1.0
            } else {
                exp2(f64::from(k)) - 1.0
            };
            assert_eq!(exp2m1(f64::from(k)).to_bits(), expected.to_bits(), "exp2m1({k})");
        }
        assert_eq!(exp2m1(0.0).to_bits(), 0.0_f64.to_bits());
        assert_eq!(exp2m1(-0.0).to_bits(), (-0.0_f64).to_bits());
        assert_eq!(exp2m1(1024.0), f64::INFINITY);
        assert_eq!(exp2m1(f64::INFINITY), f64::INFINITY);
        assert_eq!(exp2m1(f64::NEG_INFINITY), -1.0);
        assert!(exp2m1(f64::NAN).is_nan());
    }
}
