// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! What the tests of keptone-math share: a random source and the measure of an
//! error in ulp.

#![allow(dead_code, reason = "each test file takes what it needs of this module")]

/// `SplitMix64`, the generator kiral's tests use: a 64-bit state advanced by a
/// constant and mixed, so that a seed gives the same sequence on every system.
#[derive(Debug, Clone)]
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    /// The next 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A uniform binary64 of [0, 1), on the grid of 2⁻⁵².
    pub fn unit(&mut self) -> f64 {
        f64::from_bits(0x3ff0_0000_0000_0000 | (self.next_u64() >> 12)) - 1.0
    }

    /// A uniform integer of [0, n).
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    /// A binary64 of random sign, exponent field in [0, top] (0 the subnormals)
    /// and mantissa: magnitudes spread evenly over their binades.
    pub fn binades(&mut self, top: u64) -> f64 {
        let r = self.next_u64();
        let exponent = self.below(top + 1);
        f64::from_bits((r & 0x8000_0000_0000_0000) | (exponent << 52) | (r & 0x000f_ffff_ffff_ffff))
    }
}

/// The unit in the last place at y: 2^(e − 52) for |y| in [2^e, 2^(e+1)), and
/// 2⁻¹⁰⁷⁴ below the normal range. y is finite.
pub fn ulp(y: f64) -> f64 {
    let field = (y.abs().to_bits() >> 52) & 0x7ff;
    if field > 52 {
        f64::from_bits((field - 52) << 52)
    } else if field > 0 {
        f64::from_bits(1 << (field - 1))
    } else {
        f64::from_bits(1)
    }
}

/// |y − reference| in ulp of the reference. The two are close enough for the
/// difference to be exact (Sterbenz's lemma), and the ulp is a power of two.
pub fn ulp_error(y: f64, reference: f64) -> f64 {
    (y - reference).abs() / ulp(reference)
}

/// A double-double, hi + lo with |lo| at most half an ulp of hi: about 106
/// bits, for references that binary64 alone cannot give. The operations are
/// Dekker's and Knuth's, each within a few units of 2⁻¹⁰⁶ of the result.
#[derive(Debug, Clone, Copy)]
pub struct Dd(pub f64, pub f64);

impl Dd {
    /// self + other.
    pub fn add(self, other: Self) -> Self {
        let s = self.0 + other.0;
        let a = s - other.0;
        let e = (self.0 - a) + (other.0 - (s - a)) + self.1 + other.1;
        Self::normal(s, e)
    }

    /// self · other.
    pub fn mul(self, other: Self) -> Self {
        let p = self.0 * other.0;
        let e = self.0.mul_add(other.0, -p) + self.0.mul_add(other.1, self.1 * other.0);
        Self::normal(p, e)
    }

    /// self / d, d a binary64.
    pub fn div(self, d: f64) -> Self {
        let q = self.0 / d;
        let r = (-q).mul_add(d, self.0) + self.1;
        Self::normal(q, r / d)
    }

    /// −self.
    pub fn neg(self) -> Self {
        Self(-self.0, -self.1)
    }

    /// hi + lo rounded to binary64.
    pub fn value(self) -> f64 {
        self.0 + self.1
    }

    fn normal(hi: f64, lo: f64) -> Self {
        let s = hi + lo;
        Self(s, lo - (s - hi))
    }
}

/// 2π as a double-double.
pub const TWO_PI: Dd = Dd(core::f64::consts::TAU, 2.449_293_598_294_706_4e-16);

/// sin 2πx as a double-double, within about 10⁻³⁰ of it: x reduces exactly to
/// t, |t| ≤ 1/8, and a quarter turn, and the Taylor series of the sine and
/// cosine of 2πt, to their terms in θ²⁹ and θ²⁸, are summed in double-double.
pub fn sin2pi_dd(x: f64) -> Dd {
    let r = x - x.round_ties_even();
    let q = (4.0 * r).round_ties_even();
    let theta = TWO_PI.mul(Dd(r - 0.25 * q, 0.0));
    if q > 1.5 || q < -1.5 {
        series(theta, 1).neg()
    } else if q > 0.5 {
        series(theta, 0)
    } else if q < -0.5 {
        series(theta, 0).neg()
    } else {
        series(theta, 1)
    }
}

/// Σ (−1)ᵏ θ^(2k+first)/(2k+first)!: the cosine for first = 0, the sine for 1.
fn series(theta: Dd, first: u32) -> Dd {
    let square = theta.mul(theta);
    let mut term = if first == 0 { Dd(1.0, 0.0) } else { theta };
    let mut sum = term;
    for k in 1..=14_u32 {
        let n = 2 * k + first;
        term = term.mul(square).neg().div(f64::from((n - 1) * n));
        sum = sum.add(term);
    }
    sum
}
