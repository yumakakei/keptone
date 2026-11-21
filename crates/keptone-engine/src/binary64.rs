// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! What the laws share of binary64: a number split exactly into an integer and
//! a part, a scaling by a power of two that is exact in the normal range, and
//! the largest finite number taken for a result beyond it (docs/MATH.md §0.5).

use keptone_math::exp2;

/// 2¹⁰²³ and 2⁻⁹⁶⁹, the steps of [`scaled`] above and below the normal range's
/// exponents, the second 2⁻¹⁰²²·2⁵³ as musl's scalbn takes it.
const TWO_TO_1023: f64 = f64::from_bits(2046 << 52);
const TWO_TO_MINUS_969: f64 = f64::from_bits(54 << 52);

/// A number as the integer nearest it, ties to even, and the part within 1/2
/// that is left, exactly: Sterbenz's lemma where the integer is not 0, and the
/// number itself where it is.
pub(crate) fn split(value: f64) -> (f64, f64) {
    let whole = value.round_ties_even();
    (whole, value - whole)
}

/// value·2^k for an integral k, exactly where the result is normal: 2^k is
/// applied in steps of 2¹⁰²³ above the normal range's exponents and of 2⁻⁹⁶⁹
/// below them, two at most, as musl's scalbn takes them, so that a result in
/// the normal range is never rounded on the way and a subnormal one is
/// rounded within two steps of the subnormals; a result beyond binary64's
/// largest finite number is taken at it, with its sign (MATH §0.5).
pub(crate) fn scaled(value: f64, octaves: f64) -> f64 {
    let (mut value, mut octaves) = (value, octaves);
    for _ in 0..2 {
        if octaves > 1023.0 {
            value *= TWO_TO_1023;
            octaves -= 1023.0;
        }
        if octaves < -1022.0 {
            value *= TWO_TO_MINUS_969;
            octaves += 969.0;
        }
    }
    finite(value * exp2(octaves.clamp(-1022.0, 1023.0)))
}

/// A result beyond binary64's largest finite number, taken at it with its sign
/// (MATH §0.5).
pub(crate) fn finite(value: f64) -> f64 {
    value.clamp(-f64::MAX, f64::MAX)
}
