// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Controllers and amplitude modulation (docs/MATH.md §7.1, §7.2), and the
//! pitch's modulation by the LFO and the bend (§2.4): a step of a depth or a
//! range scaled as the firmware scales it; the depth of the controllers
//! assigned to a destination, the depths of the LFO's modulations with them,
//! and the EG bias, each in the two evaluations of MATH §0.7, so that what a
//! parameter beyond its scale adds passes the instrument's limits; the LFO's
//! pitch; the part of its amplitude that an operator keeps and the level by
//! which that lowers it; and the bend from a value of its message, its range
//! and its step. Each is a function of its inputs at an instant, which a voice
//! evaluates wherever it needs it (MATH §0.3).
//!
//! Each function documents its error against the law's exact value at its
//! binary64 inputs, relatively or in the magnitudes of its values; where a
//! value is subnormal, each rounding adds up to 2⁻¹⁰⁷⁵ more. tests/control.rs
//! measures each against exact values (`scripts/control_references.py`).

use keptone_math::eft::two_product;
use keptone_math::log2;

use crate::binary64::finite;
use crate::steps::{between, lerp, scale_value};

/// The saturation of the firmware's sums of depths, 255/256 (MATH §7.1).
const SATURATION: f64 = 255.0 / 256.0;

/// pmstab of MATH §2.4, the pitch's sensitivity in 1/256 octave at PMS 0 to 7
/// \[MSFA `pitchmodsenstab`; AJXS-ROM\].
const PITCH_SENSITIVITY: [f64; 8] = [0.0, 10.0, 20.0, 33.0, 55.0, 92.0, 153.0, 255.0];

/// The amplitude's sensitivity of MATH §7.2 in 1/255 at AMS 0 to 3 \[MSFA
/// `ampmodsenstab`\], provisional (VERIFICATION V14).
const AMPLITUDE_SENSITIVITY: [f64; 4] = [0.0, 66.0, 109.0, 255.0];

/// A whole step of a table's scale as its index.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole binary64 of [0, 7] converts exactly"
)]
fn index(step: f64) -> usize {
    step as usize
}

/// A step of a depth or a range, 0 to 99 on its scale, scaled as the firmware
/// scales it, ⌊165v/64⌋/256 \[AJXS-ROM `PATCH_ACTIVATE_SCALE_VALUE`\]: from 0 to
/// 255/256 on the scale at whole v, linear between them (MATH §0.6), and as
/// written beyond the scale (§0.5).
///
/// Exact at whole values below 3.49·10¹⁵ in magnitude, where ⌊165v/64⌋ is
/// below 2⁵³, and within 2u of itself beyond. Between whole values there the
/// two steps and their difference are exact, so that the fused step rounds
/// alone, within u|d|, and the part of a v in (−1, 0), alone inexact, within
/// u/2 of a difference of 3/256: 3u/512 more there. Between whole values
/// beyond, the steps' 2u and the fused step's u: within 3u|d|.
#[must_use]
pub fn depth(value: f64) -> f64 {
    between(value, |whole| scale_value(whole) / 256.0)
}

/// A value of a law with limits in the two evaluations of MATH §0.7.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limited {
    /// The first evaluation, with the parameters as they are: the law's value,
    /// which passes a limit by what the parameters beyond their scales add.
    pub value: f64,
    /// The second evaluation, with each parameter beyond its scale at the
    /// scale's nearer end: the instrument's value, within its limits.
    pub on_scale: f64,
}

impl Limited {
    /// A limit at most a bound (MATH §0.7): the second evaluation takes the
    /// bound where it lies beyond it, and the first that value plus the
    /// difference between the two at the limit's input.
    fn at_most(self, bound: f64) -> Self {
        let on_scale = self.on_scale.min(bound);
        Self {
            value: passed(on_scale, self),
            on_scale,
        }
    }

    /// A limit at least a bound, likewise.
    fn at_least(self, bound: f64) -> Self {
        let on_scale = self.on_scale.max(bound);
        Self {
            value: passed(on_scale, self),
            on_scale,
        }
    }
}

/// The first evaluation at a limit: the second's value there plus the
/// difference between the two evaluations at the limit's input, exactly the
/// second's where the two agree, the difference then 0, and otherwise within
/// u of the difference and u of the result.
fn passed(on_scale: f64, input: Limited) -> f64 {
    finite(on_scale + finite(input.value - input.on_scale))
}

/// A controller's position and range (MATH §7.1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Source {
    /// Its position x, from 0 at rest to 1 at its far end.
    pub position: f64,
    /// Its RANGE, 0 to 99 on its scale.
    pub range: f64,
}

/// The controllers' depth of MATH §7.1, of the controllers assigned to a
/// destination, the pitch or the amplitude: their depths x·r, r the [`depth`]
/// of the range,
/// summed in the order given, which the engine takes from the function data:
/// the modulation wheel, the foot controller, the breath controller and
/// aftertouch; and saturating at 255/256, a limit of the instrument that a
/// range beyond its scale passes (MATH §0.7).
///
/// Each evaluation sums with fused steps: within Σ|x|e + uΣ|S| of its exact
/// value, e the error of each depth and S the sum after each controller. At
/// the limit the first evaluation, where it differs from the second, adds
/// the second's error, the difference's rounding within u of it and the
/// sum's within u|d|.
#[must_use]
pub fn controllers(sources: &[Source]) -> Limited {
    let (mut value, mut on_scale) = (0.0, 0.0);
    for source in sources {
        value = finite(source.position.mul_add(depth(source.range), value));
        on_scale = finite(source.position.mul_add(depth(source.range.clamp(0.0, 99.0)), on_scale));
    }
    Limited { value, on_scale }.at_most(SATURATION)
}

/// D of MATH §2.4 or a of §7.2, the depth of the LFO's modulation of the pitch
/// or of the amplitude: the [`depth`] of PMD or AMD times the fade w of the
/// LFO's delay, added to the depth of the controllers assigned to the
/// destination ([`controllers`]), and saturating at 255/256, a limit of the
/// instrument that a depth or a range beyond its scale passes (MATH §0.7).
///
/// Each evaluation, d·w + c in a fused step, c the controllers' depth, is
/// within |w|e + u|d·w + c| of its exact value at the depth's error e, beside
/// the error of the controllers' depth; the limit adds as in [`controllers`].
#[must_use]
pub fn modulation(depth_parameter: f64, fade: f64, controllers: Limited) -> Limited {
    let value = finite(depth(depth_parameter).mul_add(fade, controllers.value));
    let on_scale = finite(depth(depth_parameter.clamp(0.0, 99.0)).mul_add(fade, controllers.on_scale));
    Limited { value, on_scale }.at_most(SATURATION)
}

/// b of MATH §7.1, the EG bias of the controllers assigned to it
/// \[AJXS-ROM `MOD_PROCESS_INPUT_SOURCES`\]: the sum of their ranges' depths,
/// saturating at 255/256, less the sum of their depths x·r, and at least 0,
/// both limits of the instrument that a range beyond its scale passes (MATH
/// §0.7). A controller at rest attenuates by its whole range and one at its
/// far end not at all.
///
/// Each evaluation's sums are within Σe + uΣ|R| and Σ|x|e + uΣ|S| of their
/// exact values, R and S the sums after each controller, and their
/// difference within u|R − S| more; each limit adds as in [`controllers`].
#[must_use]
pub fn bias(sources: &[Source]) -> Limited {
    let (mut ranges, mut ranges_on_scale, mut depths, mut depths_on_scale) = (0.0, 0.0, 0.0, 0.0);
    for source in sources {
        let scaled = depth(source.range);
        let nearer = depth(source.range.clamp(0.0, 99.0));
        ranges = finite(ranges + scaled);
        ranges_on_scale = finite(ranges_on_scale + nearer);
        depths = finite(source.position.mul_add(scaled, depths));
        depths_on_scale = finite(source.position.mul_add(nearer, depths_on_scale));
    }
    let ranges = Limited {
        value: ranges,
        on_scale: ranges_on_scale,
    }
    .at_most(SATURATION);
    Limited {
        value: finite(ranges.value - depths),
        on_scale: finite(ranges.on_scale - depths_on_scale),
    }
    .at_least(0.0)
}

/// A table's value in its steps at a value of its scale, from 0 to the last
/// step: linear between the steps (MATH §0.6), exact at them and within one
/// fused rounding between them, the step's part and the steps' difference
/// being exact.
fn table(steps: &[f64], value: f64) -> f64 {
    between(value, |step| steps[index(step)])
}

/// σ(PMS) of MATH §2.4 in octaves: pmstab\[PMS\]/256 at the steps, linear
/// between them (MATH §0.6), and along its first step below 0 and its last
/// above 7 (§0.5), 10/256 and 102/256 octave a step.
///
/// Exact at the steps; within u|σ| between them and below 0, a product's or a
/// fused step's rounding, and within 2u|σ| above 7, where PMS − 7 rounds
/// within u|PMS − 7| as well; the division by 256 exact.
#[must_use]
pub fn pitch_sensitivity(pms: f64) -> f64 {
    let steps = if pms < 0.0 {
        finite(10.0 * pms)
    } else if pms > 7.0 {
        finite(102.0_f64.mul_add(finite(pms - 7.0), 255.0))
    } else {
        table(&PITCH_SENSITIVITY, pms)
    };
    steps / 256.0
}

/// α(AMS) of MATH §7.2: (0, 66, 109, 255)/255 at AMS 0 to 3 \[MSFA
/// `ampmodsenstab`\], provisional (VERIFICATION V14), linear between the steps
/// (MATH §0.6), and along its first step below 0 and its last above 3 (§0.5),
/// so that below 0 the modulation and the bias raise the level.
///
/// The steps in 1/255 as [`pitch_sensitivity`] has them, and their division
/// by 255 within u more: within u|α| at the steps, 2u|α| between them and below
/// 0, and 3u|α| above 3.
#[must_use]
pub fn amplitude_sensitivity(ams: f64) -> f64 {
    let steps = if ams < 0.0 {
        finite(66.0 * ams)
    } else if ams > 3.0 {
        finite(146.0_f64.mul_add(finite(ams - 3.0), 255.0))
    } else {
        table(&AMPLITUDE_SENSITIVITY, ams)
    };
    steps / 255.0
}

/// The LFO's pitch of MATH §2.4 in octaves: σ(PMS)·D·ℓ, for the PMS, the depth
/// D of the pitch modulation (the first evaluation of [`modulation`]'s) and
/// the LFO's wave ℓ (`crate::lfo`).
///
/// [`pitch_sensitivity`]'s error and the two products' roundings: within
/// 2u|p| more than σ's relative error times |p|.
#[must_use]
pub fn lfo_pitch(pms: f64, depth: f64, wave: f64) -> f64 {
    finite(finite(pitch_sensitivity(pms) * depth) * wave)
}

/// r of MATH §7.2, the part of its amplitude that an operator keeps:
/// 1 − b − min(a, 255/256 − b)·(1 − ℓ)/2 for the EG bias b ([`bias`]), the
/// depth a of the amplitude modulation ([`modulation`]) and the LFO's wave ℓ,
/// the room that the bias leaves the depth being the second evaluation's
/// 255/256 − b₂, and what a beyond its scale adds passing it (MATH §0.7): at
/// least 1/256 on the scales.
///
/// The room rounds within u|255/256 − b₂|; the depth so held, where the two
/// evaluations of a differ, within u(|a − a₂| + |e|) more, e the depth; h =
/// (1 − ℓ)/2 within u|h|; 1 − b within u|1 − b|; and the fused step within
/// u|r|: within u(|1 − b| + |h|(|255/256 − b₂| + |a − a₂| + 2|e|) + |r|)
/// absolutely, 5u on the scales.
#[must_use]
pub fn kept(bias: Limited, depth: Limited, wave: f64) -> f64 {
    let room = finite(SATURATION - bias.on_scale);
    let held = depth.on_scale.min(room);
    let modulation = finite(held + finite(depth.value - depth.on_scale));
    finite((-modulation).mul_add((1.0 - wave) / 2.0, 1.0 - bias.value))
}

/// ΔT of MATH §7.2, the level in steps by which the amplitude modulation and
/// the EG bias lower an operator's level: (Aₘₐₓ/8)·α(AMS)·log₂(1/r), for
/// `am.max_steps` Aₘₐₓ, the AMS ([`amplitude_sensitivity`]) and the part r of
/// its amplitude that the operator keeps ([`kept`]); where r is 0 or less, the
/// value that it tends to as r falls to 0: none at AMS 0 or Aₘₐₓ 0, binary64's
/// largest finite number, at which the operator is silent, where α(AMS) lies
/// above 0, and its negative, at which it takes the largest gain (MATH §3.4),
/// below 0.
///
/// Aₘₐₓ/8 exact, its product with α within u, log₂ r within 0.81 ulp, 1.62u
/// relatively, and the last product within u: within 3.62u|ΔT| more than α's
/// relative error times |ΔT|.
#[must_use]
pub fn attenuation(max_steps: f64, ams: f64, kept: f64) -> f64 {
    let scale = finite(max_steps / 8.0 * amplitude_sensitivity(ams));
    if scale == 0.0 {
        0.0
    } else if kept <= 0.0 {
        f64::MAX.copysign(scale)
    } else {
        finite(scale * -log2(kept))
    }
}

/// 2^(m − 1) for a count of bits m from 2 to 53, exactly.
fn centre(bits: u32) -> f64 {
    f64::from_bits(u64::from(1022 + bits) << 52)
}

/// A value below 2⁵³ as a binary64, exactly.
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "an integer below 2⁵³ converts exactly"
)]
fn exactly(value: u64) -> f64 {
    value as f64
}

/// The bend's position x of a value v of its message, of m bits from 2 to 53
/// (MATH §2.4): (v − 2^(m−1))/2^(m−1) below the centre and
/// (v − 2^(m−1))/(2^(m−1) − 1) from it on, so that both ends reach the range,
/// −1 at 0, 0 at the centre and 1 at 2^m − 1; none where v has more bits than
/// m, or m lies outside 2 to 53.
///
/// v − 2^(m−1) is exact, and so is the quotient below the centre; above it
/// the quotient rounds within u|x|.
#[must_use]
pub fn bend_position(value: u64, bits: u32) -> Option<f64> {
    if !(2..=53).contains(&bits) || value >> bits != 0 {
        return None;
    }
    let centre = centre(bits);
    let offset = exactly(value) - centre;
    Some(if offset < 0.0 {
        offset / centre
    } else {
        offset / (centre - 1.0)
    })
}

/// ⌊a·b⌋ of the exact product of two binary64 of 0 or more whose product lies
/// below 2⁵³: the product's high part's floor, one less where the high part is
/// whole and the low part, at most half an ulp of it, lies below 0; a high
/// part that is not whole lies farther than its low part from every integer.
fn floor_of_product(a: f64, b: f64) -> f64 {
    let (high, low) = two_product(a, b);
    let whole = high.floor();
    if low < 0.0 && high - whole == 0.0 {
        whole - 1.0
    } else {
        whole
    }
}

/// The bend k of MATH §2.4 at a whole STEP of 0 or more: x·RANGE at STEP 0,
/// and at a STEP s of 1 or more n = min(N, ⌊|x|(N + ½)⌋) steps of s toward
/// x's side, N = ⌊12/s⌋, whatever the range, and none above 12.
fn bend_at(position: f64, range: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return finite(position * range);
    }
    let most = (12.0 / step).floor();
    let steps = floor_of_product(position.abs(), most + 0.5).min(most);
    (steps * step).copysign(position)
}

/// The bend k of MATH §2.4 in semitones, at a position x of the bend from −1
/// to 1, a RANGE and a STEP: x·RANGE at STEP 0, the default; at a STEP s of 1
/// or more, whatever the range, n = min(N, ⌊|x|(N + ½)⌋) steps of s semitones
/// toward x's side, N = ⌊12/s⌋, a staircase by design, which MATH §0.6 does
/// not smooth, and none above 12; linear between whole STEPs (§0.6), and below
/// 0 along its first step, k(x, 0) + s·(k(x, 1) − k(x, 0)).
///
/// The staircase's floor is taken of the exact product, so that it is exact
/// at whole STEPs of 1 or more; x·RANGE within u|k|. Between whole STEPs n and
/// n + 1, and below 0, t the STEP's part or the STEP: within
/// u(|1 − t|·|k(n)| + |t|·|k(n + 1) − k(n)| + |k|), the first term only where
/// n is 0, the difference and the fused step each rounding.
#[must_use]
pub fn bend(position: f64, range: f64, step: f64) -> f64 {
    if step < 0.0 {
        return lerp(step, bend_at(position, range, 0.0), bend_at(position, range, 1.0));
    }
    between(step, |whole| bend_at(position, range, whole))
}
