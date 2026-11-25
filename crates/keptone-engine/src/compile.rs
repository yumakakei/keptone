// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! A part's compiled patch (DESIGN §6.1): what its voices play, its patch's
//! voice and its function data as the engine's laws take them, built on the
//! main thread from `keptone-model`'s typed values alone, so that the engine
//! takes no raw bytes or unchecked numbers (DESIGN §5). What depends on the
//! patch alone is resolved here once: the operators' frequency ratios and
//! fixed pitches, the sensitivities, the feedback's κ, the portamento's speed,
//! and the graph's strongly connected components in their order of evaluation
//! (MATH §8.2); what depends on a note, its key and velocity, a voice resolves
//! at its key-on. A supplement and a fractional scaling are held by the patch
//! and not heard until their laws are written (MATH §13.5, DESIGN §16).

use core::num::NonZeroU32;

use keptone_model::function::{Controller, Function, Polyphony, PortamentoMode};
use keptone_model::graph::{Graph, Kind};
use keptone_model::part::{KeyFilter, Part};
use keptone_model::patch::Patch;
use keptone_model::voice::{Mode, Operator, Voice, Wave};

use crate::configuration::Configuration;
use crate::control::{amplitude_sensitivity, pitch_sensitivity};
use crate::eg::envelope;
use crate::feedback::kappa;
use crate::graph::evaluation::Plan;
use crate::graph::{Component, components};
use crate::level::Side;
use crate::peg;
use crate::pitch::{fixed_pitch, ratio};
use crate::portamento;

/// An operator as its laws take it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompiledOperator {
    /// Its envelope's rates, levels and rate scaling (MATH §4).
    pub envelope: envelope::Parameters,
    /// Its output level OL (MATH §3.2).
    pub output_level: f64,
    /// Its keyboard level scaling's break point (MATH §3.2), a position on the
    /// keyboard (§0.7).
    pub break_point: f64,
    /// The scaling's curve and depth below the break point.
    pub left: Side,
    /// The scaling's curve and depth above it.
    pub right: Side,
    /// Its key velocity sensitivity KVS (MATH §3.2).
    pub velocity_sensitivity: f64,
    /// α(AMS), its sensitivity to the amplitude modulation and the EG bias
    /// (MATH §7.2).
    pub amplitude_sensitivity: f64,
    /// Whether its frequency is a ratio to the key's or a frequency of its own.
    pub mode: Mode,
    /// Its frequency ratio in the ratio mode, ρ(COARSE)(1 + FINE/100) (MATH
    /// §2.2).
    pub ratio: f64,
    /// Its pitch in the fixed mode, in octaves, its detune included (MATH §2.3).
    pub fixed_pitch: f64,
    /// Its DETUNE, 0 to 14, 7 none, which the ratio mode takes at the key's
    /// pitch (MATH §2.2).
    pub detune: f64,
    /// Whether it is switched on (MATH §8.5).
    pub on: bool,
}

impl CompiledOperator {
    /// An operator of a voice, in the configuration's fixed detune.
    fn new(operator: &Operator, configuration: &Configuration) -> Self {
        let envelope = envelope::Parameters {
            rates: operator.envelope.rates.map(keptone_model::voice::Value::get),
            levels: operator.envelope.levels.map(keptone_model::voice::Value::get),
            rate_scaling: operator.rate_scaling.get(),
        };
        let (coarse, fine, detune) = (operator.coarse.get(), operator.fine.get(), operator.detune.get());
        Self {
            envelope,
            output_level: operator.output_level.get(),
            break_point: operator.break_point.get(),
            left: Side {
                curve: operator.left_curve,
                depth: operator.left_depth.get(),
            },
            right: Side {
                curve: operator.right_curve,
                depth: operator.right_depth.get(),
            },
            velocity_sensitivity: operator.velocity_sensitivity.get(),
            amplitude_sensitivity: amplitude_sensitivity(operator.amplitude_sensitivity.get()),
            mode: operator.mode,
            ratio: ratio(coarse, fine),
            fixed_pitch: fixed_pitch(coarse, fine, detune, configuration.fixed_detune),
            detune,
            on: operator.on,
        }
    }
}

/// A feedback edge of a graph and its κ (MATH §9.1, §9.7).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeedbackEdge {
    /// The index of the operator whose output feeds back.
    pub from: usize,
    /// The index of the operator that it enters.
    pub to: usize,
    /// κ in turns per unit of output.
    pub kappa: f64,
}

/// The LFO's parameters (MATH §6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompiledLfo {
    /// Its speed.
    pub speed: f64,
    /// Its DELAY.
    pub delay: f64,
    /// Its pitch modulation depth PMD.
    pub pitch_depth: f64,
    /// Its amplitude modulation depth AMD.
    pub amplitude_depth: f64,
    /// Whether a key-on resets its phase: LFO SYNC (MATH §6.5).
    pub sync: bool,
    /// Its wave.
    pub wave: Wave,
}

/// A controller's range and what it is assigned to (MATH §7.1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompiledController {
    /// Its RANGE.
    pub range: f64,
    /// Whether it modulates the pitch.
    pub pitch: bool,
    /// Whether it modulates the amplitude.
    pub amplitude: bool,
    /// Whether it biases the envelopes.
    pub eg_bias: bool,
}

impl CompiledController {
    /// A controller of the function data.
    fn new(controller: &Controller) -> Self {
        Self {
            range: controller.range.get(),
            pitch: controller.assign.pitch,
            amplitude: controller.assign.amplitude,
            eg_bias: controller.assign.eg_bias,
        }
    }
}

/// The function data as the laws take it (MATH §2.4, §7).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompiledFunction {
    /// Whether the part plays one voice at a time (MATH §7.3).
    pub mono: bool,
    /// The pitch bend's RANGE and STEP (MATH §2.4).
    pub bend_range: f64,
    /// The pitch bend's STEP.
    pub bend_step: f64,
    /// Whether the portamento follows: FOLLOW in poly, full time in mono.
    pub follow: bool,
    /// Whether it glides with glissando.
    pub glissando: bool,
    /// κ of its TIME, or none at or below 0, where there is no glide (MATH
    /// §7.3).
    pub portamento_speed: Option<f64>,
    /// The modulation wheel, the foot controller, the breath controller and
    /// aftertouch, in that order, the order in which their depths add (MATH
    /// §7.1).
    pub controllers: [CompiledController; 4],
}

impl CompiledFunction {
    /// The function data's.
    fn new(function: &Function) -> Self {
        Self {
            mono: function.polyphony == Polyphony::Mono,
            bend_range: function.pitch_bend_range.get(),
            bend_step: function.pitch_bend_step.get(),
            follow: function.portamento_mode == PortamentoMode::Follow,
            glissando: function.glissando,
            portamento_speed: portamento::speed(function.portamento_time.get()),
            controllers: [
                CompiledController::new(&function.modulation_wheel),
                CompiledController::new(&function.foot_controller),
                CompiledController::new(&function.breath_controller),
                CompiledController::new(&function.aftertouch),
            ],
        }
    }
}

/// A part's compiled patch.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledPatch {
    /// The operators, OP1 first, by their indices in the graph.
    pub operators: Vec<CompiledOperator>,
    /// The graph: the algorithm's, with its feedback edge where FB is not 0
    /// (MATH §8.1).
    pub graph: Graph,
    /// Its strongly connected components in their order of evaluation (MATH
    /// §8.2).
    pub components: Vec<Component>,
    /// Its feedback edges and their κ.
    pub feedback: Vec<FeedbackEdge>,
    /// What a voice evaluates at each engine sample (MATH §8.2, §9).
    pub plan: Plan,
    /// The pitch envelope's rates and levels (MATH §5).
    pub pitch_envelope: peg::Parameters,
    /// The LFO's parameters (MATH §6).
    pub lfo: CompiledLfo,
    /// σ(PMS), the pitch's sensitivity to the LFO and the controllers, in
    /// octaves (MATH §2.4).
    pub pitch_sensitivity: f64,
    /// Whether a key-on restarts the operators' phases: OSC KEY SYNC (MATH
    /// §2.5).
    pub key_sync: bool,
    /// TRANSPOSE, 0 to 48, 24 none (MATH §2.1).
    pub transpose: f64,
    /// The function data (MATH §7).
    pub function: CompiledFunction,
}

impl CompiledPatch {
    /// A part's patch and function data compiled in a configuration: total, as
    /// every value of `keptone-model` is one that the laws take (MATH §0.5).
    /// The part plays its own function data until the supplement's laws are
    /// written, whatever `part.function_source` says (MATH §13.5).
    #[must_use]
    pub fn new(patch: &Patch, function: &Function, configuration: &Configuration) -> Self {
        let voice: &Voice = &patch.voice;
        let fb = voice.feedback.get();
        // FB exactly 0 has no feedback edge (MATH §8.1, §9.1).
        let graph = Graph::of_algorithm(voice.algorithm, fb != 0.0);
        let feedback: Vec<FeedbackEdge> = graph
            .edges()
            .iter()
            .filter(|edge| edge.kind == Kind::Feedback)
            .map(|edge| FeedbackEdge {
                from: edge.from,
                to: edge.to,
                kappa: kappa(fb),
            })
            .collect();
        let components = components(&graph);
        let plan = Plan::new(&graph, &components, &feedback);
        Self {
            operators: voice
                .operators
                .iter()
                .map(|operator| CompiledOperator::new(operator, configuration))
                .collect(),
            components,
            graph,
            feedback,
            plan,
            pitch_envelope: peg::Parameters {
                rates: voice.pitch_envelope.rates.map(keptone_model::voice::Value::get),
                levels: voice.pitch_envelope.levels.map(keptone_model::voice::Value::get),
            },
            lfo: CompiledLfo {
                speed: voice.lfo.speed.get(),
                delay: voice.lfo.delay.get(),
                pitch_depth: voice.lfo.pitch_depth.get(),
                amplitude_depth: voice.lfo.amplitude_depth.get(),
                sync: voice.lfo.sync,
                wave: voice.lfo.wave,
            },
            pitch_sensitivity: pitch_sensitivity(voice.pitch_sensitivity.get()),
            key_sync: voice.oscillator_sync,
            transpose: voice.transpose.get(),
            function: CompiledFunction::new(function),
        }
    }
}

/// A part as the engine takes it (MATH §13.5): the notes it takes, and the
/// patch it plays with its function data.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledPart {
    /// Whether it takes notes at all.
    pub on: bool,
    /// Whether it receives every channel.
    pub omni: bool,
    /// The channel it receives, numbered across MIDI 2.0's groups: sixteen
    /// for each group before its own, then its channel within its group, 0 to
    /// 255.
    pub channel: u8,
    /// The lowest key it takes.
    pub lowest_key: u8,
    /// The highest key it takes.
    pub highest_key: u8,
    /// Which keys in its range it takes.
    pub key_filter: KeyFilter,
    /// The note shift s in keys (MATH §2.1).
    pub note_shift: f64,
    /// At most how many voices it holds, if it has a limit (MATH §13.5).
    pub note_limit: Option<NonZeroU32>,
    /// Its compiled patch, with its function data.
    pub patch: CompiledPatch,
}

impl CompiledPart {
    /// A part compiled in a configuration: total, as every value of
    /// `keptone-model` is one that the engine takes. Until the supplement's
    /// laws are written, the part plays its own function data (MATH §13.5).
    #[must_use]
    pub fn new(part: &Part, configuration: &Configuration) -> Self {
        Self {
            on: part.on,
            omni: part.omni,
            channel: part.group.get().saturating_mul(16).saturating_add(part.channel.get()),
            lowest_key: part.lowest_key.get(),
            highest_key: part.highest_key.get(),
            key_filter: part.key_filter,
            note_shift: part.note_shift.get(),
            note_limit: part.note_limit,
            patch: CompiledPatch::new(&part.patch, &part.function, configuration),
        }
    }

    /// Whether it receives a channel's messages: it is on, and receives that
    /// channel or every channel (MATH §13.5).
    #[must_use]
    pub const fn receives(&self, channel: u8) -> bool {
        self.on && (self.omni || self.channel == channel)
    }

    /// Whether it takes a note of a channel and a key (MATH §13.5): it
    /// receives the channel, the key lies between its lowest key and its
    /// highest, both included, and its key filter lets the key through.
    #[must_use]
    pub const fn takes(&self, channel: u8, key: u8) -> bool {
        let filtered = match self.key_filter {
            KeyFilter::All => true,
            KeyFilter::Odd => !key.is_multiple_of(2),
            KeyFilter::Even => key.is_multiple_of(2),
        };
        self.receives(channel) && self.lowest_key <= key && key <= self.highest_key && filtered
    }
}
