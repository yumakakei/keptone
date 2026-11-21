// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Pitch and frequency (docs/MATH.md §2.1–§2.3): the key after transposition,
//! the pitch that 12-TET gives it, the frequency ratio and the detune of an
//! operator in the ratio mode, the pitch of one in the fixed mode, and the
//! frequency of a pitch.
//!
//! A pitch is log₂ of a frequency in hertz, in octaves. Each function is a law
//! of MATH §2 as written, its parameters any finite binary64, on their scales
//! or beyond them (MATH §0.5), and documents its error against the law's exact
//! value at its binary64 inputs, in u = 2⁻⁵³; tests/pitch.rs measures each
//! against mpmath's values (`scripts/pitch_references.py`). A result beyond
//! binary64's largest finite number is taken at it, with its sign, after each
//! operation that can reach it (MATH §0.5).

use core::f64::consts::LOG2_10;

use keptone_math::{exp2, log2};

use crate::binary64::{finite, scaled, split};

pub mod phase;

/// How each whole octave of a shift moves (MATH §2.1, `tuning.transpose_octave`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransposeOctave {
    /// By twelve keys, as on the DX7.
    Keys,
    /// By the tuning's period, so that an octave of transposition spans it, as
    /// twelve keys span 12-TET's.
    Period,
}

/// Beyond this magnitude a shift's twelfth is a binary64 of a step of 1 or
/// more, 12·2⁵².
const WHOLE_TWELFTHS: f64 = 54_043_195_528_445_952.0;

/// τ(σ) of MATH §2.1: a shift of σ keys, each whole octave of twelve keys in
/// it moved by the tuning's period of `period` keys under
/// [`TransposeOctave::Period`], σ + (P − 12)·trunc(σ/12).
///
/// Exact under [`TransposeOctave::Keys`]. Under `Period`, trunc(σ/12) is exact
/// where |σ| < 12·2⁵², since σ/12 never rounds to an integer n that it lies
/// short of: a σ short of 12n is short of it by a step of 12n's at least, 8 or
/// 16 steps of n's, and its twelfth then short of n by 2/3 of a step of n's
/// at least, nearer to n's neighbour than to n. Beyond, σ/12 is within half
/// its step of 1 or more of itself, and its trunc within u|σ|/4 of the exact
/// one. The rest is one fused rounding, within u of the result, and that of
/// P − 12, exact where P lies from 6 to 24 (Sterbenz) and within u of it
/// elsewhere: within u(|τ| + |trunc(σ/12)|·|P − 12|) of τ, and beyond 12·2⁵²
/// within |P − 12|·u|σ|/4 more.
#[must_use]
pub fn shift(sigma: f64, octave: TransposeOctave, period: f64) -> f64 {
    match octave {
        TransposeOctave::Keys => sigma,
        TransposeOctave::Period => finite((period - 12.0).mul_add((sigma / 12.0).trunc(), sigma)),
    }
}

/// τ(TRANSPOSE − 24) of MATH §2.1: the voice's transposition, 24 meaning none,
/// as [`shift`] makes it of the shift TRANSPOSE − 24.
///
/// TRANSPOSE − 24 is exact from 12 to 48 (Sterbenz) and within u of itself
/// elsewhere, where it may round across a multiple of 12, at which τ jumps;
/// so its whole octaves are taken from TRANSPOSE itself, exactly where
/// |TRANSPOSE| < 12·2⁵²: with TRANSPOSE = 12q + r, q = trunc(TRANSPOSE/12) and
/// the remainder r exact, by one fused step, trunc((TRANSPOSE − 24)/12) is
/// q − 1 where TRANSPOSE lies below 24 and r above 0, and q − 2 elsewhere.
/// The bound of [`shift`] holds, with u|TRANSPOSE − 24| more for its rounding.
#[must_use]
pub fn transposition(transpose: f64, octave: TransposeOctave, period: f64) -> f64 {
    let sigma = transpose - 24.0;
    match octave {
        TransposeOctave::Keys => sigma,
        TransposeOctave::Period => {
            let twelfths = (transpose / 12.0).trunc();
            let rest = (-12.0_f64).mul_add(twelfths, transpose);
            let octaves = if transpose < 24.0 && rest > 0.0 && transpose.abs() < WHOLE_TWELFTHS {
                twelfths - 1.0
            } else {
                twelfths - 2.0
            };
            finite((period - 12.0).mul_add(octaves, sigma))
        }
    }
}

/// k′ of MATH §2.1: a key after the voice's [`transposition`] and the part's
/// note shift, each shifted on its own, summed as (k + τ(TRANSPOSE − 24)) +
/// τ(s), and never clamped (MATH §15). Each of the two additions rounds within
/// u of its sum, beside the bounds of the two shifts.
#[must_use]
pub fn transposed_key(key: f64, transpose: f64, note_shift: f64, octave: TransposeOctave, period: f64) -> f64 {
    finite(finite(key + transposition(transpose, octave, period)) + shift(note_shift, octave, period))
}

/// 12-TET, the tuning by default (MATH §2.1): A4, key 69, at `tuning.a4_hz`,
/// and twelve keys to the octave.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TwelveTet {
    /// log₂ of A4's frequency in hertz, within 0.81 ulp of it (`keptone_math::log2`).
    a4: f64,
}

impl TwelveTet {
    /// The key whose pitch the tuning fixes, A4 (MATH §2.6).
    pub const REFERENCE_KEY: f64 = 69.0;

    /// The period in keys (MATH §2.1).
    pub const PERIOD: f64 = 12.0;

    /// 12-TET with A4 at `a4_hz` hertz, which the bounds of `tuning.a4_hz` keep
    /// above 0 (MATH §17).
    #[must_use]
    pub fn new(a4_hz: f64) -> Self {
        Self { a4: log2(a4_hz) }
    }

    /// The pitch of a key, log₂ a4 + (k − 69)/12.
    ///
    /// Within u(|p| + 1.62|log₂ a4| + |k − 69|/6) of it, but for terms of the
    /// order of u²: the sum's rounding, log₂'s 0.81 ulp, at most 1.62u of
    /// itself, and the roundings of k − 69 and of its twelfth, u of the twelfth
    /// each. At key 69 it is log₂ a4 as `new` took it.
    #[must_use]
    pub fn pitch(&self, key: f64) -> f64 {
        self.a4 + (key - 69.0) / 12.0
    }

    /// The key whose pitch this is, on the scale of 12-TET's keys from A4
    /// (MATH §2.6): 69 + 12(p − log₂ a4), the inverse of [`Self::pitch`].
    ///
    /// Within u(|k| + 12|p − log₂ a4| + 19.44|log₂ a4|) of it, but for terms of
    /// the order of u²: the fused step's rounding, that of p − log₂ a4 times 12,
    /// and log₂'s error times 12.
    #[must_use]
    pub fn key(&self, pitch: f64) -> f64 {
        finite(12.0_f64.mul_add(pitch - self.a4, 69.0))
    }
}

/// The frequency ratio of an operator in the ratio mode (MATH §2.2):
/// ρ(COARSE)·(1 + FINE/100), where ρ(n) = n from 1 up and (1 + n)/2 below, so
/// that ρ(0) = 1/2 and the ratios of the DX7's steps, 0.5 and 1 to 31, are
/// linear in between and go on beyond them \[MSFA `coarsemul`\].
///
/// Computed as ρ·((100 + FINE)/100), within γ₄ = 4u/(1 − 4u) of the ratio
/// relatively: the roundings of 100 + FINE, of its hundredth and of the
/// product, and below 1 that of 1 + COARSE, the halving being exact.
#[must_use]
pub fn ratio(coarse: f64, fine: f64) -> f64 {
    let rho = if coarse >= 1.0 {
        coarse
    } else {
        f64::midpoint(1.0, coarse)
    };
    finite(rho * ((100.0 + fine) / 100.0))
}

/// The lowest pitch at which MSFA's detune law was measured, 12-TET's key 0 at
/// A4 = 440 Hz: log₂ 440 − 69/12, correctly rounded (MATH §2.2).
const MEASURED_LOWEST: f64 = f64::from_bits(0x4008_4039_8580_00b3); // 3.0313597135246595

/// The highest, key 127: log₂ 440 + 58/12, correctly rounded.
const MEASURED_HIGHEST: f64 = f64::from_bits(0x402b_3ab9_0c0a_aad7); // 13.614693046857992

/// 0.396/ln 2, correctly rounded: e^(−0.396p) = 2^(−p·0.396/ln 2).
const DETUNE_SLOPE: f64 = f64::from_bits(0x3fe2_4826_1ced_3698); // 0.5713072361920295

/// 0.0209/7, correctly rounded.
const DETUNE_SCALE: f64 = f64::from_bits(0x3f68_757f_26cb_833f); // 0.0029857142857142856

/// δ of MATH §2.2, in octaves: MSFA's detune law \[MSFA `osc_freq`\],
/// 0.0209·e^(−0.396p)·p·(DETUNE − 7)/7, at the pitch p = tuning(k′) of the key
/// without its modulation, taken within the pitches at which the law was
/// measured, 12-TET's keys 0 to 127 at A4 = 440 Hz, so that it stays finite
/// beyond them. The law is provisional (VERIFICATION V2).
///
/// Within 18u of δ relatively, but for terms of the order of u²: the
/// exponent −0.396p/ln 2, at most 7.78 in magnitude within the clamp, carries
/// the roundings of its constant and of its product, 15.56u of it at most,
/// which 2^x turns into 10.79u of the exponential, beside exp2's 0.93 ulp of
/// it, 1.86u; then the constant 0.0209/7, the three products and
/// DETUNE − 7, which is exact from 3.5 to 14, u each.
#[must_use]
pub fn detune(detune: f64, pitch: f64) -> f64 {
    let p = pitch.clamp(MEASURED_LOWEST, MEASURED_HIGHEST);
    DETUNE_SCALE * exp2(-DETUNE_SLOPE * p) * p * (detune - 7.0)
}

/// How the detune moves an operator in the fixed mode (MATH §2.3,
/// `osc.fixed_detune`), each step of DETUNE from 7 a step of 13457·2⁻²⁴ octaves
/// \[MSFA\]: provisional (VERIFICATION V3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FixedDetune {
    /// MSFA's: above 7 the detune raises the pitch, and below it does nothing.
    Msfa,
    /// The same law on both sides of 7.
    Symmetric,
    /// No detune.
    None,
}

/// 13457·2⁻²⁴ octaves, exactly: a step of the fixed mode's detune \[MSFA\].
const FIXED_DETUNE_STEP: f64 = 13457.0 / 16_777_216.0;

/// The detune of an operator in the fixed mode (MATH §2.3), in octaves:
/// 13457·2⁻²⁴·(DETUNE − 7) on the side of 7 that the law takes, and 0 elsewhere. Within 2u of itself relatively: the
/// roundings of DETUNE − 7, exact from 3.5 to 14, and of the product.
#[must_use]
pub fn fixed_detune(detune: f64, law: FixedDetune) -> f64 {
    let steps = detune - 7.0;
    match law {
        FixedDetune::Msfa if steps > 0.0 => FIXED_DETUNE_STEP * steps,
        FixedDetune::Symmetric => FIXED_DETUNE_STEP * steps,
        FixedDetune::Msfa | FixedDetune::None => 0.0,
    }
}

/// The pitch of an operator in the fixed mode before its modulation (MATH
/// §2.3): log₂ 10·(φ(COARSE) + FINE/100) + δ, δ its [`fixed_detune`], so that
/// COARSE 0 to 3 give 1, 10, 100 and 1000 Hz, each times 10^(FINE/100)
/// \[MSFA, DX7-MANUAL\], where φ(C) is C mod 4 at an integer, the remainder of
/// Euclid's division, and between two integers linear between their values
/// (MATH §0.6), so that from 3 to 4 it falls from 3 to 0.
///
/// φ is within u|φ| of itself, each of its differences being exact and its
/// last step rounding once; with the roundings of FINE/100 and of the sum, u
/// of each, that of log₂ 10, u of it, the fused step's, u|p|, and the
/// detune's 2u of itself, the pitch is within
/// u(|p| + log₂ 10·(2|φ + FINE/100| + |φ| + |FINE|/100) + 2|δ|) of itself, but
/// for terms of the order of u².
#[must_use]
pub fn fixed_pitch(coarse: f64, fine: f64, detune: f64, law: FixedDetune) -> f64 {
    LOG2_10.mul_add(phi(coarse) + fine / 100.0, fixed_detune(detune, law))
}

/// φ of [`fixed_pitch`]: from an integer n of remainder 0, 1 or 2 it rises as
/// n mod 4 + (C − n), and from one of remainder 3 it falls as 3((n + 1) − C).
/// The floor and the remainder are exact, and so is each difference: C − n
/// where n is not −1 mod 4, C and n lying within a factor of 2 of each other or
/// n being 0, and (n + 1) − C where it is, n + 1 then exact, being odd and
/// below 2⁵³ in magnitude, and C within a factor of 2 of it or it being 0
/// (Sterbenz). The one rounding left, of the sum or of the product, is within
/// u of φ.
fn phi(coarse: f64) -> f64 {
    let floor = coarse.floor();
    let below = floor.rem_euclid(4.0);
    if below >= 3.0 {
        3.0 * ((floor + 1.0) - coarse)
    } else {
        below + (coarse - floor)
    }
}

/// The highest pitch, in octaves: a pitch above it is taken at it (MATH §2.2,
/// §2.3), where a sample's increment of the phase is a whole number of turns
/// and the phase stands still.
pub const HIGHEST_PITCH: f64 = 1023.0;

/// The frequency in hertz of a pitch, times a ratio (MATH §2.2, §2.3):
/// ratio·2^p, the pitch held at [`HIGHEST_PITCH`], and a product beyond
/// binary64's largest finite number taken at it, with its sign (MATH §0.5).
///
/// p splits exactly into k + t, k = round(p) and |t| ≤ 1/2 (Sterbenz), and the
/// ratio times 2^t is scaled by 2^k, exactly in the normal range, so that a
/// large ratio brings a low pitch's product back into it: within 2.86u of the
/// frequency relatively where it is normal, exp2's 0.93 ulp, at most 1.86u,
/// and the product's rounding, but for terms of the order of u². Below 2⁻¹⁰²²
/// the scaling takes steps of 2⁻⁹⁶⁹ first, as musl's scalbn does, and rounds
/// within two steps of the subnormals; a frequency below binary64's least is
/// 0.
#[must_use]
pub fn frequency(ratio: f64, pitch: f64) -> f64 {
    let (whole, part) = split(pitch.min(HIGHEST_PITCH));
    scaled(ratio * exp2(part), whole)
}
