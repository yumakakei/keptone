// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! One voice of a part (docs/MATH.md §1, §13): the note that it plays, its
//! operators' phases, envelopes, offsets and switches, the pitch envelope and
//! the portamento that move its pitch, its own LFO under `lfo.scope = voice`,
//! and its graph's evaluation at each engine sample. A voice is a slot of its
//! part (§13.2): it keeps what its last note leaves, its phases, for the next.
//!
//! A sample is taken in three steps, as MATH §0.3 orders them.
//! [`Voice::advance`] integrates the sample by the laws that held over it:
//! each operator's phase by its increment over the sample's pieces (§2.5),
//! each envelope run over the sample at the rates' key (§4.7), the pitch
//! envelope and the portamento taken to the sample's end, and the
//! relaxation's lags (§9.6), which take θ and g at instants within the sample
//! from the voice's states at its start run over the part of the sample up to
//! each ([`Voice::within`]). Then the events of the sample's instant act:
//! [`Voice::key_on`], [`Voice::key_off`], [`Voice::switch`], and
//! [`Voice::change`], a new patch of its part. Then
//! [`Voice::output`] gives the voice's output there, its carriers' sum as
//! `output.carrier_normalization` scales it (§14).
//!
//! An operator's gain at an instant is its envelope's level T with its offset
//! O (§3.3), 0 where that lies below the end level of `eg.end_db` (§4.6,
//! §13.4), lowered by the
//! amplitude modulation and the EG bias (§7.2), through the gain law (§3.4),
//! and times its switch's factor (§8.5). Its pitch in the ratio mode is the
//! key's with its detune, the bend, the per-note pitch, the master tune and
//! the pieces' pitch envelope, LFO and portamento (§2.2, §2.4); in the fixed
//! mode, its fixed pitch with the bend, the per-note pitch and the LFO's
//! pitch (§2.3). Under 12-TET, milestone M3's only tuning, both forms of
//! `tuning.modulation` give these intervals (§2.1). tests/voice.rs checks the
//! voice against the laws that it composes, each of which its own tests check.

use keptone_math::eft::two_product;
use keptone_model::voice::Mode;

use crate::binary64::{finite, same};
use crate::compile::{CompiledFunction, CompiledOperator, CompiledPatch};
use crate::configuration::{
    AlgorithmScope, Configuration, FreePhaseStart, LfoScope, Modulation, Normalization, RateKey, ScalingKey,
};
use crate::control::{Limited, kept, lowering, modulation};
use crate::eg::end_level;
use crate::eg::envelope::{self, Envelope, Stage};
use crate::graph::evaluation::{Memory, Plan};
use crate::level::{distance, gain, keyboard_scaling, offset};
use crate::lfo::Lfo;
use crate::peg::{self, PitchEnvelope};
use crate::pieces::{self, Keys, Parts, Portamento, Signals};
use crate::pitch::phase::Phase;
use crate::pitch::{TwelveTet, detune, transposed_key};
use crate::portamento::{Glide, Glissando, Unit};
use crate::random::{self, Streams};
use crate::time::Instant;

/// The offset O at which the envelope of an operator that is silent runs, its
/// level λ at or below 0 (MATH §3.2): every level of an envelope lies there
/// below the floor of MATH §4.6, as it does at any O below 400, so that its segments
/// take the floor's timing, as the DX7's do for an operator so far down
/// (§4.6), and an offset that a moving scaling key raises later (§2.6) moves
/// its levels as a live change of O does (§4.7).
const SILENT: f64 = 0.0;

/// A phase's increment a sample from which it is a whole number of turns at
/// binary64's precision, 2⁵², at which a phase stands still (MATH §2.2).
const STILL: f64 = 4_503_599_627_370_496.0;

/// The envelope at which a voice that has had no note rests; its first key-on
/// starts its operators' envelopes from the patch's L4 instead (MATH §4.5).
const RESTING: envelope::Parameters = envelope::Parameters {
    rates: [0.0; 4],
    levels: [0.0; 4],
    rate_scaling: 0.0,
};

/// The pitch envelope of a voice that has had no note, at its centre; a
/// key-on starts it from the patch's L4 (MATH §5).
const CENTRED: peg::Parameters = peg::Parameters {
    rates: [0.0; 4],
    levels: [50.0; 4],
};

/// A note that a voice plays (MATH §13.2): its key and velocity as its part
/// takes them, and its per-note pitch kₙₒₜₑ in keys (DESIGN §7.3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Note {
    /// The key, before transposition and the part's note shift.
    pub key: f64,
    /// The velocity, 0 to 127 on its scale.
    pub velocity: f64,
    /// The per-note pitch kₙₒₜₑ in keys.
    pub per_note: f64,
}

/// What a voice takes from its part over a sample and at its instant (MATH
/// §13.5): the part's compiled patch, the engine's configuration and tuning,
/// the part's LFO (§6.5), its note shift, the controllers' depths and bias
/// (§7.1), the bend in keys (§2.4), the master tune in octaves and the part's
/// random streams (§6.3). Each is the value that held over the sample while
/// the voice integrates it, and the value after the events at its instant
/// from then on.
#[derive(Debug, Clone, Copy)]
pub struct Context<'a> {
    /// The part's compiled patch.
    pub patch: &'a CompiledPatch,
    /// The engine's configuration.
    pub configuration: &'a Configuration,
    /// The tuning, 12-TET in milestone M3.
    pub tuning: &'a TwelveTet,
    /// The part's LFO, which the voice follows under `lfo.scope = part` and
    /// from which its own takes the phase without LFO SYNC under `voice`.
    pub lfo: &'a Lfo,
    /// The part's note shift s (MATH §2.1).
    pub note_shift: f64,
    /// The depth of the controllers assigned to the pitch (MATH §7.1).
    pub pitch_controllers: Limited,
    /// The depth of those assigned to the amplitude.
    pub amplitude_controllers: Limited,
    /// The EG bias of those assigned to it.
    pub bias: Limited,
    /// The bend in keys (MATH §2.4).
    pub bend: f64,
    /// The master tune in octaves (MATH §2.4).
    pub master_tune: f64,
    /// The part's random streams (MATH §6.3).
    pub streams: Streams,
}

/// An operator switch's factor (MATH §8.5): it moves linearly toward 1 or 0
/// over `param.level_glide_s` from where it was when the switch acted.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Ramp {
    /// The factor when the switch acted.
    from: f64,
    /// Whether it moves toward 1, the switch on, or toward 0.
    on: bool,
    /// When the switch acted.
    start: Instant,
}

impl Ramp {
    /// A factor that holds at 1 or at 0.
    const fn settled(on: bool, at: Instant) -> Self {
        Self {
            from: if on { 1.0 } else { 0.0 },
            on,
            start: at,
        }
    }

    /// The factor at an instant for a glide of so many samples, at once where
    /// that is 0 or less: from ± Δ/glide toward 1 or 0 and held there, Δ the
    /// samples since the switch acted, within u(|Δ| + 1/2) of the exact count;
    /// the quotient and the sum round within u each.
    fn factor(&self, at: Instant, glide: f64) -> f64 {
        if glide <= 0.0 {
            return if self.on { 1.0 } else { 0.0 };
        }
        let moved = at.samples_since(self.start).max(0.0) / glide;
        if self.on {
            (self.from + moved).min(1.0)
        } else {
            (self.from - moved).max(0.0)
        }
    }
}

/// A change of an operator's offset O, ramped in T over `param.level_glide_s`
/// (MATH §4.7): O(t) = target − (1 − r)·difference, r rising linearly from 0
/// at the change to 1, the target the new parameters' offset, or, where the
/// operator falls silent, the offset at which its T lay at the end level of
/// `eg.end_db` at the change; from r = 1 on, the new parameters' offset.
#[derive(Debug, Clone, Copy, PartialEq)]
struct LevelRamp {
    /// The change's instant.
    start: Instant,
    /// The target less the offset, at the change.
    difference: f64,
    /// The target where the operator falls silent; none where the new
    /// parameters' offset is the target.
    silent: Option<f64>,
}

impl LevelRamp {
    /// r at an instant for a glide of so many samples, 1 where that is 0 or
    /// less: the samples since the change over the glide, within u(|Δ| + 1/2)
    /// of the exact count and the quotient within u; from 1 on the ramp is
    /// over.
    fn progress(&self, at: Instant, glide: f64) -> f64 {
        if glide <= 0.0 {
            return 1.0;
        }
        at.samples_since(self.start).max(0.0) / glide
    }

    /// The ramp that takes an operator's offset from one side of a change to
    /// the other at an instant, its envelope there and the end level given
    /// (MATH §4.7): none where both sides are silent, where the glide is 0 or
    /// less, and where the side that is silent leaves the operator's T at or
    /// below the end level at the change, so that the change acts at once.
    fn between(
        before: Option<f64>,
        after: Option<f64>,
        envelope: &Envelope,
        at: Instant,
        end: f64,
        glide: f64,
    ) -> Option<Self> {
        if glide <= 0.0 {
            return None;
        }
        match (before, after) {
            (Some(before), Some(after)) => Some(Self {
                start: at,
                difference: finite(after - before),
                silent: None,
            }),
            (Some(before), None) => {
                let level = envelope.level(at, before).filter(|&level| level > end)?;
                let target = finite(before + (end - level));
                Some(Self {
                    start: at,
                    difference: finite(target - before),
                    silent: Some(target),
                })
            }
            (None, Some(after)) => {
                let level = envelope.level(at, after).filter(|&level| level > end)?;
                let from = finite(after + (end - level));
                Some(Self {
                    start: at,
                    difference: finite(after - from),
                    silent: None,
                })
            }
            (None, None) => None,
        }
    }
}

/// What a voice keeps of each operator.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Operator {
    /// Its phase θ (MATH §2.5).
    phase: Phase,
    /// Its envelope (MATH §4).
    envelope: Envelope,
    /// Its offset O at the note's onset (MATH §3.2), none where it is silent;
    /// under `scaling.key = pitch` it is taken anew at each instant instead.
    offset: Option<f64>,
    /// The ramp of a change of its offset under way (MATH §4.7).
    ramp: Option<LevelRamp>,
    /// Its switch's factor.
    switch: Ramp,
    /// Its phase's increment over the last sample that the voice integrated,
    /// at which its phase runs on once its note has ended (MATH §2.5).
    step: f64,
}

/// What moves a voice's pitch: its pitch envelope and its portamento.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Motion {
    /// The pitch envelope (MATH §5).
    envelope: PitchEnvelope,
    /// The portamento (MATH §7.3).
    portamento: Portamento,
}

impl Motion {
    /// Its states taken to an instant from where they are, as the walk takes
    /// them within a sample ([`pieces::advance`]).
    fn at(mut self, at: Instant, context: &Context<'_>) -> Self {
        pieces::advance(
            &mut self.envelope,
            &context.patch.pitch_envelope,
            &mut self.portamento,
            at,
            context.configuration.sample_rate,
        );
        self
    }
}

/// What one of a voice's operators gives at an instant, as MATH §16's suites
/// read it through a probe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Probe {
    /// Its phase θ in turns at the voice's last instant, after the events
    /// there.
    pub phase: f64,
    /// Its gain g at the instant (MATH §3.4, §7.2, §8.5).
    pub gain: f64,
    /// Its level T at the instant, its offset O included and the amplitude
    /// modulation not; none where it is silent.
    pub level: Option<f64>,
    /// Its pitch in octaves at the instant, log₂ of its frequency over its
    /// ratio in the ratio mode and of its frequency in the fixed mode (MATH
    /// §2.2, §2.3).
    pub pitch: f64,
}

/// The state of a voice that its signals keep: its note, its keys, its
/// operators now and at the last sample's start, and what moves its pitch.
#[derive(Debug, Clone)]
struct State {
    /// The note it plays, none before its first.
    note: Option<Note>,
    /// k′, the note's key after transposition (MATH §2.1).
    transposed: f64,
    /// kₛ at the note's onset: the scaling key under `key` and `onset-pitch`,
    /// and k′ with the per-note pitch, from which it moves, under `pitch`
    /// (MATH §2.6).
    scaling: f64,
    /// Whether its key or the sustain pedal holds it.
    held: bool,
    /// Where its last note ended (MATH §13.3), none while it sounds.
    ended: Option<Instant>,
    /// Its own LFO under `lfo.scope = voice` (MATH §6.5).
    lfo: Option<Lfo>,
    /// Its operators, OP1 first.
    operators: Vec<Operator>,
    /// Its operators as they were at the last sample's start.
    saved: Vec<Operator>,
    /// What moves its pitch.
    motion: Motion,
    /// What moved it at the last sample's start.
    saved_motion: Motion,
    /// The last sample's start.
    from: Instant,
    /// The parts of a span of the last sample.
    span_parts: Parts,
    /// The rates' key over it.
    span_keys: Keys,
}

/// A vector of so many values, its memory taken now or none where it cannot
/// be had.
fn filled<T: Clone>(count: usize, value: T) -> Option<Vec<T>> {
    let mut values = Vec::new();
    values.try_reserve_exact(count).ok()?;
    values.resize(count, value);
    Some(values)
}

/// The signals that move a voice's pitch, as they are at an instant (MATH
/// §2.4), the LFO's the voice's own or its part's.
fn signals<'s>(motion: Motion, lfo: &'s Lfo, context: &Context<'s>) -> Signals<'s> {
    let patch = context.patch;
    Signals {
        pitch_envelope: motion.envelope,
        pitch_parameters: &patch.pitch_envelope,
        portamento: motion.portamento,
        lfo,
        wave: patch.lfo.wave,
        sensitivity: patch.pitch_sensitivity,
        depth: patch.lfo.pitch_depth,
        controllers: context.pitch_controllers,
        sample_rate: context.configuration.sample_rate,
    }
}

/// The rates' key over a span of so many samples from an instant (MATH §2.6,
/// §4.7), the signals as they are at its start, k′ the key after
/// transposition and the scaling key's base given: under `frequency` the
/// pitch envelope's and the portamento's pieces from k′; under `level` with
/// `scaling.key = pitch` the pitch's pieces from the base, the LFO's among
/// them; and under `level` otherwise the base, holding still.
fn rates_keys(
    keys: &mut Keys,
    (transposed, scaling): (f64, f64),
    from: Instant,
    samples: f64,
    signals: &Signals<'_>,
    context: &Context<'_>,
) {
    let configuration = context.configuration;
    match configuration.rate_key {
        RateKey::Frequency => keys.span(
            from,
            samples,
            transposed,
            signals.pitch_envelope,
            signals.pitch_parameters,
            signals.portamento,
            configuration.sample_rate,
        ),
        RateKey::Level if configuration.scaling_key == ScalingKey::Pitch => {
            keys.pitch_span(from, samples, scaling, context.bend, signals);
        }
        RateKey::Level => keys.hold_over(scaling, samples),
    }
}

/// An operator's offset O at a scaling key and a velocity (MATH §3.2), none
/// where it is silent.
fn offset_of(operator: &CompiledOperator, key: f64, velocity: f64) -> Option<f64> {
    let scaling = keyboard_scaling(distance(key, operator.break_point), operator.left, operator.right);
    offset(operator.output_level, scaling, operator.velocity_sensitivity, velocity)
}

/// An operator's increment in turns over a sample's parts, or a span's (MATH
/// §2.5): in the ratio mode at its ratio over the engine's rate, in the fixed
/// mode at 1 over it, its pitch's constant c beside the parts' λ; the parts'
/// increments summed in their order, each sum within u of itself.
fn increment(parts: &Parts, operator: &CompiledOperator, constant: f64, sample_rate: f64) -> f64 {
    let (ratio, fixed) = match operator.mode {
        Mode::Ratio => (finite(operator.ratio / sample_rate), false),
        Mode::Fixed => (finite(1.0 / sample_rate), true),
    };
    parts
        .as_slice()
        .iter()
        .fold(0.0, |sum, part| finite(sum + part.increment(ratio, constant, fixed)))
}

/// A phase in turns run on for so many samples at a step a sample, reduced to
/// [0, 1) (MATH §2.5): θ + step·Δ, the product formed exactly as the sum of two
/// binary64 by a fused multiply–add and its whole turns taken away, as the
/// LFO's phase is (`crate::lfo`), within 3u + 2u²·|step·Δ| turns of its value
/// at the binary64 step and Δ; a step of 2⁵² turns or more, a whole number of
/// turns, leaves the phase where it is, as [`Phase::advance`] does.
fn run_on(turns: f64, step: f64, samples: f64) -> f64 {
    if step.abs() >= STILL {
        return turns;
    }
    let (high, low) = two_product(step, samples.max(0.0));
    if !high.is_finite() {
        return turns;
    }
    let whole = high.floor();
    let part = (high - whole) + (low + turns);
    part - part.floor()
}

/// The glide of a switch's factor in samples, `param.level_glide_s` at the
/// engine's rate.
fn glide(context: &Context<'_>) -> f64 {
    finite(context.configuration.level_glide_s * context.configuration.sample_rate)
}

/// A voice's portamento, taken to an instant, after a change there of TIME or
/// of the glissando (MATH §7.3): none where the new TIME makes no glide, the
/// glide taking its target there; a glissando turned on starts its glide and
/// its key at the pitch there, one turned off leaves its glide to start at the
/// key heard, and otherwise the glide is re-anchored there at the new speed,
/// a glissando keeping its progress.
fn retimed(
    portamento: Portamento,
    at: Instant,
    function: &CompiledFunction,
    configuration: &Configuration,
) -> Portamento {
    let Some(speed) = function.portamento_speed else {
        return Portamento::Still;
    };
    let rate = configuration.sample_rate;
    match portamento {
        Portamento::Still => Portamento::Still,
        Portamento::Glide(mut glide) if !function.glissando => {
            glide.set_speed(at, speed, rate);
            Portamento::Glide(glide)
        }
        Portamento::Glide(glide) => {
            let keys = match glide.unit() {
                Unit::Octaves => finite(12.0 * glide.offset(at)),
                Unit::Keys => glide.offset(at),
            };
            Portamento::Glissando(Glissando::new(at, keys, speed, rate))
        }
        Portamento::Glissando(mut glissando) if function.glissando => {
            glissando.set_speed(at, speed, rate);
            Portamento::Glissando(glissando)
        }
        Portamento::Glissando(glissando) => {
            let keys = glissando.offset();
            Portamento::Glide(match configuration.modulation {
                Modulation::Pitch => Glide::new(at, keys / 12.0, speed, Unit::Octaves, rate),
                Modulation::Keys => Glide::new(at, keys, speed, Unit::Keys, rate),
            })
        }
    }
}

impl State {
    /// Whether it holds a context's patch's operators.
    fn holds(&self, context: &Context<'_>) -> bool {
        context.patch.operators.len() <= self.operators.len()
    }

    /// Whether it sounds: it plays a note that has not ended, and holds its
    /// patch's operators.
    fn sounds(&self, context: &Context<'_>) -> bool {
        self.note.is_some() && self.ended.is_none() && self.holds(context)
    }

    /// The LFO that it follows: its own, or its part's.
    fn lfo<'s>(&'s self, context: &Context<'s>) -> &'s Lfo {
        self.lfo.as_ref().unwrap_or(context.lfo)
    }

    /// The LFO's modulation of the pitch at an instant, σ(PMS)·D·ℓ in octaves
    /// (MATH §2.4), as the pieces take it (`crate::pieces`).
    fn lfo_pitch(&self, at: Instant, context: &Context<'_>) -> f64 {
        let (patch, lfo) = (context.patch, self.lfo(context));
        let depth = modulation(patch.lfo.pitch_depth, lfo.fade(at), context.pitch_controllers).value;
        finite(finite(patch.pitch_sensitivity * depth) * lfo.wave(at, patch.lfo.wave))
    }

    /// The constant part c of an operator's pitch over a sample in octaves
    /// (MATH §2.2–§2.4): in the ratio mode the key's pitch with its detune, the
    /// bend, the per-note pitch and the master tune; in the fixed mode its
    /// fixed pitch with the bend and the per-note pitch. Each sum and the
    /// quotient by 12 round within u of themselves.
    fn constant(&self, k: usize, context: &Context<'_>) -> f64 {
        let operator = &context.patch.operators[k];
        let per_note = self.note.map_or(0.0, |note| note.per_note);
        let moved = finite(finite(context.bend + per_note) / 12.0);
        match operator.mode {
            Mode::Ratio => {
                let key = context.tuning.pitch(self.transposed);
                let detuned = finite(key + detune(operator.detune, key));
                finite(finite(detuned + moved) + context.master_tune)
            }
            Mode::Fixed => finite(operator.fixed_pitch + moved),
        }
    }

    /// The rates' key kᴿ at an instant (MATH §2.6) from a state of what moves
    /// the pitch there: under `frequency` k′ and twelve keys an octave of the
    /// pitch envelope's level and the portamento's offset, as
    /// [`Keys::walk`] takes it; under `level` the scaling key, which under
    /// `scaling.key = pitch` moves with the pitch, as [`Keys::pitch_walk`]
    /// takes it.
    fn rates_key(&self, at: Instant, motion: Motion, context: &Context<'_>) -> f64 {
        match context.configuration.rate_key {
            RateKey::Frequency => {
                let octaves = finite(motion.envelope.level(at) + motion.portamento.octaves(at));
                finite(12.0_f64.mul_add(octaves, self.transposed))
            }
            RateKey::Level if context.configuration.scaling_key == ScalingKey::Pitch => {
                self.pitch_key(self.scaling, at, motion, context)
            }
            RateKey::Level => self.scaling,
        }
    }

    /// The scaling key kₛ at an instant under `scaling.key = pitch` (MATH
    /// §2.6), from a state of what moves the pitch at or before it: a base, k′
    /// and the per-note pitch, with twelve keys an octave of the pitch
    /// envelope's level, the portamento's offset and the LFO's pitch, and the
    /// bend.
    fn pitch_key(&self, base: f64, at: Instant, motion: Motion, context: &Context<'_>) -> f64 {
        let motion = motion.at(at, context);
        let moved = finite(motion.envelope.level(at) + motion.portamento.octaves(at));
        let octaves = finite(moved + self.lfo_pitch(at, context));
        finite(finite(12.0_f64.mul_add(octaves, base)) + context.bend)
    }

    /// The offset O that an operator's parameters give at a scaling key's base
    /// and an instant (MATH §3.2, §2.6), none where it is silent: at the base
    /// itself under `key` and `onset-pitch`, and under `pitch` at the scaling
    /// key that moves from it, from a state of what moves the pitch at or
    /// before the instant.
    fn law(
        &self,
        compiled: &CompiledOperator,
        base: f64,
        at: Instant,
        motion: Motion,
        context: &Context<'_>,
    ) -> Option<f64> {
        let note = self.note?;
        let key = match context.configuration.scaling_key {
            ScalingKey::Pitch => self.pitch_key(base, at, motion, context),
            ScalingKey::Key | ScalingKey::OnsetPitch => base,
        };
        offset_of(compiled, key, note.velocity)
    }

    /// An operator's offset O at an instant (MATH §3.2, §2.6, §4.7), none where
    /// it is silent: its offset at the note's onset, or under `scaling.key =
    /// pitch` its offset at the scaling key there, from a state of what moves
    /// the pitch at or before it; while a change's ramp is under way, that
    /// offset less (1 − r) times the ramp's difference, the ramp's own target
    /// standing for it where the operator falls silent.
    fn offset_at(&self, k: usize, at: Instant, motion: Motion, context: &Context<'_>) -> Option<f64> {
        let operator = self.operators.get(k)?;
        let law = if context.configuration.scaling_key == ScalingKey::Pitch {
            self.law(context.patch.operators.get(k)?, self.scaling, at, motion, context)
        } else {
            operator.offset
        };
        let Some(ramp) = operator.ramp else {
            return law;
        };
        let r = ramp.progress(at, glide(context));
        if r >= 1.0 {
            return law;
        }
        let target = ramp.silent.or(law)?;
        Some(finite(target - (1.0 - r) * ramp.difference))
    }

    /// An operator's level T at an instant from a state of it, its offset O
    /// included (MATH §3.3), none where it is silent.
    fn level(&self, k: usize, operator: &Operator, at: Instant, motion: Motion, context: &Context<'_>) -> Option<f64> {
        let offset = self.offset_at(k, at, motion, context)?;
        operator.envelope.level(at, offset)
    }

    /// An operator's gain at an instant from a state of it (MATH §3.4, §4.6,
    /// §7.2, §8.5): 0 where it is silent or its level lies below the end level
    /// of `eg.end_db`, else
    /// the gain of its level lowered by the amplitude modulation and the EG
    /// bias, times its switch's factor.
    fn gain(&self, k: usize, operator: &Operator, at: Instant, motion: Motion, context: &Context<'_>) -> f64 {
        let Some(level) = self.level(k, operator, at, motion, context) else {
            return 0.0;
        };
        let (patch, configuration) = (context.patch, context.configuration);
        if level < end_level(configuration.end_db) {
            return 0.0;
        }
        let lfo = self.lfo(context);
        let depth = modulation(patch.lfo.amplitude_depth, lfo.fade(at), context.amplitude_controllers);
        let kept = kept(context.bias, depth, lfo.wave(at, patch.lfo.wave));
        let lowered = lowering(
            configuration.am_max_steps,
            patch.operators[k].amplitude_sensitivity,
            kept,
        );
        finite(gain(finite(level - lowered)) * operator.switch.factor(at, glide(context)))
    }

    /// An operator's θ in turns and its gain g at an instant from a state of it.
    fn value(&self, k: usize, operator: &Operator, at: Instant, motion: Motion, context: &Context<'_>) -> (f64, f64) {
        (operator.phase.turns(), self.gain(k, operator, at, motion, context))
    }

    /// Its operators' θ and g at τ samples into the last sample that it
    /// integrated, into `out` (see [`Voice::within`]).
    fn within(&mut self, samples: f64, out: &mut [(f64, f64)], context: &Context<'_>) {
        let (patch, configuration) = (context.patch, context.configuration);
        let count = patch.operators.len().min(out.len()).min(self.operators.len());
        let rate = configuration.sample_rate;
        let from = self.from;
        if samples <= 0.0 {
            for (k, slot) in out.iter_mut().enumerate().take(count) {
                *slot = self.value(k, &self.saved[k], from, self.saved_motion, context);
            }
            return;
        }
        let at = from.after(samples.min(1.0)).unwrap_or(from);
        if samples >= 1.0 {
            for (k, slot) in out.iter_mut().enumerate().take(count) {
                *slot = self.value(k, &self.operators[k], at, self.motion, context);
            }
            return;
        }
        let start = self.saved_motion;
        let lfo = self.lfo.as_ref().unwrap_or(context.lfo);
        let moving = signals(start, lfo, context);
        pieces::span(&mut self.span_parts, from, samples, &moving);
        rates_keys(
            &mut self.span_keys,
            (self.transposed, self.scaling),
            from,
            samples,
            &moving,
            context,
        );
        for (k, slot) in out.iter_mut().enumerate().take(count) {
            let compiled = &patch.operators[k];
            let mut operator = self.saved[k];
            operator
                .phase
                .advance(increment(&self.span_parts, compiled, self.constant(k, context), rate));
            operator.envelope.run(
                from,
                &self.span_keys,
                &compiled.envelope,
                &configuration.envelope,
                |instant| self.offset_at(k, instant, start, context).unwrap_or(SILENT),
                |_| {},
            );
            *slot = self.value(k, &operator, at, start, context);
        }
    }
}

/// One voice of a part (MATH §13), a slot that keeps what its last note
/// leaves for the next (§13.2).
#[derive(Debug, Clone)]
pub struct Voice {
    /// What its signals keep.
    state: State,
    /// What its feedback keeps (MATH §9.6, §9.7).
    memory: Memory,
    /// The graph that it keeps under `graph.algorithm_scope = voice`, its
    /// part's at its last note's key-on (MATH §8.3), its room taken when the
    /// engine is prepared.
    plan: Plan,
    /// Its operators' outputs at its last instant.
    outputs: Vec<f64>,
    /// Its operators' θ and g at its last instant.
    values: Vec<(f64, f64)>,
    /// The parts of the last sample (MATH §2.5).
    parts: Parts,
    /// The rates' key over it (MATH §4.7).
    keys: Keys,
    /// The samples, up to its last, at which every operator's gain has been 0
    /// since the last at which one was not (MATH §13.4).
    silent: f64,
}

impl Voice {
    /// A voice of so many operators that has had no note, from an instant on,
    /// the budget of a sample's parts `osc.max_pieces`: its memory taken now,
    /// as the engine is prepared and never on the audio thread (MATH §13.1),
    /// or none where it cannot be had.
    #[must_use]
    pub fn new(operators: usize, max_pieces: u32, at: Instant) -> Option<Self> {
        let operator = Operator {
            phase: Phase::ZERO,
            envelope: Envelope::new(&RESTING, SILENT, at),
            offset: None,
            ramp: None,
            switch: Ramp::settled(true, at),
            step: 0.0,
        };
        let motion = Motion {
            envelope: PitchEnvelope::new(&CENTRED, at),
            portamento: Portamento::Still,
        };
        Some(Self {
            state: State {
                note: None,
                transposed: 0.0,
                scaling: 0.0,
                held: false,
                ended: None,
                lfo: None,
                operators: filled(operators, operator)?,
                saved: filled(operators, operator)?,
                motion,
                saved_motion: motion,
                from: at,
                span_parts: Parts::new(max_pieces)?,
                span_keys: Keys::new(max_pieces)?,
            },
            memory: Memory::new(operators)?,
            plan: Plan::room(operators)?,
            outputs: filled(operators, 0.0)?,
            values: filled(operators, (0.0, 0.0))?,
            parts: Parts::new(max_pieces)?,
            keys: Keys::new(max_pieces)?,
            silent: 0.0,
        })
    }

    /// The note it plays or played last, none before its first.
    #[must_use]
    pub const fn note(&self) -> Option<Note> {
        self.state.note
    }

    /// k′, the key of its note after transposition and the note shift (MATH
    /// §2.1).
    #[must_use]
    pub const fn transposed(&self) -> f64 {
        self.state.transposed
    }

    /// Whether its key or the sustain pedal holds it.
    #[must_use]
    pub const fn held(&self) -> bool {
        self.state.held
    }

    /// Where its last note ended (MATH §13.3), none while it sounds and before
    /// its first note.
    #[must_use]
    pub const fn ending(&self) -> Option<Instant> {
        self.state.ended
    }

    /// Its operators' outputs at its last instant, OP1 first.
    #[must_use]
    pub fn outputs(&self) -> &[f64] {
        &self.outputs
    }

    /// The stage of an operator's envelope, none beyond its operators.
    #[must_use]
    pub fn stage(&self, operator: usize) -> Option<Stage> {
        self.state.operators.get(operator).map(|state| state.envelope.stage())
    }

    /// The level of its pitch envelope in octaves at an instant at or after its
    /// last sample's end (MATH §5).
    #[must_use]
    pub fn pitch_envelope(&self, at: Instant, context: &Context<'_>) -> f64 {
        self.state.motion.at(at, context).envelope.level(at)
    }

    /// What an operator gives at an instant at or after its last sample's end,
    /// before its next sample: its θ at its last instant, and its gain, level
    /// and pitch at the instant, as MATH §16's suites read them; none beyond
    /// its operators or before its first note.
    #[must_use]
    pub fn probe(&self, k: usize, at: Instant, context: &Context<'_>) -> Option<Probe> {
        let state = &self.state;
        if state.note.is_none() || !state.holds(context) {
            return None;
        }
        let operator = state.operators.get(k)?;
        let motion = state.motion.at(at, context);
        let lfo = state.lfo_pitch(at, context);
        let pitch = match context.patch.operators.get(k)?.mode {
            Mode::Ratio => {
                let moved = finite(motion.envelope.level(at) + motion.portamento.octaves(at));
                finite(state.constant(k, context) + finite(moved + lfo))
            }
            Mode::Fixed => finite(state.constant(k, context) + lfo),
        };
        Some(Probe {
            phase: operator.phase.turns(),
            gain: state.gain(k, operator, at, motion, context),
            level: state.level(k, operator, at, motion, context),
            pitch,
        })
    }

    /// A note's key-on at an instant (MATH §4.5, §2.5, §13.2), the part's m-th
    /// note, from 0: its key after transposition and its scaling key; the
    /// pitch envelope from L4 (§5) and the portamento that the part gives it
    /// (§7.3); under `lfo.scope = voice` its own LFO from there (§6.5); each
    /// operator's offset, its phase as OSC KEY SYNC and `osc.free_phase_start`
    /// say, and its envelope's segment 0 from where it is.
    ///
    /// A voice taken while it sounds goes on from where it is: its envelopes
    /// from their levels, its phases without OSC KEY SYNC, and its feedback
    /// from its state (§9.6). A new voice, whose last note has ended or that
    /// has had none, starts its envelopes from T(L4) at the new offsets, its
    /// phases without OSC KEY SYNC run on from its last note's end at their
    /// last increments, and its feedback anew (§9.6, §9.7); a voice's first
    /// note settles its switches at its patch's. The note's offsets act at
    /// once, a change's ramp under way ending there (§4.7), and under
    /// `graph.algorithm_scope = voice` the voice takes its part's graph, the
    /// feedback state of each operator whose component that alters starting
    /// anew where it sounds (§8.3). Nothing for a voice without room for the
    /// patch.
    pub fn key_on(&mut self, at: Instant, note: Note, portamento: Portamento, nth: u64, context: &Context<'_>) {
        let state = &mut self.state;
        let scoped = context.configuration.algorithm_scope == AlgorithmScope::Voice;
        if !state.holds(context) || (scoped && !self.plan.has_room_for(&context.patch.plan)) {
            return;
        }
        let (patch, configuration) = (context.patch, context.configuration);
        let rate = configuration.sample_rate;
        let first = state.note.is_none();
        let ended = state.ended.take();
        let fresh = first || ended.is_some();
        state.transposed = transposed_key(
            note.key,
            patch.transpose,
            context.note_shift,
            configuration.transpose_octave,
            TwelveTet::PERIOD,
        );
        state.scaling = match configuration.scaling_key {
            ScalingKey::Key => state.transposed,
            ScalingKey::OnsetPitch | ScalingKey::Pitch => finite(state.transposed + note.per_note),
        };
        state.note = Some(note);
        state.held = true;
        state.motion.envelope.key_on(at, &patch.pitch_envelope, rate);
        state.motion.portamento = portamento;
        state.lfo = (configuration.lfo_scope == LfoScope::Voice).then(|| {
            let lfo = &patch.lfo;
            Lfo::of_voice(context.lfo, at, lfo.sync, lfo.delay, rate, context.streams.voices, nth)
        });
        for (k, (operator, compiled)) in state.operators.iter_mut().zip(&patch.operators).enumerate() {
            operator.offset = offset_of(compiled, state.scaling, note.velocity);
            operator.ramp = None;
            if first {
                operator.switch = Ramp::settled(compiled.on, at);
            }
            operator.phase = if patch.key_sync {
                Phase::ZERO
            } else {
                match configuration.free_phase_start {
                    FreePhaseStart::Continue => ended.map_or(operator.phase, |end| {
                        Phase::new(run_on(operator.phase.turns(), operator.step, at.samples_since(end)))
                    }),
                    FreePhaseStart::Zero => Phase::ZERO,
                    FreePhaseStart::Random => {
                        let number = u64::try_from(k + 1).unwrap_or(u64::MAX);
                        let index = Streams::phase_index(nth, number);
                        Phase::new(random::phase(random::value(context.streams.phases, index)))
                    }
                }
            };
        }
        let key = state.rates_key(at, state.motion, context);
        for (k, compiled) in patch.operators.iter().enumerate() {
            let offset = state.offset_at(k, at, state.motion, context).unwrap_or(SILENT);
            let operator = &mut state.operators[k];
            if fresh {
                operator.envelope = Envelope::new(&compiled.envelope, offset, at);
            }
            operator
                .envelope
                .key_on(at, &compiled.envelope, &configuration.envelope, key, offset);
        }
        if scoped {
            if !fresh {
                self.memory.renew_changed(&self.plan, &patch.plan);
            }
            self.plan.copy_from(&patch.plan);
        }
        if fresh {
            self.memory.renew();
            self.silent = 0.0;
        }
    }

    /// The note's key-off at an instant, once neither its key nor the sustain
    /// pedal holds it (MATH §4.5): each envelope's segment 3 from where it is,
    /// and the pitch envelope's (§5). Nothing where no key holds it.
    pub fn key_off(&mut self, at: Instant, context: &Context<'_>) {
        let state = &mut self.state;
        if !state.sounds(context) || !state.held {
            return;
        }
        let (patch, configuration) = (context.patch, context.configuration);
        state.held = false;
        let key = state.rates_key(at, state.motion, context);
        for (k, compiled) in patch.operators.iter().enumerate() {
            let offset = state.offset_at(k, at, state.motion, context).unwrap_or(SILENT);
            state.operators[k]
                .envelope
                .key_off(at, &compiled.envelope, &configuration.envelope, key, offset);
        }
        state
            .motion
            .envelope
            .key_off(at, &patch.pitch_envelope, configuration.sample_rate);
    }

    /// An operator switch acting at an instant (MATH §8.5): its factor moves
    /// from where it is toward 1 or 0 over `param.level_glide_s`; a switch to
    /// where it moves already changes nothing.
    pub fn switch(&mut self, at: Instant, operator: usize, on: bool, context: &Context<'_>) {
        let glide = glide(context);
        if let Some(state) = self.state.operators.get_mut(operator)
            && on != state.switch.on
        {
            state.switch = Ramp {
                from: state.switch.factor(at, glide),
                on,
                start: at,
            };
        }
    }

    /// A change of its note's per-note pitch kₙₒₜₑ, in keys (DESIGN §7.3), at the
    /// instant of the sample at which it acts: the operators' pitches take it
    /// from there on (MATH §2.2–§2.4), the phases going on (§4.7), and under
    /// `scaling.key = pitch` the scaling key with it (§2.6), while
    /// `onset-pitch` keeps the one at the onset. Nothing for a voice that does
    /// not sound.
    pub fn set_per_note(&mut self, per_note: f64, context: &Context<'_>) {
        let state = &mut self.state;
        if !state.sounds(context) {
            return;
        }
        if let Some(note) = state.note.as_mut() {
            note.per_note = per_note;
        }
        if context.configuration.scaling_key == ScalingKey::Pitch {
            state.scaling = finite(state.transposed + per_note);
        }
    }

    /// The key at which its pitch lies at an instant, from which a glide of the
    /// part's portamento starts (MATH §7.3): its key after transposition and
    /// its portamento's offset there in keys, twelve to an octave under 12-TET,
    /// its glide taken to the instant from where it is, as the walk takes it;
    /// none before its first note. The fused step rounds within u of itself,
    /// beside the offset's error (`crate::portamento`).
    #[must_use]
    pub fn heard(&self, at: Instant, context: &Context<'_>) -> Option<f64> {
        let portamento = self.portamento(at, context)?;
        Some(finite(12.0_f64.mul_add(portamento.octaves(at), self.state.transposed)))
    }

    /// Its portamento at an instant (MATH §7.3), taken there from where it is,
    /// as the walk takes it, so that a glissando that goes on toward another
    /// target keeps its progress; none before its first note.
    #[must_use]
    pub fn portamento(&self, at: Instant, context: &Context<'_>) -> Option<Portamento> {
        let state = &self.state;
        state.note?;
        Some(state.motion.at(at, context).portamento)
    }

    /// A change of its note's key at an instant without a key event (MATH
    /// §7.3): a poly part's follow, which takes a voice whose key is up toward
    /// another note's pitch, its own per-note pitch kept, and a mono part's
    /// change of its sounding key, which gives the per-note pitch of the key
    /// that sounds now. Its key after transposition becomes the new key's, its
    /// portamento from there on the one given, and its envelopes, its pitch
    /// envelope and its phases go on; under `scaling.key = pitch` its scaling
    /// key moves with its pitch, the other two keeping the onset's (§2.6).
    /// Nothing before its first note.
    pub fn retarget(
        &mut self,
        at: Instant,
        key: f64,
        per_note: Option<f64>,
        portamento: Portamento,
        context: &Context<'_>,
    ) {
        let state = &mut self.state;
        let Some(note) = state.note.as_mut() else {
            return;
        };
        note.key = key;
        if let Some(per_note) = per_note {
            note.per_note = per_note;
        }
        let per_note = note.per_note;
        let (patch, configuration) = (context.patch, context.configuration);
        state.transposed = transposed_key(
            key,
            patch.transpose,
            context.note_shift,
            configuration.transpose_octave,
            TwelveTet::PERIOD,
        );
        if configuration.scaling_key == ScalingKey::Pitch {
            state.scaling = finite(state.transposed + per_note);
        }
        let mut motion = state.motion.at(at, context);
        motion.portamento = portamento;
        state.motion = motion;
    }

    /// A change of its part's compiled patch at an instant (MATH §0.3, §4.7,
    /// §5, §8.3, §8.5), the part's context before the change, `old`, and after
    /// it, `new`. A voice that has had a note: its switches move toward the
    /// new patch's (§8.5), and its portamento takes a new TIME or glissando
    /// from the pitch there (§7.3). One that sounds besides: its key after
    /// transposition moves with the transpose and the note shift, and its
    /// scaling key with it (§2.1, §2.6), while one that does not keeps the
    /// pitch where it was; each operator whose offset's law changes ramps
    /// from its offset there to the new law's over `param.level_glide_s`, in
    /// T, through the end level of `eg.end_db` from or to silence (§4.7); each
    /// envelope whose parameters or rates' key change, and the pitch envelope
    /// where its own do, are re-anchored there (§4.7, §5); its own LFO takes a
    /// new speed or DELAY under `lfo.scope = voice` (§6.1, §6.5); and under
    /// `graph.algorithm_scope = patch` the feedback state of each operator
    /// whose component the new graph alters starts anew, while under `voice`
    /// the voice keeps its graph (§8.3). Nothing for a voice that has had no
    /// note, nor for one without room for either patch.
    pub fn change(&mut self, at: Instant, old: &Context<'_>, new: &Context<'_>) {
        let Some(note) = self.state.note else {
            return;
        };
        if !self.state.holds(old) || !self.state.holds(new) {
            return;
        }
        for (k, compiled) in new.patch.operators.iter().enumerate() {
            self.switch(at, k, compiled.on, new);
        }
        let state = &mut self.state;
        let configuration = new.configuration;
        let rate = configuration.sample_rate;
        let before = state.motion.at(at, old);
        let mut after = before;
        let (from, to) = (&old.patch.function, &new.patch.function);
        if (from.portamento_speed, from.glissando) != (to.portamento_speed, to.glissando) {
            after.portamento = retimed(after.portamento, at, to, configuration);
        }
        if !state.sounds(new) {
            state.motion = after;
            return;
        }
        let transposed = transposed_key(
            note.key,
            new.patch.transpose,
            new.note_shift,
            configuration.transpose_octave,
            TwelveTet::PERIOD,
        );
        let scaling = match configuration.scaling_key {
            ScalingKey::Pitch => finite(transposed + note.per_note),
            ScalingKey::Key | ScalingKey::OnsetPitch => finite(state.scaling + finite(transposed - state.transposed)),
        };
        if old.patch.pitch_envelope != new.patch.pitch_envelope {
            after.envelope.change(at, &new.patch.pitch_envelope, rate);
        }
        let key = state.rates_key(at, before, old);
        let end = end_level(configuration.end_db);
        let glide = glide(new);
        for (k, compiled) in new.patch.operators.iter().enumerate() {
            let offset = state.offset_at(k, at, before, old);
            let previous = state.law(&old.patch.operators[k], state.scaling, at, before, old);
            let next = state.law(compiled, scaling, at, after, new);
            let operator = &mut state.operators[k];
            if next.map(f64::to_bits) != previous.map(f64::to_bits) {
                operator.ramp = LevelRamp::between(offset, next, &operator.envelope, at, end, glide);
            }
            operator.offset = offset_of(compiled, scaling, note.velocity);
        }
        state.motion = after;
        state.transposed = transposed;
        state.scaling = scaling;
        let moved = state.rates_key(at, after, new);
        for (k, compiled) in new.patch.operators.iter().enumerate() {
            if old.patch.operators[k].envelope != compiled.envelope || !same(moved, key) {
                state.operators[k]
                    .envelope
                    .change(at, &compiled.envelope, &configuration.envelope, moved);
            }
        }
        if let Some(lfo) = state.lfo.as_mut() {
            let (was, is) = (&old.patch.lfo, &new.patch.lfo);
            if !same(was.speed, is.speed) {
                lfo.set_speed(at, is.speed, rate);
            }
            if !same(was.delay, is.delay) {
                lfo.set_delay(at, is.delay, rate);
            }
        }
        if configuration.algorithm_scope == AlgorithmScope::Patch && old.patch.plan != new.patch.plan {
            self.memory.renew_changed(&old.patch.plan, &new.patch.plan);
        }
    }

    /// The graph that it plays (MATH §8.3): its own under
    /// `graph.algorithm_scope = voice`, its part's under `patch`.
    fn plan<'s>(&'s self, context: &Context<'s>) -> &'s Plan {
        match context.configuration.algorithm_scope {
            AlgorithmScope::Voice => &self.plan,
            AlgorithmScope::Patch => &context.patch.plan,
        }
    }

    /// The level T at an instant of its loudest carrier, its offset O included
    /// and the amplitude modulation not, as `voice.steal = quietest` compares
    /// voices (MATH §13.2); −∞ where every carrier is silent, or for a voice
    /// that does not sound.
    #[must_use]
    pub fn loudest(&self, at: Instant, context: &Context<'_>) -> f64 {
        let state = &self.state;
        if !state.sounds(context) {
            return f64::NEG_INFINITY;
        }
        self.plan(context)
            .carriers()
            .iter()
            .filter_map(|&carrier| {
                let operator = state.operators.get(carrier)?;
                state.level(carrier, operator, at, state.motion, context)
            })
            .fold(f64::NEG_INFINITY, f64::max)
    }

    /// Records the end of its note at an instant (MATH §13.3): it sounds no
    /// more, and its phases run on from there at their last increments until
    /// its next note (§2.5).
    pub fn end(&mut self, at: Instant) {
        self.state.ended = Some(at);
        self.state.held = false;
    }

    /// Integrates the sample that runs one engine sample from an instant by the
    /// laws that held over it (MATH §0.3): each operator's phase by its
    /// increment over the sample's pieces (§2.5), its envelope run over the
    /// sample at the rates' key (§4.7), the pitch envelope and the portamento
    /// taken to the sample's end, and the relaxation's lags (§9.6), which take
    /// θ and g at instants within the sample from [`Voice::within`]. Nothing
    /// for a voice that does not sound.
    pub fn advance(&mut self, from: Instant, context: &Context<'_>) {
        let state = &mut self.state;
        if !state.sounds(context) {
            return;
        }
        let (patch, configuration) = (context.patch, context.configuration);
        let count = patch.operators.len();
        let rate = configuration.sample_rate;
        let to = from.after(1.0).unwrap_or(from);
        state.from = from;
        state.saved[..count].copy_from_slice(&state.operators[..count]);
        let start = state.motion;
        state.saved_motion = start;
        let lfo = state.lfo.as_ref().unwrap_or(context.lfo);
        let moving = signals(start, lfo, context);
        pieces::sample(&mut self.parts, from, &moving);
        rates_keys(
            &mut self.keys,
            (state.transposed, state.scaling),
            from,
            1.0,
            &moving,
            context,
        );
        for (k, compiled) in patch.operators.iter().enumerate() {
            let increment = increment(&self.parts, compiled, state.constant(k, context), rate);
            let mut envelope = state.operators[k].envelope;
            envelope.run(
                from,
                &self.keys,
                &compiled.envelope,
                &configuration.envelope,
                |at| state.offset_at(k, at, start, context).unwrap_or(SILENT),
                |_| {},
            );
            let operator = &mut state.operators[k];
            operator.phase.advance(increment);
            operator.step = increment;
            operator.envelope = envelope;
        }
        state.motion = start.at(to, context);
        let plan = match configuration.algorithm_scope {
            AlgorithmScope::Voice => &self.plan,
            AlgorithmScope::Patch => &patch.plan,
        };
        plan.integrate(
            &mut self.memory,
            |samples, out| state.within(samples, out, context),
            &self.outputs,
            &configuration.feedback,
            rate,
        );
    }

    /// Its operators' θ in turns and gains g at τ samples into the last sample
    /// that it integrated, into `out` by their indices, as the relaxation takes
    /// them (MATH §9.6): at 0 its states at the sample's start, at 1 at its end
    /// before the events there, and between them its states at the start run
    /// over the part of the sample up to τ, as the sample itself is walked and
    /// run ([`pieces::span`], [`Keys::span`]).
    pub fn within(&mut self, samples: f64, out: &mut [(f64, f64)], context: &Context<'_>) {
        if self.state.holds(context) {
            self.state.within(samples, out, context);
        }
    }

    /// Its output at an instant after the events there (MATH §1, §8.2, §14):
    /// its operators' θ and g there through its graph, the feedback as
    /// `feedback.model` says, and the carriers' sum divided by their count or
    /// its square root as `output.carrier_normalization` says, switched off or
    /// not; 0 for a voice that does not sound. Its count of silent samples goes
    /// on, and once it reaches ⌈`engine.idle_s`·fₑ⌉ its feedback's states are
    /// set to 0 (§13.4).
    pub fn output(&mut self, at: Instant, context: &Context<'_>) -> f64 {
        let state = &self.state;
        if !state.sounds(context) {
            return 0.0;
        }
        let (patch, configuration) = (context.patch, context.configuration);
        let count = patch.operators.len();
        for k in 0..count {
            self.values[k] = state.value(k, &state.operators[k], at, state.motion, context);
        }
        let values = &self.values;
        let plan = match configuration.algorithm_scope {
            AlgorithmScope::Voice => &self.plan,
            AlgorithmScope::Patch => &patch.plan,
        };
        let sum = plan.output(
            &mut self.memory,
            |k| values[k],
            &configuration.feedback,
            &mut self.outputs,
        );
        if values[..count].iter().all(|&(_, gain)| gain == 0.0) {
            self.silent += 1.0;
            if self.silent >= (configuration.idle_s * configuration.sample_rate).ceil() {
                self.memory.zero();
            }
        } else {
            self.silent = 0.0;
        }
        let carriers = u32::try_from(plan.carriers().len()).map_or(f64::MAX, f64::from);
        match configuration.normalization {
            Normalization::Count if carriers > 0.0 => sum / carriers,
            Normalization::SqrtCount if carriers > 0.0 => sum / carriers.sqrt(),
            Normalization::None | Normalization::Count | Normalization::SqrtCount => sum,
        }
    }

    /// Whether its note has ended at an instant (MATH §13.3): no key or pedal
    /// holds it, and each carrier of its graph is switched off, its factor at
    /// 0, or in segment 3 or the rest after it with its level, O included and
    /// the amplitude modulation not, below the end level of `eg.end_db`. A
    /// voice that does not sound
    /// has ended.
    #[must_use]
    pub fn ended(&self, at: Instant, context: &Context<'_>) -> bool {
        let state = &self.state;
        if !state.sounds(context) {
            return true;
        }
        if state.held {
            return false;
        }
        let end = end_level(context.configuration.end_db);
        let glide = glide(context);
        self.plan(context).carriers().iter().all(|&carrier| {
            let Some(operator) = state.operators.get(carrier) else {
                return true;
            };
            let off = operator.switch.factor(at, glide) <= 0.0;
            let released = matches!(operator.envelope.stage(), Stage::Segment(3) | Stage::Resting);
            let below = state
                .level(carrier, operator, at, state.motion, context)
                .is_none_or(|level| level < end);
            off || (released && below)
        })
    }

    /// Sets its feedback's states below 2⁻⁵⁰⁰ in magnitude to 0, as MATH §13.4
    /// does at every 64th engine sample.
    pub fn flush(&mut self) {
        self.memory.flush();
    }
}
