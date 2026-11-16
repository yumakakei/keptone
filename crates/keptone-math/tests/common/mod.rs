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
