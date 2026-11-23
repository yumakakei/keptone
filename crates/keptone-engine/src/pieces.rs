// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The pieces of an engine sample (docs/MATH.md §2.5): the signals that move a
//! voice's pitch within it, its pitch envelope, its portamento and its LFO,
//! split at every instant where one of them has a corner or a jump, and the
//! parts on which each operator's phase increment is integrated: in closed form
//! where the pitch moves linearly, and by the 3-point Gauss–Legendre rule on as
//! many equal parts as keep its truncation below 2⁻⁶⁰ of each part's integral
//! elsewhere. A sample whose pieces would take more parts than
//! `osc.max_pieces`, or whose LFO has more corners and edges in it than that,
//! is integrated instead by the rule on that many equal parts of the sample,
//! each signal taken at the rule's instants by its own law.
//!
//! An operator's pitch is c + λ(τ), τ the samples since the piece's start: c
//! is the operator's own and holds still within a sample, the key's pitch with
//! the detune, the bend, the per-note pitch and the master tune in the ratio
//! mode (MATH §2.2, §2.4), the fixed pitch with the bend and the per-note pitch
//! in the fixed mode (§2.3), all of which change only at the events of a
//! sample's instant (§0.3); λ is the voice's, the pitch envelope, the
//! portamento and the LFO's pitch in the ratio mode, and the LFO's pitch alone
//! in the fixed mode. The bend, the per-note pitch and the portamento move the
//! pitch in octaves; under 12-TET, the only tuning that milestone M3 computes,
//! a glide or a glissando in keys moves it by a twelfth of an octave a key, so
//! that `tuning.modulation`'s two modes are the same (§2.1).
//!
//! The pieces' signals are those of `crate::peg`, `crate::portamento`,
//! `crate::lfo` and `crate::control`, each within the error that its laws
//! document; the forms here document theirs, and tests/pieces.rs measures the
//! increments of every kind of piece against mpmath's quadrature of its exact
//! law (`scripts/pieces_references.py`) and the walk's parts against the
//! signals' own laws.

use core::f64::consts::{LN_2, LOG2_E, TAU};

use keptone_math::eft::two_product;
use keptone_math::{exp2, exp2m1, log2, sin2pi};
use keptone_model::voice::Wave;

use crate::binary64::finite;
use crate::control::{Limited, modulation, saturating_fade};
use crate::lfo::{Fading, Lfo, shape};
use crate::peg::{self, PitchEnvelope};
use crate::pitch::phase::{gauss_increment, gauss_instants, linear_increment};
use crate::portamento::{Glide, Glissando, Unit};
use crate::time::Instant;

/// 2016000·2⁻⁶⁰: the rule's truncation on a part of length h is
/// h⁷|f⁽⁶⁾|/2016000, and its parts are short enough where h⁶B₆e^(a₁h) is at
/// most this (MATH §2.5).
const TRUNCATION: f64 = 2_016_000.0 / 1_152_921_504_606_846_976.0;

/// A share below 1 of the part's length that the rule's sizing takes, so that
/// the roundings of its bounds, a few u of them, never lengthen a part past
/// its bound.
const SAFETY: f64 = 0.999;

/// 2⁵³ turns a sample, from which the LFO's phase stands still (MATH §6.1).
const STILL: f64 = 9_007_199_254_740_992.0;

/// The LFO's wave on a piece (MATH §6.2), τ the samples since its start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// ℓ holding still: the square, the sample and hold, and a wave whose phase
    /// stands still.
    Constant(f64),
    /// ℓ₀ + ℓ′τ, the triangle and the saws between their corners and edges.
    Linear {
        /// ℓ at the piece's start.
        value: f64,
        /// Its slope a sample.
        slope: f64,
    },
    /// −sin 2π(φ₀ + sτ).
    Sine {
        /// The phase φ₀ at the piece's start, in turns.
        turns: f64,
        /// The phase's advance s a sample.
        step: f64,
    },
}

/// The voice's moving part of the pitch on a piece of a sample, in octaves, τ
/// the samples since its start: λ(τ) = P + bτ + G(τ) + A(τ)ℓ(τ) in the ratio
/// mode and A(τ)ℓ(τ) in the fixed mode, P the pitch envelope's level with the
/// glissando's key heard, b the pitch envelope's slope, G the glide's offset
/// from its target, d + (d + sgn(d)/4)·(e^(−ρτ) − 1) from its offset d at the
/// start (MATH §7.3), A = σ(PMS)·D the depth of the LFO's pitch, linear in τ,
/// and ℓ the LFO's wave (§2.4). A piece with a glide ends at or before the
/// glide reaches its target, ln(1 + 4|d|)/ρ samples on, as the walk ends it
/// there: its law past that instant is not the glide's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    /// Its length in samples.
    pub length: f64,
    /// P at its start.
    pub pitch: f64,
    /// b, in octaves a sample.
    pub slope: f64,
    /// The glide's offset d from its target at its start, in octaves; 0 where
    /// there is no glide.
    pub glide: f64,
    /// ρ = 4κ/fₑ, the rate of the glide's law a sample.
    pub rate: f64,
    /// A at its start, in octaves.
    pub depth: f64,
    /// A's slope a sample.
    pub depth_slope: f64,
    /// ℓ.
    pub shape: Shape,
}

/// A part of a sample on which an operator's increment is integrated (MATH
/// §2.5): in closed form where λ is linear on it, and by the rule at its three
/// instants elsewhere, for the ratio mode and the fixed mode.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Part {
    /// λ linear on the part.
    Linear {
        /// Its length in samples.
        length: f64,
        /// λ at its start in the ratio mode.
        pitch: f64,
        /// λ's slope a sample in the ratio mode.
        slope: f64,
        /// λ at its start in the fixed mode.
        fixed_pitch: f64,
        /// λ's slope a sample in the fixed mode.
        fixed_slope: f64,
    },
    /// λ at the rule's three instants of the part.
    Rule {
        /// Its length in samples.
        length: f64,
        /// λ at the instants in the ratio mode.
        pitches: [f64; 3],
        /// λ at the instants in the fixed mode.
        fixed: [f64; 3],
    },
}

impl Part {
    /// An operator's increment over the part, in turns, for its frequency
    /// ratio over the engine's rate, r/fₑ, its pitch's constant c and its mode:
    /// r/fₑ·∫2^(c + λ) dτ, the pitch held at 1023 octaves (MATH §2.2). c + λ
    /// rounds within u/2 of itself, which moves the increment by ln 2·u/2·|c +
    /// λ| relatively, beside the closed form's or the rule's errors
    /// (`crate::pitch::phase`).
    #[must_use]
    pub fn increment(&self, ratio: f64, constant: f64, fixed: bool) -> f64 {
        match *self {
            Self::Linear {
                length,
                pitch,
                slope,
                fixed_pitch,
                fixed_slope,
            } => {
                let (pitch, slope) = if fixed {
                    (fixed_pitch, fixed_slope)
                } else {
                    (pitch, slope)
                };
                linear_increment(ratio, constant + pitch, slope, length)
            }
            Self::Rule {
                length,
                pitches,
                fixed: fixed_pitches,
            } => {
                let pitches = if fixed { fixed_pitches } else { pitches };
                gauss_increment(ratio, length, pitches.map(|pitch| constant + pitch))
            }
        }
    }

    /// Its length in samples.
    #[must_use]
    pub const fn length(&self) -> f64 {
        match *self {
            Self::Linear { length, .. } | Self::Rule { length, .. } => length,
        }
    }
}

/// The complete Bell polynomial B₆ of bounds of the first six derivatives,
/// each product saturated, so that an infinite bound gives the largest finite
/// number and never a NaN: by Faà di Bruno, the sixth derivative of e^g is
/// e^g·B₆(g′, …, g⁽⁶⁾), and B₆'s coefficients are all positive.
fn bell(bounds: [f64; 6]) -> f64 {
    let [x1, x2, x3, x4, x5, x6] = bounds;
    let product = |factors: &[f64]| factors.iter().fold(1.0, |product, factor| finite(product * factor));
    let terms = [
        product(&[x1, x1, x1, x1, x1, x1]),
        15.0 * product(&[x1, x1, x1, x1, x2]),
        20.0 * product(&[x1, x1, x1, x3]),
        45.0 * product(&[x1, x1, x2, x2]),
        15.0 * product(&[x1, x1, x4]),
        60.0 * product(&[x1, x2, x3]),
        6.0 * product(&[x1, x5]),
        15.0 * product(&[x2, x2, x2]),
        15.0 * product(&[x2, x4]),
        10.0 * product(&[x3, x3]),
        x6,
    ];
    terms.into_iter().fold(0.0, |sum, term| finite(sum + term))
}

/// The phase φ₀ + sτ, reduced to [0, 1): the product formed exactly as the sum
/// of two binary64 numbers (two-product), its whole turns taken away exactly,
/// and the rest summed with φ₀ in two roundings, within u(3 + |lo|) turns of
/// the exact phase, lo the product's low part, at most u·sτ, as the LFO's own
/// phase is (`crate::lfo`).
fn turns_after(turns: f64, step: f64, samples: f64) -> f64 {
    let (high, low) = two_product(step, samples);
    let whole = high.floor();
    let part = (high - whole) + (low + turns);
    let reduced = part - part.floor();
    if reduced < 1.0 { reduced } else { 0.0 }
}

impl Piece {
    /// The largest magnitude of a linear quantity over the piece, at one of its
    /// ends.
    fn ends(&self, start: f64, slope: f64) -> f64 {
        start.abs().max(finite(slope.mul_add(self.length, start)).abs())
    }

    /// λ as P + bτ where it is linear on the piece, in the ratio mode and in the
    /// fixed mode: (λ(0), its slope, λ(0) in the fixed mode, its slope), none
    /// where a glide moves it or the LFO's depth and wave both move, or the
    /// wave is a sine that moves.
    ///
    /// A glide that does not move, ρ being 0, adds its offset. The LFO's pitch
    /// A·ℓ at the start and its slope, A′ℓ or Aℓ′, round within u each, and
    /// their sums with P and b within u each: λ(0) within u(|A₀ℓ₀| + |λ(0)|),
    /// and the slope within u(|A′ℓ₀| + |Aℓ′| + |λ′|).
    fn linear(&self) -> Option<[f64; 4]> {
        let pitch = if self.glide == 0.0 {
            self.pitch
        } else if self.rate <= 0.0 {
            finite(self.pitch + self.glide)
        } else {
            return None;
        };
        let (lfo, lfo_slope) = if self.depth == 0.0 && self.depth_slope == 0.0 {
            (0.0, 0.0)
        } else {
            match self.shape {
                Shape::Constant(value) => (finite(self.depth * value), finite(self.depth_slope * value)),
                Shape::Sine { turns, step: 0.0 } => {
                    let value = -sin2pi(turns);
                    (finite(self.depth * value), finite(self.depth_slope * value))
                }
                Shape::Linear { value, slope } if self.depth_slope == 0.0 => {
                    (finite(self.depth * value), finite(self.depth * slope))
                }
                Shape::Linear { .. } | Shape::Sine { .. } => return None,
            }
        };
        Some([finite(pitch + lfo), finite(self.slope + lfo_slope), lfo, lfo_slope])
    }

    /// Bounds a₁ … a₆ of the first six derivatives of λ ln 2 on the piece, in
    /// the ratio mode, which bound the fixed mode's too: |b| for the pitch
    /// envelope; (|d| + 1/4)ρᵏ for the glide; for the LFO's pitch A·ℓ, with Aₘ
    /// and ℓₘ the largest |A| and |ℓ| at the piece's ends, |A′ℓ| for a
    /// constant wave, |A′|ℓₘ + Aₘ|ℓ′| and 2|A′ℓ′| for a linear one, and
    /// Aₘωᵏ + k|A′|ωᵏ⁻¹ for a sine, ω = 2πs (Leibniz); each times ln 2. They
    /// are computed within a few u of themselves, which [`SAFETY`] covers.
    fn bounds(&self) -> [f64; 6] {
        let mut bounds = [0.0; 6];
        bounds[0] = self.slope.abs();
        if self.glide != 0.0 && self.rate > 0.0 {
            let mut term = self.glide.abs() + 0.25;
            for bound in &mut bounds {
                term = finite(term * self.rate);
                *bound = finite(*bound + term);
            }
        }
        let depth = self.ends(self.depth, self.depth_slope);
        let depth_slope = self.depth_slope.abs();
        match self.shape {
            Shape::Constant(value) => bounds[0] = finite(bounds[0] + finite(depth_slope * value.abs())),
            Shape::Linear { value, slope } => {
                let wave = self.ends(value, slope);
                bounds[0] = finite(bounds[0] + finite(depth_slope * wave) + finite(depth * slope.abs()));
                bounds[1] = finite(bounds[1] + 2.0 * finite(depth_slope * slope.abs()));
            }
            Shape::Sine { step, .. } => {
                let omega = finite(TAU * step.abs());
                let (mut power, mut below) = (omega, 1.0);
                for (order, bound) in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0].into_iter().zip(&mut bounds) {
                    let term = finite(finite(depth * power) + finite(order * finite(depth_slope * below)));
                    *bound = finite(*bound + term);
                    below = power;
                    power = finite(power * omega);
                }
            }
        }
        bounds.map(|bound| finite(bound * LN_2))
    }

    /// The parts on which the piece is integrated (MATH §2.5): 1 where λ is
    /// linear on it, the closed form taking it whole; elsewhere equal parts no
    /// longer than a length h on which h⁶B₆e^(a₁h) ≤ 2016000·2⁻⁶⁰, B₆ the
    /// complete Bell polynomial of the bounds a₁ … a₆ of λ ln 2's derivatives,
    /// so that the rule's truncation on each, h⁷|f⁽⁶⁾|/2016000 with |f⁽⁶⁾| ≤
    /// B₆·max f and max f ≤ e^(a₁h)·min f on the part, stays below 2⁻⁶⁰ of the
    /// part's integral. h is 0.999·h₀e^(−0.999·a₁h₀/6), h₀ =
    /// (2016000·2⁻⁶⁰/B₆)^(1/6), which meets the bound however its bounds round,
    /// and lies within 0.3 % of the longest length that does, a₁h₀ being at
    /// most 2016000^(1/6)·2⁻¹⁰ as B₆ is at least a₁⁶: the count passes the
    /// fewest by 0.3 % at most. A count, at least 1, which may be beyond every
    /// integer, or +∞, where the piece's signals move far beyond their
    /// scales.
    #[must_use]
    pub fn count(&self) -> f64 {
        if self.linear().is_some() {
            return 1.0;
        }
        let bounds = self.bounds();
        let bell = bell(bounds);
        if bell <= 0.0 {
            return 1.0;
        }
        let first = exp2(log2(TRUNCATION / bell) / 6.0) * SAFETY;
        let length = first * exp2(-(bounds[0] * first / 6.0) * LOG2_E);
        (self.length / length).ceil().max(1.0)
    }

    /// The voice's λ at τ samples into the piece, in the ratio mode and in the
    /// fixed mode.
    ///
    /// P + bτ rounds within u|P + bτ|; G within u((|d| + 1/4)(2.24|m| +
    /// 3x·e^(−x)) + |G|), x = ρτ and m = e^(−x) − 1, as a glide's offset does
    /// (`crate::portamento`), 4.35u(|d| + 1/4) at most; A within u|A|; ℓ is
    /// exact where it holds still, within u where it is linear, and within 21u
    /// where it is a sine, its phase within 3u turns and sin2pi within 1.46u
    /// of itself; A·ℓ within u|Aℓ|; and the two sums within u of themselves
    /// each: λ within u(|P + bτ| + 4.35(|d| + 1/4) + |P + bτ + G| + |λ| +
    /// |A|(2 + e)), e = 0, 1 or 21 the wave's, and the fixed mode's within
    /// u|A|(2 + e), |ℓ| being at most 1.
    #[must_use]
    pub fn at(&self, samples: f64) -> (f64, f64) {
        let envelope = self.slope.mul_add(samples, self.pitch);
        let glide = if self.glide == 0.0 {
            0.0
        } else {
            let from = self.glide.abs();
            let falling = exp2m1(-(self.rate * samples) * LOG2_E);
            (from + 0.25).mul_add(falling, from).max(0.0).copysign(self.glide)
        };
        let depth = self.depth_slope.mul_add(samples, self.depth);
        let wave = match self.shape {
            Shape::Constant(value) => value,
            Shape::Linear { value, slope } => slope.mul_add(samples, value),
            Shape::Sine { turns, step } => -sin2pi(turns_after(turns, step, samples)),
        };
        let lfo = finite(depth * wave);
        (finite(finite(envelope + glide) + lfo), lfo)
    }

    /// The index-th of `count` parts of the piece, from 0: the whole piece in
    /// closed form where λ is linear on it, and elsewhere the part from
    /// L·index/count to L·(index + 1)/count, the last ending at L, with λ at the
    /// rule's three instants (`crate::pitch::phase::gauss_instants`). Each
    /// boundary rounds within 2u·L of itself, and a part's length, the
    /// difference of two boundaries within a factor of 2 of each other, is
    /// exact (Sterbenz), so that the parts tile the piece exactly; an instant
    /// lies within u·L + 2u·h of its place, h the part's length, which moves λ
    /// there by its slope times that.
    #[must_use]
    pub fn part(&self, count: u32, index: u32) -> Part {
        if let Some([pitch, slope, fixed_pitch, fixed_slope]) = self.linear() {
            return Part::Linear {
                length: self.length,
                pitch,
                slope,
                fixed_pitch,
                fixed_slope,
            };
        }
        let count = count.max(1);
        let boundary = |index: u32| {
            if index >= count {
                self.length
            } else {
                self.length * f64::from(index) / f64::from(count)
            }
        };
        let start = boundary(index);
        let length = boundary(index.saturating_add(1)) - start;
        let (mut pitches, mut fixed) = ([0.0; 3], [0.0; 3]);
        for ((pitch, fixed), instant) in pitches.iter_mut().zip(&mut fixed).zip(gauss_instants(length)) {
            (*pitch, *fixed) = self.at(start + instant);
        }
        Part::Rule { length, pitches, fixed }
    }
}

/// The parts of a sample, at most `osc.max_pieces` of them, held in memory
/// that the engine takes when it is prepared, so that the audio thread never
/// allocates (DESIGN §5).
#[derive(Debug, Clone, PartialEq)]
pub struct Parts {
    /// The parts.
    parts: Vec<Part>,
    /// The most that it holds, `osc.max_pieces`.
    limit: u32,
}

impl Parts {
    /// Room for so many parts, at least 1, or none where the memory cannot be
    /// had.
    #[must_use]
    pub fn new(limit: u32) -> Option<Self> {
        let limit = limit.max(1);
        let mut parts = Vec::new();
        parts.try_reserve_exact(usize::try_from(limit).ok()?).ok()?;
        Some(Self { parts, limit })
    }

    /// The parts of the last sample, in their order.
    #[must_use]
    pub fn as_slice(&self) -> &[Part] {
        &self.parts
    }

    /// The most that it holds.
    #[must_use]
    pub const fn limit(&self) -> u32 {
        self.limit
    }

    /// Holds a part more, where it has room, which the walk's budget keeps.
    fn push(&mut self, part: Part) {
        if self.parts.len() < self.parts.capacity() {
            self.parts.push(part);
        }
    }
}

/// A voice's portamento as its pitch takes it (MATH §7.3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Portamento {
    /// None: the voice sounds its note's own pitch.
    Still,
    /// A glide, in octaves or in keys.
    Glide(Glide),
    /// A glissando, in keys.
    Glissando(Glissando),
}

impl Portamento {
    /// The instant of its next change: a glide's arrival or a glissando's next
    /// step.
    fn end(&self) -> Option<Instant> {
        match self {
            Self::Still => None,
            Self::Glide(glide) => glide.end(),
            Self::Glissando(glissando) => glissando.end(),
        }
    }

    /// Its changes at or before an instant: a glide's arrival, and a
    /// glissando's steps in closed form (`Glissando::advance`).
    pub fn advance(&mut self, to: Instant) {
        match self {
            Self::Still => {}
            Self::Glide(glide) => {
                if glide.end().is_some_and(|end| end <= to) {
                    glide.next();
                }
            }
            Self::Glissando(glissando) => glissando.advance(to),
        }
    }

    /// A glide's offset at an instant in octaves and its rate, none where there
    /// is no glide: under 12-TET a key is a twelfth of an octave.
    fn glide(&self, at: Instant) -> Option<(f64, f64)> {
        match self {
            Self::Glide(glide) => {
                let offset = glide.offset(at);
                let octaves = match glide.unit() {
                    Unit::Octaves => offset,
                    Unit::Keys => offset / 12.0,
                };
                Some((octaves, glide.rate()))
            }
            Self::Still | Self::Glissando(_) => None,
        }
    }

    /// Its offset at an instant in octaves, once [`Portamento::advance`] has
    /// taken it there: a glide's, or a glissando's key heard.
    fn octaves(&self, at: Instant) -> f64 {
        match self {
            Self::Still => 0.0,
            Self::Glide(_) => self.glide(at).map_or(0.0, |(octaves, _)| octaves),
            Self::Glissando(glissando) => glissando.offset() / 12.0,
        }
    }
}

/// The signals that move a voice's pitch within a sample (MATH §2.4), as they
/// are at the sample's start.
#[derive(Debug, Clone, Copy)]
pub struct Signals<'a> {
    /// The voice's pitch envelope.
    pub pitch_envelope: PitchEnvelope,
    /// Its parameters.
    pub pitch_parameters: &'a peg::Parameters,
    /// The voice's portamento.
    pub portamento: Portamento,
    /// The LFO that the voice takes, its part's or its own (MATH §6.5).
    pub lfo: &'a Lfo,
    /// The LFO's wave.
    pub wave: Wave,
    /// σ(PMS), in octaves (`crate::control::pitch_sensitivity`).
    pub sensitivity: f64,
    /// PMD, the depth of the LFO's pitch modulation.
    pub depth: f64,
    /// The controllers' depth of the pitch (`crate::control::controllers`).
    pub controllers: Limited,
    /// The engine's rate fₑ.
    pub sample_rate: f64,
}

impl Signals<'_> {
    /// A = σ(PMS)·D at a fade, D the depth of the LFO's pitch modulation, the
    /// first evaluation of `crate::control::modulation`'s: within u|A| of σ
    /// times D as it rounds.
    fn depth_at(&self, fade: f64) -> f64 {
        finite(self.sensitivity * modulation(self.depth, fade, self.controllers).value)
    }
}

/// The LFO's grid of corners and edges within a sample: half a turn for the
/// triangle and the square, a turn for the saws and the sample and hold, and
/// none for the sine or a phase that stands still (MATH §6.2).
#[derive(Debug, Clone, Copy)]
struct Grid {
    /// The phase φ at the sample's start, in [0, 1).
    turns: f64,
    /// Its advance s a sample, 0 where it stands still.
    step: f64,
    /// The cells' width in turns, 1/2 or 1, or 0 where there are none.
    width: f64,
    /// The draws that the LFO had made by the sample's start.
    draws: u64,
}

impl Grid {
    /// The grid of an LFO's wave at an instant.
    fn new(lfo: &Lfo, wave: Wave, at: Instant) -> Self {
        let raw = lfo.step();
        let step = if raw < STILL { raw } else { 0.0 };
        let width = match wave {
            _ if step <= 0.0 => 0.0,
            Wave::Triangle | Wave::Square => 0.5,
            Wave::SawDown | Wave::SawUp | Wave::SampleAndHold => 1.0,
            Wave::Sine => 0.0,
        };
        Self {
            turns: lfo.turns(at),
            step,
            width,
            draws: lfo.draws(at),
        }
    }

    /// The samples from the sample's start to the start of cell k, where the
    /// phase φ + sτ, counted on from φ without its whole turns taken away,
    /// reaches k·w, w the cells' width: (k·w − φ)/s, rounding within u of the
    /// difference and u of the quotient, non-decreasing in k, and at or below
    /// 0 for the cell that holds φ.
    fn start_of(&self, cell: f64) -> f64 {
        finite(cell * self.width - self.turns) / self.step
    }

    /// The cell that holds the instant just after τ samples from the sample's
    /// start: the last whose start lies at or before τ, found from the phase
    /// there, ⌊(φ + sτ)/w⌋, and checked against the cells' starts as
    /// [`Grid::start_of`] gives them, so that the walk's pieces meet the grid
    /// exactly. The walk takes a grid only where a sample holds fewer cells
    /// than its budget of parts, s/w below 2³², where the phase there lies
    /// within u·sτ of itself, far less than a cell, and the estimate within
    /// one cell of the cell that the starts give.
    fn cell(&self, samples: f64) -> f64 {
        if self.width <= 0.0 {
            return 0.0;
        }
        let (high, low) = two_product(self.step, samples);
        let mut cell = (((high + low) + self.turns) / self.width).floor().max(0.0);
        if cell > 0.0 && self.start_of(cell) > samples {
            cell -= 1.0;
        }
        if self.start_of(cell + 1.0) <= samples {
            cell += 1.0;
        }
        cell
    }

    /// The samples from the sample's start to the start of the cell after one.
    fn next(&self, cell: f64) -> Option<f64> {
        (self.width > 0.0).then(|| self.start_of(cell + 1.0))
    }

    /// The cells that a sample holds, about: s/w, 0 where there are none.
    fn cells(&self) -> f64 {
        if self.width > 0.0 { self.step / self.width } else { 0.0 }
    }

    /// The wave on a cell from τ samples on (MATH §6.2): its value there from
    /// the phase within the cell's period, f = φ + sτ less the period's start,
    /// held within the cell, and its slope; the sample and hold's value of the
    /// draws made by the cell; and the sine's phase.
    fn shape(&self, lfo: &Lfo, wave: Wave, cell: f64, samples: f64) -> Shape {
        if self.step <= 0.0 {
            let held = if wave == Wave::SampleAndHold {
                lfo.held(self.draws)
            } else {
                0.0
            };
            return Shape::Constant(shape(wave, self.turns, held));
        }
        let period = (cell * self.width).floor();
        let (high, low) = two_product(self.step, samples);
        let within = ((high - period) + low) + self.turns;
        let (first, last) = if self.width < 1.0 {
            let half = cell * self.width - period;
            (half, half + 0.5)
        } else {
            (0.0, 1.0)
        };
        let f = within.clamp(first, last);
        let s = self.step;
        match wave {
            Wave::Triangle if first < 0.5 => Shape::Linear {
                value: (-4.0_f64).mul_add(f, 1.0),
                slope: -4.0 * s,
            },
            Wave::Triangle => Shape::Linear {
                value: 4.0_f64.mul_add(f, -3.0),
                slope: 4.0 * s,
            },
            Wave::SawDown => Shape::Linear {
                value: (-2.0_f64).mul_add(f, 1.0),
                slope: -2.0 * s,
            },
            Wave::SawUp => Shape::Linear {
                value: 2.0_f64.mul_add(f, -1.0),
                slope: 2.0 * s,
            },
            Wave::Square => Shape::Constant(if first < 0.5 { -1.0 } else { 1.0 }),
            Wave::SampleAndHold => Shape::Constant(lfo.held(self.draws.wrapping_add(whole(cell)))),
            Wave::Sine => Shape::Sine {
                turns: turns_after(self.turns, s, samples),
                step: s,
            },
        }
    }
}

/// A whole count of cells below 2⁶⁴ as a u64, modulo 2⁶⁴ beyond, as the random
/// source takes its indices (MATH §6.3).
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole binary64 of 0 or more converts exactly below 2⁶⁴ and saturates beyond"
)]
fn whole(cell: f64) -> u64 {
    cell as u64
}

/// Where the fade of the LFO's delay is within a sample (MATH §6.4): w as a
/// line from an anchor, 0 before its rise and 1 after it.
#[derive(Debug, Clone, Copy)]
struct Fade {
    /// The samples from the sample's start to the rise's start, at or below 0
    /// where it had begun.
    rise: f64,
    /// The samples to where it is full, +∞ where it never is within any time.
    full: f64,
    /// The rise's length in samples.
    length: f64,
    /// w at the rise's anchor: 0 at its start, or w at the sample's start
    /// where it rises there.
    from: f64,
}

impl Fade {
    /// The fade of an LFO from an instant on.
    fn new(fading: Fading) -> Self {
        match (fading.rise, fading.full) {
            (Some(rise), full) => Self {
                rise,
                full: full.unwrap_or(f64::INFINITY),
                length: fading.length,
                from: 0.0,
            },
            (None, Some(full)) => Self {
                rise: 0.0,
                full,
                length: fading.length,
                from: fading.depth,
            },
            (None, None) => Self {
                rise: f64::NEG_INFINITY,
                full: f64::NEG_INFINITY,
                length: 0.0,
                from: 1.0,
            },
        }
    }

    /// w at τ samples by the law of the stage that holds a piece's middle, so
    /// that a piece takes its stage's line at both its ends: 0 before the rise,
    /// the rise's line, and 1 once full. The rise's line rounds within u of
    /// the difference and u of the quotient and u of the sum.
    fn at(&self, samples: f64, middle: f64) -> f64 {
        if middle >= self.full {
            1.0
        } else if middle < self.rise {
            0.0
        } else {
            self.from + (samples - self.rise) / self.length
        }
    }

    /// The samples to where w reaches a fade within its rise, none where it
    /// does not.
    fn reaching(&self, fade: f64) -> Option<f64> {
        (fade > self.from && self.length > 0.0)
            .then(|| (fade - self.from).mul_add(self.length, self.rise))
            .filter(|samples| *samples < self.full)
    }
}

/// The parts of the sample that runs one engine sample from an instant, as the
/// walk or the fallback gives them (MATH §2.5); the signals' own states are
/// left as they are, which [`advance`] then takes to the sample's end.
pub fn sample(parts: &mut Parts, from: Instant, signals: &Signals<'_>) {
    if !walk(parts, from, signals) {
        fallback(parts, from, signals);
    }
}

/// The signals' own states taken to an instant: the pitch envelope's segments
/// that end at or before it ended, and the portamento's changes made, as the
/// walk takes them within a sample.
pub fn advance(
    pitch_envelope: &mut PitchEnvelope,
    parameters: &peg::Parameters,
    portamento: &mut Portamento,
    to: Instant,
    sample_rate: f64,
) {
    while pitch_envelope.end().is_some_and(|end| end <= to) {
        let before = pitch_envelope.stage();
        pitch_envelope.next(parameters, sample_rate);
        if pitch_envelope.stage() == before {
            break;
        }
    }
    portamento.advance(to);
}

/// The instant τ samples after the start of a sample, exactly for τ from 0 to 1.
fn instant_after(from: Instant, samples: f64) -> Instant {
    from.after(samples).unwrap_or(from)
}

/// The least of the changes after τ, and of 1, the sample's end.
fn earliest(samples: f64, changes: [Option<f64>; 6]) -> f64 {
    changes
        .into_iter()
        .flatten()
        .filter(|change| *change > samples)
        .fold(1.0, f64::min)
}

/// The walk over a sample: its pieces between the instants where a signal has
/// a corner or a jump, and their parts, while the budget of `osc.max_pieces`
/// parts lasts, each piece taking one at least; false where it does not, and
/// at once where the LFO's wave has more corners and edges in the sample than
/// the budget, each of which would take a piece.
fn walk(parts: &mut Parts, from: Instant, signals: &Signals<'_>) -> bool {
    parts.parts.clear();
    let mut budget = f64::from(parts.limit);
    let grid = Grid::new(signals.lfo, signals.wave, from);
    if grid.cells() >= budget {
        return false;
    }
    let mut envelope = signals.pitch_envelope;
    let mut portamento = signals.portamento;
    let fade = Fade::new(signals.lfo.fading(from));
    let corner = saturating_fade(signals.depth, signals.controllers).and_then(|fade_at| fade.reaching(fade_at));
    let mut samples = 0.0;
    while samples < 1.0 {
        let at = instant_after(from, samples);
        advance(
            &mut envelope,
            signals.pitch_parameters,
            &mut portamento,
            at,
            signals.sample_rate,
        );
        let since = |instant: Option<Instant>| instant.map(|instant| instant.samples_since(from));
        let cell = grid.cell(samples);
        let next = earliest(
            samples,
            [
                since(envelope.corner()),
                since(portamento.end()),
                grid.next(cell),
                Some(fade.rise),
                Some(fade.full),
                corner,
            ],
        );
        let length = next - samples;
        let middle = samples + length / 2.0;
        let moving = envelope.corner().is_some_and(|corner| corner > at);
        let glide = portamento.glide(at).filter(|(offset, _)| *offset != 0.0);
        let held = match portamento {
            Portamento::Glissando(glissando) => glissando.offset() / 12.0,
            Portamento::Still | Portamento::Glide(_) => 0.0,
        };
        let depth = signals.depth_at(fade.at(samples, middle));
        let depth_end = signals.depth_at(fade.at(next, middle));
        let piece = Piece {
            length,
            pitch: finite(envelope.level(at) + held),
            slope: if moving { envelope.slope() } else { 0.0 },
            glide: glide.map_or(0.0, |(offset, _)| offset),
            rate: glide.map_or(0.0, |(_, rate)| rate),
            depth,
            depth_slope: finite(depth_end - depth) / length,
            shape: grid.shape(signals.lfo, signals.wave, cell, samples),
        };
        let count = piece.count();
        if count > budget {
            return false;
        }
        budget -= count;
        let count = parts_of(count);
        for index in 0..count {
            parts.push(piece.part(count, index));
        }
        samples = next;
    }
    true
}

/// A count of parts within the budget as a u32: at most `osc.max_pieces`.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole binary64 of 1 to u32::MAX converts exactly"
)]
fn parts_of(count: f64) -> u32 {
    count as u32
}

/// The fallback past the budget (MATH §2.5): the rule on `osc.max_pieces`
/// equal parts of the sample, each signal taken at the rule's instants by its
/// own law, whatever its corners and jumps.
fn fallback(parts: &mut Parts, from: Instant, signals: &Signals<'_>) {
    parts.parts.clear();
    let count = parts.limit;
    let boundary = |index: u32| {
        if index >= count {
            1.0
        } else {
            f64::from(index) / f64::from(count)
        }
    };
    let mut envelope = signals.pitch_envelope;
    let mut portamento = signals.portamento;
    for index in 0..count {
        let start = boundary(index);
        let length = boundary(index + 1) - start;
        let (mut pitches, mut fixed) = ([0.0; 3], [0.0; 3]);
        for ((pitch, fixed), instant) in pitches.iter_mut().zip(&mut fixed).zip(gauss_instants(length)) {
            let at = instant_after(from, start + instant);
            advance(
                &mut envelope,
                signals.pitch_parameters,
                &mut portamento,
                at,
                signals.sample_rate,
            );
            let lfo = finite(signals.depth_at(signals.lfo.fade(at)) * signals.lfo.wave(at, signals.wave));
            *pitch = finite(finite(envelope.level(at) + portamento.octaves(at)) + lfo);
            *fixed = lfo;
        }
        parts.push(Part::Rule { length, pitches, fixed });
    }
}
