// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The operator envelope (docs/MATH.md §4): the laws of its rates and of its
//! segments, here, and the envelope itself, which runs its segments by them,
//! in [`envelope`]. The laws: the rate q of a segment from its rate parameter,
//! the rate scaling and the key of the rates; the speed ρ(q) at which its
//! level moves; in the progress that a segment has made, its attack, the jump
//! that starts it, the progress at which it reaches a target, and its wait;
//! the DX7's floor, which times the segments; and the level below which an
//! operator is silent.
//!
//! A level is a total level T in the DX7's steps (MATH §3.3), and a speed is in
//! steps a second. The progress of a segment is ∫ρ dt from its start, in
//! steps, ρt where the speed holds still, which the laws of a segment take in
//! place of the time (MATH §4.7): a decay falls by its progress, T₀ − Φ, and
//! an attack and a wait are laws of it below. Each function is a law of MATH
//! §4 as written, its parameters and inputs any finite binary64, between the
//! steps interpolated as MATH §0.6 says, beyond the scales continued as §0.5
//! says and past the instrument's limits as §0.7 says, and documents its error
//! against the law's exact value at its binary64 inputs; tests/eg.rs measures
//! each against exact values (`scripts/eg_references.py`).

use keptone_math::{exp2, exp2m1, log2p1};

use crate::binary64::{finite, scaled, split};
use crate::level::FULL_SCALE;
use crate::steps::between;

pub mod envelope;

/// ⌊41R/64⌋ + RS(k) of MATH §4.1 at a whole rate R, sensitivity KRS and key k,
/// the rate before its bound: RS = ⌊KRS·clamp(⌊k/3⌋ − 7, 0, 31)/8⌋
/// \[LEVIEN-EG, MSFA `ScaleRate`\], whose group of keys holds its bounds
/// wherever the key lies, as only keys and positions reach them (MATH §0.7).
fn unbounded(rate: f64, sensitivity: f64, key: f64) -> f64 {
    let group = ((key / 3.0).floor() - 7.0).clamp(0.0, 31.0);
    let scaling = (finite(sensitivity * group) / 8.0).floor();
    finite((finite(41.0 * rate) / 64.0).floor() + scaling)
}

/// The rate q of MATH §4.1 of a segment, from its rate R, 0 to 99 on its scale,
/// the operator's rate scaling KRS, 0 to 7, and the key of the rates k (§2.6):
/// min(63, ⌊41R/64⌋ + RS(k)) at whole rates, sensitivities and keys, RS =
/// ⌊KRS·clamp(⌊k/3⌋ − 7, 0, 31)/8⌋ \[LEVIEN-EG, MSFA `ScaleRate`\], a law that
/// stays provisional (VERIFICATION V20), and trilinear between them after the
/// bound (MATH §0.6). The bound of 63 is a limit of the instrument: a rate or a
/// sensitivity beyond its scale adds past it the difference that it makes to
/// ⌊41R/64⌋ + RS, each interpolated (MATH §0.7). The key is an input, and
/// TRANSPOSE and the note shift, which move it, are positions.
///
/// Its values at whole inputs are exact where |R| and |KRS| lie below 2⁴⁷ and
/// |k| below 2⁵³: 41R, ⌊k/3⌋, KRS times the group and the sum round nowhere.
/// Beyond, 41R and KRS times the group round within u of themselves, and a
/// floor may lie 1 from the exact one. Of such values at most M in magnitude,
/// the three interpolations add 4uM each, as a law's between the steps does:
/// within 12uM of q on the scales. Beyond them q is the sum of three such
/// interpolations, at the inputs and at the nearer ends with and without the
/// bound, and its two sums round within 5uM: within 41uM.
#[must_use]
pub fn rate(rate: f64, sensitivity: f64, key: f64) -> f64 {
    let at = |rate: f64, sensitivity: f64, bounded: bool| {
        between(rate, |rate| {
            between(sensitivity, |sensitivity| {
                between(key, |key| {
                    let value = unbounded(rate, sensitivity, key);
                    if bounded { value.min(63.0) } else { value }
                })
            })
        })
    };
    let (rate_on_scale, sensitivity_on_scale) = (rate.clamp(0.0, 99.0), sensitivity.clamp(0.0, 7.0));
    let on_scale = at(rate_on_scale, sensitivity_on_scale, true);
    if (0.0..=99.0).contains(&rate) && (0.0..=7.0).contains(&sensitivity) {
        on_scale
    } else {
        let passed = finite(at(rate, sensitivity, false) - at(rate_on_scale, sensitivity_on_scale, false));
        finite(on_scale + passed)
    }
}

/// How the speed of an envelope follows its rate: `eg.rate_law` (MATH §4.1,
/// §17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RateLaw {
    /// C·2^(q/4) linear between multiples of 4, as the DX7's: the default.
    #[default]
    Dx7,
    /// C·2^(q/4).
    Exponential,
}

/// The speed ρ(q) of MATH §4.1, in steps a second, of a segment of rate q:
/// C·2^⌊q/4⌋·(1 + (q − 4⌊q/4⌋)/4) under [`RateLaw::Dx7`], which interpolates
/// C·2^(q/4) linearly between multiples of 4, and C·2^(q/4) under
/// [`RateLaw::Exponential`], C = `eg.clock_hz`/4096 \[LEVIEN-EG\]: one step in
/// 4096 samples of the clock at q = 0. `clock_hz` lies above 0, as the
/// setting's bounds keep it (MATH §17). A speed beyond binary64's largest
/// finite number is taken at it (MATH §0.5), and one below its least is 0, at
/// which a segment never moves.
///
/// Under Dx7, q − 4⌊q/4⌋ is exact but in (−4, 0), where it rounds within 2u,
/// 1 + its quarter and C times that round once each, and the scaling by
/// 2^⌊q/4⌋ is exact: within 2.5u relatively. Under Exponential, q/4 splits
/// exactly, 2 to its part is within 0.93 ulp of itself, 1.86u, and C times that
/// rounds once: within 2.86u. Below the normal range, within two steps of the
/// subnormals more.
#[must_use]
pub fn speed(rate: f64, clock_hz: f64, law: RateLaw) -> f64 {
    let base = clock_hz / 4096.0;
    match law {
        RateLaw::Dx7 => {
            let quarter = (rate / 4.0).floor();
            let part = rate - 4.0 * quarter;
            scaled(base * (1.0 + part / 4.0), quarter)
        }
        RateLaw::Exponential => {
            let (whole, part) = split(rate / 4.0);
            scaled(base * exp2(part), whole)
        }
    }
}

/// The level of the jump of MATH §4.3, 1700 steps above the DX7's floor of 16,
/// to which a rising segment's level below it jumps \[LEVIEN-EG; MSFA
/// `jumptarget`\].
pub const JUMP: f64 = 1716.0;

/// The attack's knee, 256 steps above [`FULL_SCALE`], where its multiplier
/// comes down to 1, and from which it rises at its speed (MATH §4.3).
pub const KNEE: f64 = FULL_SCALE + 256.0;

/// The asymptote T* of the attack's law below its knee, 512 steps above full
/// scale.
const ASYMPTOTE: f64 = FULL_SCALE + 512.0;

/// 256 ln 2, correctly rounded: the progress over which an attack below its
/// knee halves its distance from its asymptote.
const HALVING: f64 = f64::from_bits(0x4066_2e42_fefa_39ef);

/// 1/(256 ln 2), correctly rounded.
const PER_HALVING: f64 = f64::from_bits(0x3f77_1547_652b_82fe);

/// The level from which a segment that rises starts (MATH §4.3): a start below
/// [`JUMP`] jumps to it, or to the target where that lies below it, which it
/// then takes at once. A start at or above [`JUMP`], or not below its target, is
/// kept; so is every start under `eg.attack_jump = false`, which does not call
/// this. Exact.
#[must_use]
pub fn jump(start: f64, target: f64) -> f64 {
    if start < JUMP && start < target {
        target.min(JUMP)
    } else {
        start
    }
}

/// The progress from a start T₀ below the knee to it, 256 ln((T* − T₀)/256) =
/// 256 ln 2·log₂(1 + (KNEE − T₀)/256): KNEE − T₀ within u of itself, its 256th
/// exact, log₂p1 within 0.81 ulp, 1.62u, of itself and its argument's error
/// passed on at most as it is, and the constant and the product within u
/// each: within 4.62u relatively.
fn progress_to_knee(start: f64) -> f64 {
    HALVING * log2p1(finite(KNEE - start) / 256.0)
}

/// The level of an attack of MATH §4.3 from a start T₀ after a progress Φ of 0
/// or more: below the knee T* − (T* − T₀)e^(−Φ/256), the solution of dT/dΦ =
/// 2 + (3840 − T)/256, and from the knee on rising as Φ does, the multiplier
/// being 1 there: the continuous law that the DX7's integer multiplier steps
/// in bands of 256 steps (§15). The level goes on past every target: a segment
/// ends at its own ([`attack_progress`]).
///
/// Below the knee, T = T₀ + (T₀ − T*)·(2^(−Φ/(256 ln 2)) − 1) by exp2m1, which
/// keeps its accuracy where Φ is small: the exponent within 2u of itself, so
/// that 2^z − 1, within 0.62 ulp of itself, is within 3.24u, T₀ − T* within u,
/// and the fused step within u of the result: within u|T| + 4.24u|T − T₀|.
/// From the knee on, KNEE + (Φ − Φₖ), Φₖ the progress to the knee within
/// 4.62u of itself: within u(|T| + |Φ − Φₖ|) + 4.62uΦₖ. The law's slope is
/// 1 on either side of the knee, so that taking one side for the other near it
/// costs a term of the second order. From a start at or above the knee, T₀ + Φ
/// within u|T|.
#[must_use]
pub fn attack(start: f64, progress: f64) -> f64 {
    if start >= KNEE {
        return finite(start + progress);
    }
    let to_knee = progress_to_knee(start);
    if progress >= to_knee {
        finite(KNEE + (progress - to_knee))
    } else {
        (start - ASYMPTOTE).mul_add(exp2m1(-progress * PER_HALVING), start)
    }
}

/// The progress Φ* of MATH §4.3 at which an attack from T₀ reaches a target, the
/// time t* = Φ*/ρ where the speed holds still: none where the target is not
/// above T₀; up to the knee 256 ln((T* − T₀)/(T* − Tₜ)), computed as
/// 256 ln 2·log₂p1((Tₜ − T₀)/(T* − Tₜ)) (§12.4), which keeps its
/// accuracy where the start is near the target; beyond it the progress to the
/// knee plus the target's distance above it; and from a start at or above the
/// knee the distance itself.
///
/// Up to the knee: Tₜ − T₀, T* − Tₜ and their quotient within u of
/// themselves each, log₂p1 within 0.81 ulp, 1.62u, of itself and its argument's
/// error passed on at most as it is, and the constant and the product within u
/// each: within 6.62u of Φ* relatively. Beyond it, the progress to the knee
/// within 4.62u of itself, the distance and the sum within u each: within
/// u(|Φ*| + |Tₜ − KNEE|) + 4.62uΦₖ. From the knee, within u|Φ*|.
#[must_use]
pub fn attack_progress(start: f64, target: f64) -> f64 {
    if target <= start {
        0.0
    } else if start >= KNEE {
        finite(target - start)
    } else if target <= KNEE {
        HALVING * log2p1(finite(target - start) / (ASYMPTOTE - target))
    } else {
        finite(progress_to_knee(start) + (target - KNEE))
    }
}

/// W of MATH §4.4: a segment that does not move waits as long as it would take
/// to move 480 steps at its speed \[MSFA `ACCURATE_ENVELOPE`\], a value that no
/// independent source settles (VERIFICATION V8: provisional).
pub const WAIT: f64 = 480.0;

/// The progress that a segment waits (MATH §4.4), counted from its start as its
/// move is: W(1 − m) where it moves by m steps, less than one, as the DX7 takes
/// its levels (§4.6), and none where it moves by a step or more; a twentieth of
/// that in segment 0, `first`. `eg.static_segments = none` waits nowhere, and
/// does not call this.
///
/// 1 − m is exact from m = 1/2 up (Sterbenz) and within u of itself below, and
/// the product and the twentieth round once each: within 3u relatively.
#[must_use]
pub fn wait(first: bool, moved: f64) -> f64 {
    let wait = WAIT * (1.0 - moved.abs().min(1.0));
    if first { wait / 20.0 } else { wait }
}

/// The DX7's floor of MATH §4.6, 16 steps, 90 dB below full scale
/// \[MSFA `Env::advance`, LEVIEN-EG\]: the DX7 holds an operator's level no
/// lower, and its segments start, wait and end where they would with their
/// levels and targets taken no lower. Keptone keeps that timing and takes the
/// level on below it ([`envelope`]).
pub const FLOOR: f64 = 16.0;

/// log₂ 10/20, correctly rounded: the octaves of gain in a decibel.
const OCTAVES_PER_DECIBEL: f64 = f64::from_bits(0x3fc5_42a5_a12e_1c5b);

/// The level of MATH §4.6 below which an operator has gain 0, until a segment
/// takes its level above it: for `eg.end_db`, D decibels of gain relative to
/// g = 1, 256(14 + D·log₂ 10/20), −4920.136 at the default, −200 dB.
///
/// The constant within u of itself, the fused step within u of its result and
/// the scaling by 256 exact: within u(256|D·log₂ 10/20| + |T|) of the level T.
#[must_use]
pub fn end_level(end_db: f64) -> f64 {
    256.0 * end_db.mul_add(OCTAVES_PER_DECIBEL, 14.0)
}
