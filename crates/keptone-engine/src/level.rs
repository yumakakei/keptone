// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Levels (docs/MATH.md §3): the curve that maps a level of 0–99 to the DX7's
//! level steps, an operator's offset from its output level, its keyboard level
//! scaling and the key's velocity, the envelope's part of an operator's total
//! level, the total level, and its gain.
//!
//! A total level T is in the DX7's steps of 20·log₁₀2/256 dB, 3840 at full
//! scale, and its gain is 2^(T/256 − 14), 2 at full scale. Each function is a
//! law of MATH §3 as written, its parameters and inputs any finite binary64,
//! between the steps interpolated as MATH §0.6 says, beyond the scales
//! continued as §0.5 says and past the instrument's limits as §0.7 says, and
//! documents its error against the law's exact value at its binary64 inputs;
//! tests/level.rs measures each against exact values
//! (`scripts/level_references.py`).

use keptone_math::{exp2, log2};
use keptone_model::voice::Curve;

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

/// below + part·(above − below), saturated (MATH §0.5): the linear step
/// between two values of a law at whole inputs (MATH §0.6).
fn lerp(part: f64, below: f64, above: f64) -> f64 {
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
fn between(x: f64, at: impl Fn(f64) -> f64) -> f64 {
    let whole = x.floor();
    let below = at(whole);
    let part = x - whole;
    if part > 0.0 {
        lerp(part, below, at(whole + 1.0))
    } else {
        below
    }
}

/// G(x) of MATH §3.2 at a whole key x, ⌊(x − 1)/3⌋. Exact where |x| < 2⁵³:
/// x − 1 is exact, and (x − 1)/3, which lies 1/3 from an integer at least
/// where it is not one, rounds within u of itself and so never across one.
fn group_at(key: f64) -> f64 {
    ((key - 1.0) / 3.0).floor()
}

/// G of MATH §3.2: the DX7's group of three keys of a key on its key code,
/// ⌊(x − 1)/3⌋ at a whole key \[AJXS-ROM `TABLE_MIDI_KEY_TO_LOG_F`\], groups
/// fixed on the keyboard, and linear between keys (MATH §0.6): it rises by 1
/// from each key 3j to the next and is flat elsewhere.
///
/// Exact at a whole key below 2⁵³ in magnitude. Between keys, which lie below
/// 2⁵² in magnitude, x less its floor is exact but in (−1, 0), where G does not
/// rise, and a rise rounds once: within u|G|. Beyond 2⁵³, where every key is
/// whole and u|x| is 1 or more, x − 1 and its third round within u|x|/3 each
/// and the floor moves by 1 at most: within 2u|x|.
#[must_use]
pub fn group(key: f64) -> f64 {
    let whole = key.floor();
    let below = group_at(whole);
    let part = key - whole;
    if part > 0.0 && group_at(whole + 1.0) > below {
        part + below
    } else {
        below
    }
}

/// ξ of MATH §3.2: the distance of the group of the key of the level's
/// scaling (§2.6) from the break point's, G(k) + 1 − G(BP + 20), at which the
/// right curve applies from 1 up and the left from 0 down. The break point is
/// a position on the keyboard, as the key is (MATH §0.7), and BP + 20 is never
/// beyond binary64's largest finite number, to which it rounds.
///
/// BP + 20, G(k) + 1 and their difference round within u of themselves each,
/// and G's slope is 0 or 1: within u(|BP + 20| + |G(k) + 1| + |ξ|) of ξ, beside
/// the errors of the two groups that [`group`] documents.
#[must_use]
pub fn distance(key: f64, break_point: f64) -> f64 {
    (group(key) + 1.0) - group(break_point + 20.0)
}

/// C(u) of MATH §3.2 at a whole distance u ≥ 0 from the break point's group:
/// Keptone's laws for the firmware's two curves \[AJXS-ROM
/// `TABLE_KBD_SCALING_CURVE_LIN`, `TABLE_KBD_SCALING_CURVE_EXP`\], 8u for the
/// linear and, for the exponential, u up to 9 and 2.094·2^(u/4) from 10 on,
/// each at most 255, which they reach at 32 and at 28. The bound is a limit of
/// the instrument that only the key and the break point reach, and so holds
/// wherever they lie (MATH §0.7). A curve's sign is the scaling's
/// ([`keyboard_scaling`]): here −LIN is LIN and −EXP is EXP.
///
/// Exact but for the exponential from 10 on, where fl(2.094), within 0.96u of
/// 2.094, times 2^(u/4), within exp2's 0.93 ulp, 1.86u of itself, and exact at
/// multiples of 4, rounds once: within 3.82u of C(u) relatively.
#[must_use]
pub fn scaling_curve(shape: Curve, distance: f64) -> f64 {
    match shape {
        Curve::NegativeLinear | Curve::PositiveLinear => finite(8.0 * distance).min(255.0),
        Curve::NegativeExponential | Curve::PositiveExponential => {
            if distance < 10.0 {
                distance
            } else {
                (2.094 * exp2(distance.min(28.0) / 4.0)).min(255.0)
            }
        }
    }
}

/// One side of an operator's keyboard level scaling (MATH §3.2): its curve and
/// its depth, 0 to 99 on its scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Side {
    /// The curve, −LIN, −EXP, +EXP or +LIN.
    pub curve: Curve,
    /// The depth.
    pub depth: f64,
}

/// An operator's keyboard level scaling ±c of MATH §3.2, which raises its level
/// λ where a positive curve applies and lowers it where a negative one does, in
/// the steps of λ, as MATH §0.7 evaluates it twice: with the depths as they
/// are, and with each depth beyond its scale at the scale's nearer end, the
/// instrument's, where c takes its bound of 127.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scaling {
    /// ±c with the depths as they are, the first evaluation: what a depth
    /// beyond its scale adds passes c's bound.
    pub value: f64,
    /// ±c with each depth on its scale, the second evaluation.
    pub on_scale: f64,
}

/// ⌊C·⌊165D/64⌋/256⌋ of MATH §3.2 at a whole depth D for a curve's value C: the
/// scaling before its bound, the depth scaled as the LFO's depths are
/// \[AJXS-ROM `PATCH_ACTIVATE_SCALE_VALUE`\].
fn unbounded(curve_value: f64, depth: f64) -> f64 {
    let scaled_depth = (finite(165.0 * depth) / 64.0).floor();
    (finite(curve_value * scaled_depth) / 256.0).floor()
}

/// ±c at a whole distance n, as [`keyboard_scaling`] evaluates it: the right
/// side's at u = n where n ≥ 1 and the left's at u = −n where n ≤ 0.
fn signed_scaling(distance: f64, left: Side, right: Side) -> Scaling {
    let (side, u) = if distance >= 1.0 {
        (right, distance)
    } else {
        (left, -distance)
    };
    let curve_value = scaling_curve(side.curve, u);
    let at = |depth: f64| unbounded(curve_value, depth);
    let nearer_end = side.depth.clamp(0.0, 99.0);
    let on_scale = between(nearer_end, |depth| at(depth).min(127.0));
    let value = if (0.0..=99.0).contains(&side.depth) {
        on_scale
    } else {
        // The nearer end is a whole depth, where c's bound takes `taken` from
        // the scaling, 0 or less; what the depth adds beyond it passes the
        // bound (MATH §0.7).
        let taken = on_scale - at(nearer_end);
        between(side.depth, |depth| finite(at(depth) + taken))
    };
    match side.curve {
        Curve::PositiveExponential | Curve::PositiveLinear => Scaling { value, on_scale },
        Curve::NegativeLinear | Curve::NegativeExponential => Scaling {
            value: -value,
            on_scale: -on_scale,
        },
    }
}

/// ±c of MATH §3.2 at a distance ξ ([`distance`]) for the curves and the depths
/// of the scaling's two sides \[AJXS-ROM
/// `PATCH_ACTIVATE_OPERATOR_KBD_SCALING_LEVEL`\]: the right side's at u = ξ
/// where ξ ≥ 1 and the left's at u = −ξ where ξ ≤ 0, c = min(127,
/// ⌊C(u)·⌊165D/64⌋/256⌋) at whole distances and depths, raising the level for
/// +LIN and +EXP and lowering it for −LIN and −EXP, and linear between whole
/// distances and depths after the bound (MATH §0.6). A depth beyond its scale
/// adds past the bound ⌊C(u)·⌊165D/64⌋/256⌋ at the depth less its value at the
/// scale's nearer end, interpolated in the depth (MATH §0.7).
///
/// Its values at whole distances and depths, below 2⁵³ in magnitude, are exact
/// where ⌊165D/64⌋ is, as it is where |165D| ≤ 2⁵³, and C(u)·⌊165D/64⌋/256,
/// which [`scaling_curve`]'s 3.82u and the product's rounding keep within 4.82u
/// of itself, is exact or lies farther than that from every integer: so at
/// every whole distance and depth of the scales. Elsewhere a floor is within 1
/// of the exact one. Of such values at most M in magnitude, the interpolations
/// in the depth and in the distance add 4uM each, the difference of two values
/// rounding within 2uM, the fused step within uM and the part of a depth or a
/// distance in (−1, 0), alone inexact, within u/2; and the sum of a value at a
/// depth beyond its scale and what the bound takes rounds within uM: each
/// evaluation is within 9uM of its exact value, beside the floors' errors.
#[must_use]
pub fn keyboard_scaling(distance: f64, left: Side, right: Side) -> Scaling {
    let whole = distance.floor();
    let below = signed_scaling(whole, left, right);
    let part = distance - whole;
    if part > 0.0 {
        let above = signed_scaling(whole + 1.0, left, right);
        Scaling {
            value: lerp(part, below.value, above.value),
            on_scale: lerp(part, below.on_scale, above.on_scale),
        }
    } else {
        below
    }
}

/// The knots of Keptone's velocity curve B(v) of MATH §3.2, as (v, B): linear
/// between them, 0 from 116 on and along its first step below 0.
const VELOCITY_CURVE: [(f64, f64); 5] = [(0.0, 128.0), (28.0, 104.0), (56.0, 60.0), (88.0, 30.0), (116.0, 0.0)];

/// ⌊K·B(v)/8⌋ at a whole sensitivity K and a whole velocity v. K·B(v) times the
/// length L of v's step of B is the integer N = K(B₀L + (v − v₀)(B₁ − B₀)), and
/// N/(8L), an integer or 1/(8L) from one at least, rounds within u of itself:
/// the floor is exact where N is and |N| < 2⁵³, so where |K|(3584 + 24|v|) <
/// 2⁵³. Elsewhere N/(8L) is within 3u of itself, and its floor within 1 of the
/// exact one.
fn velocity_floor(sensitivity: f64, velocity: f64) -> f64 {
    if velocity >= 116.0 {
        return 0.0;
    }
    let knot = (1..4)
        .rev()
        .find(|&knot| velocity >= VELOCITY_CURVE[knot].0)
        .unwrap_or(0);
    let (from, level) = VELOCITY_CURVE[knot];
    let (to, next) = VELOCITY_CURVE[knot + 1];
    let length = to - from;
    let numerator = finite(sensitivity * finite((velocity - from).mul_add(next - level, level * length)));
    (numerator / (8.0 * length)).floor()
}

/// V at a whole sensitivity and a whole velocity, 16(2K − ⌊K·B(v)/8⌋): exact
/// where [`velocity_floor`] is and |V| < 2⁵³, and within u|V| beside 16 times
/// the floor's error elsewhere.
fn velocity_at(sensitivity: f64, velocity: f64) -> f64 {
    finite(16.0 * finite(finite(2.0 * sensitivity) - velocity_floor(sensitivity, velocity)))
}

/// V of MATH §3.2: the part of an operator's offset that a key's velocity v
/// gives at its velocity sensitivity KVS, 16(2·KVS − ⌊KVS·B(v)/8⌋) at whole
/// sensitivities and velocities \[AJXS-ROM `PATCH_ACTIVATE_OPERATOR_KBD_VEL_SENS`,
/// `VOICE_ADD_LOAD_OPERATOR_DATA_TO_EGS`\], with B Keptone's velocity curve, 128
/// at the softest and 0 from 116 on, and bilinear between them (MATH §0.6): so
/// −224·KVS at v = 0 and 32·KVS from 116 on.
///
/// Its values at the whole sensitivities K and velocities v around (KVS, v), at
/// most M in magnitude, are exact where |K|(3584 + 24|v|) and |V| lie below 2⁵³
/// at each, and the two interpolations add 4uM each, as [`keyboard_scaling`]'s
/// do: within 8uM of V there, and within 9uM beside 16 times the floors' errors
/// elsewhere.
#[must_use]
pub fn velocity_offset(sensitivity: f64, velocity: f64) -> f64 {
    between(sensitivity, |sensitivity| {
        between(velocity, |velocity| velocity_at(sensitivity, velocity))
    })
}

/// The highest offset, 4240, at which the firmware's least attenuation, 4,
/// holds a loud note (MATH §3.2): the bound of a limit of the instrument.
pub const HIGHEST_OFFSET: f64 = 4240.0;

/// The offset O of MATH §3.2 in an operator's total level (§3.3), from its
/// output level OL, its keyboard level scaling σ = ±c ([`keyboard_scaling`])
/// and the part V ([`velocity_offset`]) that a key's velocity v gives at its
/// velocity sensitivity KVS \[AJXS-ROM `VOICE_ADD_LOAD_OPERATOR_DATA_TO_EGS`\]:
/// min(4240, 32λ + V) where the operator's level λ = s(OL) + σ, taken from 0 to
/// 127, is 1 or more; O₁ + 256 log₂ λ where λ lies between 0 and 1, O₁ the
/// offset at λ = 1, so that the gain falls linearly to silence at 0; and none
/// at or below 0, where the operator is silent.
///
/// The bounds of λ and O are limits of the instrument, which MATH §0.7 makes
/// the second evaluation's, OL, the depths and KVS beyond their scales taken
/// at their nearer ends: λ₂ = clamp(y₂, 0, 127) of y₂ = s(OL₂) + σ₂, and λ =
/// λ₂ + (y − y₂) of y = s(OL) + σ; z₂ = 32λ₂ + V₂, and O = min(4240, z₂) + (z −
/// z₂) of z = 32λ + V. On the scales the two evaluations are one, and at whole
/// values O is the DX7's.
///
/// First order in u, ε(x) the error of x: s(OL) and s(OL₂) are within u of
/// themselves ([`curve`]), and y and y₂ within u|y| more; ε(λ₂) ≤ ε(y₂), and
/// ε(λ) ≤ ε(λ₂) + ε(y) + ε(y₂) + u(|y − y₂| + |λ|) where the evaluations
/// differ, and ε(λ₂) where they agree. With ε(V) and ε(V₂) from
/// [`velocity_offset`], ε(z₂) ≤ 32ε(λ₂) + ε(V₂) + u|z₂|, which min(4240, z₂) −
/// z₂ passes on where z₂ passes 4240, Sterbenz's lemma making the difference
/// exact. From λ = 1 up, ε(O) ≤ 32ε(λ) + ε(V) + u(|z| + |O|) beside that;
/// below 1, ε(O₁) ≤ ε(V) + u(|32 + V| + |O₁|) beside it, O₁ = (32 + V) +
/// (min(4240, z₂) − z₂), and ε(O) ≤ ε(O₁) + (256/ln 2)·ε(λ)/λ + 415u|log₂ λ| +
/// u|O|, log₂'s 1.62u of itself times 256.
#[must_use]
pub fn offset(output_level: f64, scaling: Scaling, sensitivity: f64, velocity: f64) -> Option<f64> {
    // The second evaluation, the instrument's, which takes the limits.
    let level_on_scale = curve(output_level.clamp(0.0, 99.0)) + scaling.on_scale;
    let lambda_on_scale = level_on_scale.clamp(0.0, 127.0);
    // The first, in which what lies beyond the scales passes them.
    let level = finite(curve(output_level) + scaling.value);
    let lambda = finite(lambda_on_scale + finite(level - level_on_scale));
    if lambda <= 0.0 {
        return None;
    }
    let velocity_on_scale = velocity_offset(sensitivity.clamp(0.0, 7.0), velocity);
    let taken = (HIGHEST_OFFSET - 32.0_f64.mul_add(lambda_on_scale, velocity_on_scale)).min(0.0);
    let velocity = if (0.0..=7.0).contains(&sensitivity) {
        velocity_on_scale
    } else {
        velocity_offset(sensitivity, velocity)
    };
    Some(if lambda >= 1.0 {
        finite(finite(32.0_f64.mul_add(lambda, velocity)) + taken)
    } else {
        256.0_f64.mul_add(log2(lambda), finite(32.0 + velocity) + taken)
    })
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
