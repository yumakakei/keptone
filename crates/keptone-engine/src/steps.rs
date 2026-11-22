// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Between the steps (docs/MATH.md §0.6): a law that the DX7 computes from its
//! parameters' steps, taken at whole inputs and linear between them, in each
//! input that lies between integers and multilinearly where several do.

use crate::binary64::finite;

/// below + part·(above − below), saturated (MATH §0.5): the linear step
/// between two values of a law at whole inputs (MATH §0.6).
pub(crate) fn lerp(part: f64, below: f64, above: f64) -> f64 {
    finite(part.mul_add(finite(above - below), below))
}

/// A law at whole inputs, linear between them (MATH §0.6): `at` at the integer
/// at or below `x` and at the next, weighted by x's part, or `at(x)` alone
/// where x is whole.
///
/// Of values of the law within e of themselves and at most M in magnitude, the
/// result is within e + 4uM of the law between them: x's part is exact but in
/// (−1, 0), where it rounds within u/2, the difference of the two values
/// rounds within u of itself, at most 2M, and the fused step within u of the
/// result.
pub(crate) fn between(x: f64, at: impl Fn(f64) -> f64) -> f64 {
    let whole = x.floor();
    let below = at(whole);
    let part = x - whole;
    if part > 0.0 {
        lerp(part, below, at(whole + 1.0))
    } else {
        below
    }
}

/// ⌊165n/64⌋ at a whole n, the firmware's scaling of a step of a parameter by
/// 165/64, from 0 to 99 to 0 to 255 \[AJXS-ROM `PATCH_ACTIVATE_SCALE_VALUE`\]:
/// n splits exactly into 64q + r, Euclid's r from 0 up to 64, so that
/// ⌊165n/64⌋ = 165q + ⌊165r/64⌋ takes its floor of an exact value however
/// large n is. Exact below 2⁵³ in magnitude, |n| below 3.49·10¹⁵; beyond within
/// 2u of itself, the product's and the sum's roundings, and binary64's largest
/// finite number, with its sign, where it lies beyond it (MATH §0.5).
pub(crate) fn scale_value(whole: f64) -> f64 {
    let q = (whole / 64.0).floor();
    let r = whole - 64.0 * q;
    finite(165.0 * q + (165.0 * r / 64.0).floor())
}
