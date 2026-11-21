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
