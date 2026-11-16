// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The exponential of base 2 (docs/MATH.md §12.3).

use crate::coefficients::{
    EXP2_1_HI, EXP2_1_LO, EXP2_2, EXP2_3, EXP2_4, EXP2_5, EXP2_6, EXP2_7, EXP2_8, EXP2_9, EXP2_10, EXP2_11, EXP2_12,
};
use crate::eft::{fast_two_sum, two_product};

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

#[cfg(test)]
mod tests {
    use super::exp2;

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
}
