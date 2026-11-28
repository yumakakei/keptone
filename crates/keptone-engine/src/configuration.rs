// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The engine's configuration: the settings of docs/MATH.md §17 that the
//! engine takes, typed, from those of the native state
//! (`keptone_model::settings`), with the host's rate and the engine's, fixed
//! when the engine is prepared (DESIGN §5); and the settings whose values
//! milestone M3 does not compute, which it refuses by name, so that no render
//! differs silently from MATH (DESIGN §16).

use keptone_model::settings::{self, Settings, Value};

use crate::eg::RateLaw;
use crate::eg::envelope;
use crate::pitch::{FixedDetune, TransposeOctave};

/// What a setting is: on or off, a choice's index, a number or an integer.
fn value(settings: &Settings, name: &'static str) -> Value {
    settings
        .get(settings::number_of(name).unwrap_or(u16::MAX))
        .unwrap_or(Value::Automatic)
}

/// A switch's value, or its default where the settings give it no switch.
fn switch(settings: &Settings, name: &'static str, default: bool) -> bool {
    match value(settings, name) {
        Value::Switch(on) => on,
        _ => default,
    }
}

/// A choice's index.
fn choice(settings: &Settings, name: &'static str) -> u8 {
    match value(settings, name) {
        Value::Choice(index) => index,
        _ => 0,
    }
}

/// A number's value, or its default where the settings give it no number.
fn number(settings: &Settings, name: &'static str, default: f64) -> f64 {
    match value(settings, name) {
        Value::Number(number) => number,
        _ => default,
    }
}

/// An integer's value, or its default where the settings give it no integer.
fn integer(settings: &Settings, name: &'static str, default: u64) -> u64 {
    match value(settings, name) {
        Value::Integer(integer) => integer,
        _ => default,
    }
}

/// A count of a setting that the engine holds in memory, at most the largest
/// that `usize` holds.
fn count(settings: &Settings, name: &'static str, default: u64) -> usize {
    usize::try_from(integer(settings, name, default)).unwrap_or(usize::MAX)
}

/// A count that a solver or a sample's integral takes, at most `u32::MAX`.
fn iterations(settings: &Settings, name: &'static str, default: u64) -> u32 {
    u32::try_from(integer(settings, name, default)).unwrap_or(u32::MAX)
}

/// Why the engine refuses a configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Refusal {
    /// A setting whose value milestone M3 does not compute (DESIGN §16), by
    /// its name, with the value it has.
    Setting {
        /// The setting's name, as MATH §17 gives it.
        name: &'static str,
        /// Its value.
        value: Value,
    },
    /// A host rate that is not a finite number above 0.
    HostRate(f64),
    /// The memory that the settings ask for, the voices of every part (MATH
    /// §13.1), which the engine takes when it is prepared and never on the
    /// audio thread, and which cannot be had.
    Memory,
}

/// Where a voice's oscillators start when OSC KEY SYNC is off (MATH §2.5):
/// `osc.free_phase_start`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreePhaseStart {
    /// Each operator's phase goes on from the slot's last note.
    Continue,
    /// Each starts at 0.
    Zero,
    /// Each takes a phase from the part's stream of phases (MATH §6.3).
    Random,
}

/// Whose LFO a voice takes (MATH §6.5): `lfo.scope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LfoScope {
    /// The part's, which its voices share.
    Part,
    /// Each voice's own.
    Voice,
}

/// Whose graph a sounding voice plays when its part's patch changes (MATH
/// §8.3): `graph.algorithm_scope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlgorithmScope {
    /// The new patch's.
    Patch,
    /// The graph that the voice started with.
    Voice,
}

/// What an operator's feedback does where its loop gain reaches 1 (MATH §9.6):
/// `feedback.model`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackModel {
    /// The lag of the fed-back signal.
    Relaxation,
    /// The DX7's recurrence at the engine rate.
    Dx7Discrete,
    /// The point solution with the loop gain held below 1.
    Clamp,
}

/// The settings of the feedback (MATH §9).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Feedback {
    /// `feedback.model`.
    pub model: FeedbackModel,
    /// `feedback.tau_s`.
    pub tau_s: f64,
    /// `feedback.beta_clamp`.
    pub beta_clamp: f64,
    /// `feedback.max_iterations`.
    pub max_iterations: u32,
    /// `feedback.relaxation_onset`.
    pub relaxation_onset: f64,
    /// `feedback.max_substeps`.
    pub max_substeps: u32,
}

impl Feedback {
    /// The feedback's settings of a set of settings.
    fn new(settings: &Settings) -> Self {
        Self {
            model: match choice(settings, "feedback.model") {
                0 => FeedbackModel::Relaxation,
                1 => FeedbackModel::Dx7Discrete,
                _ => FeedbackModel::Clamp,
            },
            tau_s: number(settings, "feedback.tau_s", 1.5 / 49096.0),
            beta_clamp: number(settings, "feedback.beta_clamp", 0.999),
            max_iterations: iterations(settings, "feedback.max_iterations", 16),
            relaxation_onset: number(settings, "feedback.relaxation_onset", 0.9),
            max_substeps: iterations(settings, "feedback.max_substeps", 64),
        }
    }
}

/// The first setting, in MATH §17's order, whose value M3 does not compute
/// (DESIGN §16): each of these five must have the one value that it computes.
fn refused(settings: &Settings) -> Option<Refusal> {
    [
        ("tuning.source", Value::Choice(0)),
        ("graph.operators", Value::Integer(6)),
        ("feedback.series_beta_max", Value::Number(0.0)),
        ("adaa.order", Value::Choice(0)),
        ("engine.oversampling", Value::Integer(1)),
    ]
    .into_iter()
    .find_map(|(name, computed)| {
        let given = value(settings, name);
        (given != computed).then_some(Refusal::Setting { name, value: given })
    })
}

/// How a part takes a voice when it has none to spare (MATH §13.2):
/// `voice.steal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Steal {
    /// The DX7's: the first voice whose key is up after the one last given.
    Dx7,
    /// The voice whose note started first.
    Oldest,
    /// The voice whose loudest carrier is the quietest.
    Quietest,
    /// Of the voices whose key is up, the one released first.
    ReleasedFirst,
}

/// What a key pressed again while its voice sounds takes (MATH §13.2):
/// `voice.retrigger`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retrigger {
    /// The voice already there, restarted.
    SameVoice,
    /// A voice of its own.
    NewVoice,
}

/// Whose function data a part plays (MATH §13.5): `part.function_source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionSource {
    /// The supplement of the part's patch, where it has one.
    Patch,
    /// The part's own.
    Part,
}

/// How a voice's carriers are scaled (MATH §14): `output.carrier_normalization`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Normalization {
    /// Not at all.
    None,
    /// By the number of the graph's carriers.
    Count,
    /// By that number's square root.
    SqrtCount,
}

/// The output's channels (MATH §14): `output.channels`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channels {
    /// Two, each part placed by its pan.
    Stereo,
    /// One, every part without its pan.
    Mono,
}

/// The key that the keyboard scaling of the level takes (MATH §2.6):
/// `scaling.key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalingKey {
    /// The key after transposition.
    Key,
    /// The pitch that the tuning gives the key at key-on.
    OnsetPitch,
    /// The voice's pitch at each sample.
    Pitch,
}

/// What bends, glides and the per-note pitch move (MATH §2.1):
/// `tuning.modulation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modulation {
    /// The pitch, beside the tuning.
    Pitch,
    /// The note value, through the tuning.
    Keys,
}

/// The key that the keyboard scaling of the rates takes (MATH §2.6):
/// `scaling.rate_key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateKey {
    /// The pitch of the key after transposition with its portamento and the
    /// pitch envelope, at each sample.
    Frequency,
    /// The level's key.
    Level,
}

/// The engine's configuration, fixed when it is prepared.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Configuration {
    /// The host's rate fₛ.
    pub host_rate: f64,
    /// The engine's rate fₑ = M·fₛ, M being `engine.oversampling`, 1 in M3.
    pub sample_rate: f64,
    /// `tuning.a4_hz`.
    pub a4_hz: f64,
    /// `tuning.transpose_octave`.
    pub transpose_octave: TransposeOctave,
    /// `tuning.modulation`.
    pub modulation: Modulation,
    /// `osc.fixed_detune`.
    pub fixed_detune: FixedDetune,
    /// `osc.free_phase_start`.
    pub free_phase_start: FreePhaseStart,
    /// `osc.max_pieces`.
    pub max_pieces: u32,
    /// The settings that an envelope takes, `eg.clock_hz`, `eg.rate_law`,
    /// `eg.attack_jump` and `eg.static_segments`, with the engine's rate.
    pub envelope: envelope::Settings,
    /// `eg.end_db`.
    pub end_db: f64,
    /// `param.level_glide_s`.
    pub level_glide_s: f64,
    /// `scaling.key`.
    pub scaling_key: ScalingKey,
    /// `scaling.rate_key`.
    pub rate_key: RateKey,
    /// `lfo.scope`.
    pub lfo_scope: LfoScope,
    /// `lfo.seed`.
    pub lfo_seed: u64,
    /// `am.max_steps`.
    pub am_max_steps: f64,
    /// `graph.algorithm_scope`.
    pub algorithm_scope: AlgorithmScope,
    /// `graph.operators`.
    pub operators: usize,
    /// The feedback's settings.
    pub feedback: Feedback,
    /// `voice.count`.
    pub voice_count: usize,
    /// `voice.steal`.
    pub steal: Steal,
    /// `voice.retrigger`.
    pub retrigger: Retrigger,
    /// `engine.idle_s`.
    pub idle_s: f64,
    /// `part.count`.
    pub part_count: usize,
    /// `part.function_source`.
    pub function_source: FunctionSource,
    /// `output.gain`.
    pub output_gain: f64,
    /// `output.carrier_normalization`.
    pub normalization: Normalization,
    /// `output.channels`.
    pub channels: Channels,
}

impl Configuration {
    /// The configuration of a host rate and a set of settings, or the reason it
    /// is refused: a host rate that is not finite and above 0, or the first in
    /// MATH §17's order of `engine.oversampling` above 1, `adaa.order` above
    /// 0, `feedback.series_beta_max` above 0, `tuning.source` other than
    /// `12-tet` and `graph.operators` other than 6, whose values M3 does not
    /// compute (DESIGN §16).
    ///
    /// # Errors
    ///
    /// The [`Refusal`] that names the setting refused, or the host rate.
    pub fn new(settings: &Settings, host_rate: f64) -> Result<Self, Refusal> {
        if !(host_rate.is_finite() && host_rate > 0.0) {
            return Err(Refusal::HostRate(host_rate));
        }
        if let Some(refusal) = refused(settings) {
            return Err(refusal);
        }
        // M = 1 in M3, which refuses every other.
        let sample_rate = host_rate;
        Ok(Self {
            host_rate,
            sample_rate,
            a4_hz: number(settings, "tuning.a4_hz", 440.0),
            transpose_octave: match choice(settings, "tuning.transpose_octave") {
                0 => TransposeOctave::Keys,
                _ => TransposeOctave::Period,
            },
            modulation: match choice(settings, "tuning.modulation") {
                0 => Modulation::Pitch,
                _ => Modulation::Keys,
            },
            fixed_detune: match choice(settings, "osc.fixed_detune") {
                0 => FixedDetune::Msfa,
                1 => FixedDetune::Symmetric,
                _ => FixedDetune::None,
            },
            free_phase_start: match choice(settings, "osc.free_phase_start") {
                0 => FreePhaseStart::Continue,
                1 => FreePhaseStart::Zero,
                _ => FreePhaseStart::Random,
            },
            max_pieces: iterations(settings, "osc.max_pieces", 256),
            envelope: envelope::Settings {
                sample_rate,
                clock_hz: number(settings, "eg.clock_hz", 49096.0),
                rate_law: match choice(settings, "eg.rate_law") {
                    0 => RateLaw::Dx7,
                    _ => RateLaw::Exponential,
                },
                attack_jump: switch(settings, "eg.attack_jump", true),
                waits: choice(settings, "eg.static_segments") == 0,
            },
            end_db: number(settings, "eg.end_db", -200.0),
            level_glide_s: number(settings, "param.level_glide_s", 0.002),
            scaling_key: match choice(settings, "scaling.key") {
                0 => ScalingKey::Key,
                1 => ScalingKey::OnsetPitch,
                _ => ScalingKey::Pitch,
            },
            rate_key: match choice(settings, "scaling.rate_key") {
                0 => RateKey::Frequency,
                _ => RateKey::Level,
            },
            lfo_scope: match choice(settings, "lfo.scope") {
                0 => LfoScope::Part,
                _ => LfoScope::Voice,
            },
            lfo_seed: integer(settings, "lfo.seed", 0),
            am_max_steps: number(settings, "am.max_steps", 2048.0),
            algorithm_scope: match choice(settings, "graph.algorithm_scope") {
                0 => AlgorithmScope::Patch,
                _ => AlgorithmScope::Voice,
            },
            operators: count(settings, "graph.operators", 6),
            feedback: Feedback::new(settings),
            voice_count: count(settings, "voice.count", 16),
            steal: match choice(settings, "voice.steal") {
                0 => Steal::Dx7,
                1 => Steal::Oldest,
                2 => Steal::Quietest,
                _ => Steal::ReleasedFirst,
            },
            retrigger: match choice(settings, "voice.retrigger") {
                0 => Retrigger::SameVoice,
                _ => Retrigger::NewVoice,
            },
            idle_s: number(settings, "engine.idle_s", 0.5),
            part_count: count(settings, "part.count", 2),
            function_source: match choice(settings, "part.function_source") {
                0 => FunctionSource::Patch,
                _ => FunctionSource::Part,
            },
            output_gain: number(settings, "output.gain", 0.5),
            normalization: match choice(settings, "output.carrier_normalization") {
                0 => Normalization::None,
                1 => Normalization::Count,
                _ => Normalization::SqrtCount,
            },
            channels: match choice(settings, "output.channels") {
                0 => Channels::Stereo,
                _ => Channels::Mono,
            },
        })
    }
}
