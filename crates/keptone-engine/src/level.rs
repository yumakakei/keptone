// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Levels (docs/MATH.md §3.1, §3.3, §3.4): the curve that maps a level of 0–99
//! to the DX7's level steps, the envelope's part of an operator's total level,
//! the total level, and its gain.
//!
//! A total level T is in the DX7's steps of 20·log₁₀2/256 dB, 3840 at full
//! scale, and its gain is 2^(T/256 − 14), 2 at full scale. Each function is a
//! law of MATH §3 as written, its levels any finite binary64, between the steps
//! interpolated as MATH §0.6 says and beyond the scale continued as §0.5 says,
//! and documents its error against the law's exact value at its binary64
//! inputs; tests/level.rs measures each against exact values
//! (`scripts/level_references.py`).

use keptone_math::{exp2, log2};

use crate::binary64::{finite, scaled, split};

/// s(L) at the steps 0 to 20, \[MSFA `scaleoutlevel`, LEVIEN-EG\] and the
/// firmware's values as well, whose table holds 127 − s(L) \[AJXS-ROM
/// `TABLE_LOG`\]; from 20 on s(L) is 28 + L.
const CURVE: [f64; 21] = [
    0.0, 5.0, 9.0, 13.0, 17.0, 20.0, 23.0, 25.0, 27.0, 29.0, 31.0, 33.0, 35.0, 37.0, 39.0, 41.0, 42.0, 43.0, 45.0,
    46.0, 48.0,
];

/// s(L) of MATH §3.1, the level curve: the DX7's at its steps, linear between
/// them (MATH §0.6), along its first step below 0 and 28 + L from 20 on (MATH
/// §0.5).
///
/// One rounding, a fused step between two steps, the sum from 20 on or the
/// product below 0, within u of s(L): L less its step is exact.
#[must_use]
pub fn curve(level: f64) -> f64 {
    if level >= 20.0 {
        return finite(28.0 + level);
    }
    if level < 0.0 {
        return finite(5.0 * level);
    }
    let step = (0..20_u8).rev().find(|&step| level >= f64::from(step)).unwrap_or(0);
    let below = CURVE[usize::from(step)];
    let above = CURVE[usize::from(step) + 1];
    (level - f64::from(step)).mul_add(above - below, below)
}

/// E(L) of MATH §3.3 at a step: 64⌊s(L)/2⌋, the firmware's level for the EGS
/// \[AJXS-ROM `PATCH_ACTIVATE_OPERATOR_EG_LEVEL`\], exact where it is below 2⁵³.
fn envelope_at(step: f64) -> f64 {
    finite(64.0 * (curve(step) / 2.0).floor())
}

/// The envelope's part of an operator's total level at an envelope level L
/// (MATH §3.3): E(L) = 64⌊s(L)/2⌋ at a step, linear between the steps as a
/// whole (MATH §0.6), not through s, whose floor would keep the steps. Below 1
/// the level is interpolated in the gain, E(1) + 256 log₂ L, so that the gain
/// falls linearly to silence at 0, and at or below 0 the operator is silent,
/// which `None` says.
///
/// From 1 up one fused rounding between two exact steps, L less its step being
/// exact: within u of E. Below 1, log₂'s 0.81 ulp, 1.62u of itself, times 256
/// and the fused step's rounding: within u|E| + 415u|log₂ L|.
#[must_use]
pub fn envelope(level: f64) -> Option<f64> {
    if level >= 1.0 {
        let step = level.floor();
        let below = envelope_at(step);
        Some(finite((level - step).mul_add(envelope_at(step + 1.0) - below, below)))
    } else if level > 0.0 {
        Some(256.0_f64.mul_add(log2(level), envelope_at(1.0)))
    } else {
        None
    }
}

/// T at full scale, L = OL = 99 without scaling or velocity: 4032 + 4064 − 4256
/// (MATH §3.3).
pub const FULL_SCALE: f64 = 3840.0;

/// The total level T of MATH §3.3, E + O − 4256: the envelope's part and the
/// operator's offset (§3.2). Two roundings: within u|E + O| + u|T|.
#[must_use]
pub fn total(envelope: f64, offset: f64) -> f64 {
    finite(finite(envelope + offset) - 4256.0)
}

/// The highest level whose gain is taken, 256·974, where the gain is 2⁹⁶⁰, so
/// that every sum of the engine, of fewer than 2⁶² terms, stays within binary64
/// (MATH §3.4).
pub const HIGHEST_LEVEL: f64 = 249_344.0;

/// The gain g = 2^(T/256 − 14) of a total level T (MATH §3.4), 2 at full scale,
/// T taken at most at [`HIGHEST_LEVEL`].
///
/// T/256 is exact, and splits exactly into an integer and a part within 1/2;
/// 2 to the part, within 0.93 ulp of it, is scaled by 2 to the integer less 14
/// exactly: within 1.86u of the gain relatively where it is normal, and within
/// two steps of the subnormals below.
#[must_use]
pub fn gain(level: f64) -> f64 {
    let (whole, part) = split(level.min(HIGHEST_LEVEL) / 256.0);
    scaled(exp2(part), whole - 14.0)
}
