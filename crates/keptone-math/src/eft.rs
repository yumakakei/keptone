// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Error-free transformations: a sum or a product of two binary64 as the
//! rounded result and the exact error of the rounding, which binary64 holds
//! (Dekker's and Knuth's; [HIGHAM] §4.3).

/// a·b as (p, e) with p = fl(a·b) and p + e = a·b exactly: the fused operation
/// rounds once, so a·b − p is computed exactly. Exact unless a·b underflows.
#[inline]
pub(crate) fn two_product(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    (p, a.mul_add(b, -p))
}

/// a + b as (s, e) with s = fl(a + b) and s + e = a + b exactly, given |a| ≥ |b|
/// or a = 0 (Dekker's fast two-sum).
#[inline]
pub(crate) fn fast_two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    (s, b - (s - a))
}

/// a + b as (s, e) with s = fl(a + b) and s + e = a + b exactly, whatever the
/// sizes of a and b, unless the sum overflows (Knuth's two-sum).
#[inline]
pub(crate) fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let a_part = s - b;
    let b_part = s - a_part;
    (s, (a - a_part) + (b - b_part))
}
