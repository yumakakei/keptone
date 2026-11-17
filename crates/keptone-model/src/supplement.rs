// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The DX7II's supplement of a voice, every parameter of which is a range type.
//!
//! A DX7II voice is a DX7 voice ([`crate::voice`]) and its supplement, the
//! additional parameters of the ACED and AMEM formats of \[DX7II-MANUAL\],
//! which the DX7s shares \[DX7S-MANUAL\]: each
//! operator's scaling mode, normal or fractional, and its amplitude modulation
//! sensitivity, 0 to 7 where the DX7's is 0 to 3; the pitch envelope's range
//! and rate scaling, and whether velocity scales it; the LFO's key trigger;
//! mono or poly and unison, with the unison's detune; the pitch bend's range,
//! step and mode; a random pitch fluctuation; the portamento's mode, step and
//! time; and the ranges by which the modulation wheel, the two foot
//! controllers, the breath controller, the aftertouch and a MIDI controller
//! modulate the pitch and the amplitude and bias the envelopes, with a volume
//! for some and a pitch bias for others. What each does to the sound is the
//! DX7II's emulation's to say (docs/DESIGN.md §15); the supplement holds them,
//! each number a [`Value`] of its scale that may lie between its steps, each
//! choice an enumeration and each switch a `bool`.
//!
//! [`SupplementParameter`] numbers the parameters as the DX7II's parameter
//! changes of the supplement do: 0 to 38, and 64 to 73.

use std::fmt;

use crate::function::{Polyphony, PortamentoMode};
use crate::voice::{Rounding, Value};

/// How an operator's output level follows the keys: the DX7's keyboard level
/// scaling, or the DX7II's fractional scaling, by its step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScalingMode {
    /// 0: the DX7's.
    Normal,
    /// 1: fractional.
    Fractional,
}

/// An operator's part of the supplement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OperatorSupplement {
    /// How its output level follows the keys.
    pub scaling_mode: ScalingMode,
    /// Its amplitude modulation sensitivity, 0 to 7.
    pub amplitude_sensitivity: Value<7>,
}

/// The range of the pitch envelope, by its step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PitchEgRange {
    /// 0: eight octaves.
    EightOctaves,
    /// 1: four octaves.
    FourOctaves,
    /// 2: one octave.
    OneOctave,
    /// 3: half an octave.
    HalfOctave,
}

/// How a key starts the LFO, by its step: the LFO key trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LfoTrigger {
    /// 0: one LFO for every key, which a key starts when none is held.
    Single,
    /// 1: an LFO for each key.
    Multi,
}

/// Which notes the pitch bend bends, by its step \[DX7S-MANUAL\] pp. 55,
/// Add-10. \[DX7II-MANUAL\]'s table gives the mode as 0 to 2, low, high and
/// key on, leaving out the normal mode at 0, as its AMEM format's two bits for
/// it and its initial value, 0, show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BendMode {
    /// 0: every note, those the foot switch sustains among them, as the DX7
    /// bends them.
    Normal,
    /// 1: the lowest note only.
    Lowest,
    /// 2: the highest note only.
    Highest,
    /// 3: every note whose key is held, and not those the foot switch
    /// sustains.
    KeyOn,
}

/// How far a controller modulates the pitch and the amplitude and biases the
/// envelopes at its fullest, 0 to 99 each.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Modulation {
    /// How far it modulates the pitch.
    pub pitch: Value<99>,
    /// How far it modulates the amplitude.
    pub amplitude: Value<99>,
    /// How far it biases the envelopes.
    pub eg_bias: Value<99>,
}

impl Modulation {
    /// No modulation.
    const ZERO: Self = Self {
        pitch: Value::ZERO,
        amplitude: Value::ZERO,
        eg_bias: Value::ZERO,
    };
}

/// The DX7II's supplement of a voice: what its ACED format holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Supplement {
    /// OP1 to OP6, in that order; the dumps hold them the other way round.
    pub operators: [OperatorSupplement; 6],
    /// The pitch envelope's range.
    pub pitch_eg_range: PitchEgRange,
    /// How a key starts the LFO.
    pub lfo_trigger: LfoTrigger,
    /// Whether velocity scales the pitch envelope.
    pub pitch_eg_velocity: bool,
    /// Mono or poly.
    pub polyphony: Polyphony,
    /// Whether every voice plays each key, detuned: unison.
    pub unison: bool,
    /// The pitch bend's range in semitones, 0 to 12.
    pub pitch_bend_range: Value<12>,
    /// The pitch bend's step, 0 to 12.
    pub pitch_bend_step: Value<12>,
    /// Which keys the pitch bend bends.
    pub pitch_bend_mode: BendMode,
    /// The random pitch fluctuation, 0 for none to 7.
    pub random_pitch: Value<7>,
    /// The portamento's mode.
    pub portamento_mode: PortamentoMode,
    /// The portamento's step, 0 to 12.
    pub portamento_step: Value<12>,
    /// The portamento's time, 0 to 99.
    pub portamento_time: Value<99>,
    /// The modulation wheel.
    pub modulation_wheel: Modulation,
    /// The first foot controller.
    pub foot_controller_1: Modulation,
    /// The first foot controller's volume range, 0 to 99.
    pub foot_controller_1_volume: Value<99>,
    /// The breath controller.
    pub breath_controller: Modulation,
    /// The breath controller's pitch bias, 0 to 100.
    pub breath_controller_pitch_bias: Value<100>,
    /// The aftertouch.
    pub aftertouch: Modulation,
    /// The aftertouch's pitch bias, 0 to 100.
    pub aftertouch_pitch_bias: Value<100>,
    /// How far the keys scale the pitch envelope's rates, 0 to 7.
    pub pitch_eg_rate_scaling: Value<7>,
    /// The second foot controller.
    pub foot_controller_2: Modulation,
    /// The second foot controller's volume range, 0 to 99.
    pub foot_controller_2_volume: Value<99>,
    /// A MIDI controller.
    pub midi_control: Modulation,
    /// The MIDI controller's volume range, 0 to 99.
    pub midi_control_volume: Value<99>,
    /// The unison's detune, 0 to 7.
    pub unison_detune: Value<7>,
    /// Whether the first foot controller serves as the first continuous
    /// slider.
    pub foot_controller_1_as_slider: bool,
}

impl Supplement {
    /// A supplement of every step 0: the start of one whose every parameter a
    /// codec sets.
    pub(crate) const fn zero() -> Self {
        Self {
            operators: [OperatorSupplement {
                scaling_mode: ScalingMode::Normal,
                amplitude_sensitivity: Value::ZERO,
            }; 6],
            pitch_eg_range: PitchEgRange::EightOctaves,
            lfo_trigger: LfoTrigger::Single,
            pitch_eg_velocity: false,
            polyphony: Polyphony::Poly,
            unison: false,
            pitch_bend_range: Value::ZERO,
            pitch_bend_step: Value::ZERO,
            pitch_bend_mode: BendMode::Normal,
            random_pitch: Value::ZERO,
            portamento_mode: PortamentoMode::Retain,
            portamento_step: Value::ZERO,
            portamento_time: Value::ZERO,
            modulation_wheel: Modulation::ZERO,
            foot_controller_1: Modulation::ZERO,
            foot_controller_1_volume: Value::ZERO,
            breath_controller: Modulation::ZERO,
            breath_controller_pitch_bias: Value::ZERO,
            aftertouch: Modulation::ZERO,
            aftertouch_pitch_bias: Value::ZERO,
            pitch_eg_rate_scaling: Value::ZERO,
            foot_controller_2: Modulation::ZERO,
            foot_controller_2_volume: Value::ZERO,
            midi_control: Modulation::ZERO,
            midi_control_volume: Value::ZERO,
            unison_detune: Value::ZERO,
            foot_controller_1_as_slider: false,
        }
    }

    /// A parameter's value: its [`Value`], or the step of an enumeration or a
    /// switch; mono/poly and unison as the two bits that parameter 15 carries,
    /// unison's the higher.
    #[must_use]
    pub fn value(&self, parameter: SupplementParameter) -> f64 {
        match parameter.field() {
            Field::ScalingMode(k) => f64::from(self.operators[k].scaling_mode == ScalingMode::Fractional),
            Field::AmplitudeSensitivity(k) => self.operators[k].amplitude_sensitivity.get(),
            Field::PitchEgRange => f64::from(match self.pitch_eg_range {
                PitchEgRange::EightOctaves => 0_u8,
                PitchEgRange::FourOctaves => 1,
                PitchEgRange::OneOctave => 2,
                PitchEgRange::HalfOctave => 3,
            }),
            Field::LfoTrigger => f64::from(self.lfo_trigger == LfoTrigger::Multi),
            Field::PitchEgVelocity => f64::from(self.pitch_eg_velocity),
            Field::PolyphonyAndUnison => f64::from(self.polyphony.step() | u8::from(self.unison) << 1),
            Field::PitchBendRange => self.pitch_bend_range.get(),
            Field::PitchBendStep => self.pitch_bend_step.get(),
            Field::PitchBendMode => f64::from(match self.pitch_bend_mode {
                BendMode::Normal => 0_u8,
                BendMode::Lowest => 1,
                BendMode::Highest => 2,
                BendMode::KeyOn => 3,
            }),
            Field::RandomPitch => self.random_pitch.get(),
            Field::PortamentoMode => f64::from(self.portamento_mode.step()),
            Field::PortamentoStep => self.portamento_step.get(),
            Field::PortamentoTime => self.portamento_time.get(),
            Field::Modulation(controller, destination) => self.modulation(controller, destination).get(),
            Field::Volume(controller) => self.volume(controller).get(),
            Field::PitchBias(controller) => self.pitch_bias(controller).get(),
            Field::PitchEgRateScaling => self.pitch_eg_rate_scaling.get(),
            Field::UnisonDetune => self.unison_detune.get(),
            Field::FootControllerAsSlider => f64::from(self.foot_controller_1_as_slider),
        }
    }

    /// The step of a parameter, as a dump or a parameter change carries it: its
    /// value rounded by `rounding`.
    #[must_use]
    pub fn step(&self, parameter: SupplementParameter, rounding: Rounding) -> u8 {
        rounding.apply(self.value(parameter))
    }

    /// Whether a parameter's value is a step, which a dump holds exactly.
    #[must_use]
    pub fn is_step(&self, parameter: SupplementParameter) -> bool {
        self.value(parameter).fract() == 0.0
    }

    /// Sets a parameter to a step. If the step is no more than the parameter's
    /// largest ([`SupplementParameter::largest`]), the parameter takes it and
    /// the result is true; if it is more, the parameter takes its largest and
    /// the result is false.
    pub fn set_step(&mut self, parameter: SupplementParameter, step: u8) -> bool {
        match parameter.field() {
            Field::ScalingMode(k) => {
                self.operators[k].scaling_mode = if step == 0 {
                    ScalingMode::Normal
                } else {
                    ScalingMode::Fractional
                };
            }
            Field::AmplitudeSensitivity(k) => self.operators[k].amplitude_sensitivity = Value::saturating(step),
            Field::PitchEgRange => {
                self.pitch_eg_range = match step {
                    0 => PitchEgRange::EightOctaves,
                    1 => PitchEgRange::FourOctaves,
                    2 => PitchEgRange::OneOctave,
                    _ => PitchEgRange::HalfOctave,
                };
            }
            Field::LfoTrigger => {
                self.lfo_trigger = if step == 0 {
                    LfoTrigger::Single
                } else {
                    LfoTrigger::Multi
                }
            }
            Field::PitchEgVelocity => self.pitch_eg_velocity = step != 0,
            Field::PolyphonyAndUnison => {
                let bits = step.min(3);
                self.polyphony = Polyphony::saturating(bits & 1);
                self.unison = bits & 2 != 0;
            }
            Field::PitchBendRange => self.pitch_bend_range = Value::saturating(step),
            Field::PitchBendStep => self.pitch_bend_step = Value::saturating(step),
            Field::PitchBendMode => {
                self.pitch_bend_mode = match step {
                    0 => BendMode::Normal,
                    1 => BendMode::Lowest,
                    2 => BendMode::Highest,
                    _ => BendMode::KeyOn,
                };
            }
            Field::RandomPitch => self.random_pitch = Value::saturating(step),
            Field::PortamentoMode => self.portamento_mode = PortamentoMode::saturating(step),
            Field::PortamentoStep => self.portamento_step = Value::saturating(step),
            Field::PortamentoTime => self.portamento_time = Value::saturating(step),
            Field::Modulation(controller, destination) => {
                *self.modulation_mut(controller, destination) = Value::saturating(step);
            }
            Field::Volume(controller) => *self.volume_mut(controller) = Value::saturating(step),
            Field::PitchBias(controller) => *self.pitch_bias_mut(controller) = Value::saturating(step),
            Field::PitchEgRateScaling => self.pitch_eg_rate_scaling = Value::saturating(step),
            Field::UnisonDetune => self.unison_detune = Value::saturating(step),
            Field::FootControllerAsSlider => self.foot_controller_1_as_slider = step != 0,
        }
        step <= parameter.largest()
    }

    /// The controller of an index, in the order of the parameters: the
    /// modulation wheel, the first foot controller, the breath controller,
    /// the aftertouch, the second foot controller and the MIDI controller.
    const fn controller(&self, controller: usize) -> &Modulation {
        match controller {
            0 => &self.modulation_wheel,
            1 => &self.foot_controller_1,
            2 => &self.breath_controller,
            3 => &self.aftertouch,
            4 => &self.foot_controller_2,
            _ => &self.midi_control,
        }
    }

    /// The controller of an index, to be set.
    const fn controller_mut(&mut self, controller: usize) -> &mut Modulation {
        match controller {
            0 => &mut self.modulation_wheel,
            1 => &mut self.foot_controller_1,
            2 => &mut self.breath_controller,
            3 => &mut self.aftertouch,
            4 => &mut self.foot_controller_2,
            _ => &mut self.midi_control,
        }
    }

    /// A controller's range for a destination: 0 the pitch, 1 the amplitude,
    /// 2 the EG bias.
    const fn modulation(&self, controller: usize, destination: usize) -> Value<99> {
        let modulation = self.controller(controller);
        match destination {
            0 => modulation.pitch,
            1 => modulation.amplitude,
            _ => modulation.eg_bias,
        }
    }

    /// A controller's range for a destination, to be set.
    const fn modulation_mut(&mut self, controller: usize, destination: usize) -> &mut Value<99> {
        let modulation = self.controller_mut(controller);
        match destination {
            0 => &mut modulation.pitch,
            1 => &mut modulation.amplitude,
            _ => &mut modulation.eg_bias,
        }
    }

    /// The volume range of a controller that has one: the first foot
    /// controller (1), the second (4) or the MIDI controller.
    const fn volume(&self, controller: usize) -> Value<99> {
        match controller {
            1 => self.foot_controller_1_volume,
            4 => self.foot_controller_2_volume,
            _ => self.midi_control_volume,
        }
    }

    /// The volume range of a controller, to be set.
    const fn volume_mut(&mut self, controller: usize) -> &mut Value<99> {
        match controller {
            1 => &mut self.foot_controller_1_volume,
            4 => &mut self.foot_controller_2_volume,
            _ => &mut self.midi_control_volume,
        }
    }

    /// The pitch bias of a controller that has one: the breath controller (2)
    /// or the aftertouch.
    const fn pitch_bias(&self, controller: usize) -> Value<100> {
        if controller == 2 {
            self.breath_controller_pitch_bias
        } else {
            self.aftertouch_pitch_bias
        }
    }

    /// The pitch bias of a controller, to be set.
    const fn pitch_bias_mut(&mut self, controller: usize) -> &mut Value<100> {
        if controller == 2 {
            &mut self.breath_controller_pitch_bias
        } else {
            &mut self.aftertouch_pitch_bias
        }
    }
}

/// A parameter of the supplement, by the number that the DX7II's parameter
/// changes of the supplement give it \[DX7II-MANUAL\]:
///
/// - 0 to 5, the operators' scaling modes, and 6 to 11, their amplitude
///   modulation sensitivities, each from OP6 down to OP1;
/// - 12 to 15, the pitch envelope's range, the LFO's key trigger, whether
///   velocity scales the pitch envelope, and mono/poly with unison;
/// - 16 to 22, the pitch bend's range, step and mode, the random pitch
///   fluctuation, and the portamento's mode, step and time;
/// - 23 to 37, the ranges of the modulation wheel (pitch, amplitude, EG
///   bias), the first foot controller (and volume), the breath controller and
///   the aftertouch (and pitch bias);
/// - 38, the pitch envelope's rate scaling;
/// - 64 to 71, the ranges of the second foot controller and of the MIDI
///   controller, each with a volume; 72, the unison's detune; and 73, whether
///   the first foot controller serves as the first continuous slider.
///
/// Numbers 39 to 63 are reserved. The ACED format holds the parameters in the
/// order of their numbers, 49 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SupplementParameter(pub(crate) u8);

impl SupplementParameter {
    /// The number of parameters: 49.
    pub const COUNT: u8 = 49;

    /// The parameter of a number, if the number names one: 0 to 38 or 64 to
    /// 73.
    #[must_use]
    pub fn new(number: u8) -> Option<Self> {
        matches!(number, 0..=38 | 64..=73).then_some(Self(number))
    }

    /// Its number.
    #[must_use]
    pub const fn number(self) -> u8 {
        self.0
    }

    /// Every parameter, in the order of their numbers, which is the order of
    /// the ACED format.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..=73).filter_map(Self::new)
    }

    /// The largest step it takes \[DX7II-MANUAL\].
    #[must_use]
    pub fn largest(self) -> u8 {
        match self.field() {
            Field::ScalingMode(_)
            | Field::LfoTrigger
            | Field::PitchEgVelocity
            | Field::PortamentoMode
            | Field::FootControllerAsSlider => 1,
            Field::AmplitudeSensitivity(_) | Field::RandomPitch | Field::PitchEgRateScaling | Field::UnisonDetune => 7,
            Field::PitchEgRange | Field::PolyphonyAndUnison | Field::PitchBendMode => 3,
            Field::PitchBendRange | Field::PitchBendStep | Field::PortamentoStep => 12,
            Field::PortamentoTime | Field::Modulation(..) | Field::Volume(_) => 99,
            Field::PitchBias(_) => 100,
        }
    }

    /// What it names.
    fn field(self) -> Field {
        match self.0 {
            0..=5 => Field::ScalingMode(5 - usize::from(self.0)),
            6..=11 => Field::AmplitudeSensitivity(11 - usize::from(self.0)),
            12 => Field::PitchEgRange,
            13 => Field::LfoTrigger,
            14 => Field::PitchEgVelocity,
            15 => Field::PolyphonyAndUnison,
            16 => Field::PitchBendRange,
            17 => Field::PitchBendStep,
            18 => Field::PitchBendMode,
            19 => Field::RandomPitch,
            20 => Field::PortamentoMode,
            21 => Field::PortamentoStep,
            22 => Field::PortamentoTime,
            23..=25 => Field::Modulation(0, usize::from(self.0 - 23)),
            26..=28 => Field::Modulation(1, usize::from(self.0 - 26)),
            29 => Field::Volume(1),
            30..=32 => Field::Modulation(2, usize::from(self.0 - 30)),
            33 => Field::PitchBias(2),
            34..=36 => Field::Modulation(3, usize::from(self.0 - 34)),
            37 => Field::PitchBias(3),
            38 => Field::PitchEgRateScaling,
            64..=66 => Field::Modulation(4, usize::from(self.0 - 64)),
            67 => Field::Volume(4),
            68..=70 => Field::Modulation(5, usize::from(self.0 - 68)),
            71 => Field::Volume(5),
            72 => Field::UnisonDetune,
            _ => Field::FootControllerAsSlider,
        }
    }
}

impl fmt::Display for SupplementParameter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        const CONTROLLERS: [&str; 6] = [
            "modulation wheel",
            "foot controller 1",
            "breath controller",
            "aftertouch",
            "foot controller 2",
            "MIDI control",
        ];
        const DESTINATIONS: [&str; 3] = ["pitch modulation", "amplitude modulation", "EG bias"];
        match self.field() {
            Field::ScalingMode(k) => write!(formatter, "OP{} scaling mode", k + 1),
            Field::AmplitudeSensitivity(k) => write!(formatter, "OP{} amplitude modulation sensitivity", k + 1),
            Field::PitchEgRange => formatter.write_str("pitch EG range"),
            Field::LfoTrigger => formatter.write_str("LFO key trigger"),
            Field::PitchEgVelocity => formatter.write_str("pitch EG by velocity"),
            Field::PolyphonyAndUnison => formatter.write_str("poly/mono and unison"),
            Field::PitchBendRange => formatter.write_str("pitch bend range"),
            Field::PitchBendStep => formatter.write_str("pitch bend step"),
            Field::PitchBendMode => formatter.write_str("pitch bend mode"),
            Field::RandomPitch => formatter.write_str("random pitch fluctuation"),
            Field::PortamentoMode => formatter.write_str("portamento mode"),
            Field::PortamentoStep => formatter.write_str("portamento step"),
            Field::PortamentoTime => formatter.write_str("portamento time"),
            Field::Modulation(controller, destination) => {
                write!(
                    formatter,
                    "{} {} range",
                    CONTROLLERS[controller], DESTINATIONS[destination]
                )
            }
            Field::Volume(controller) => write!(formatter, "{} volume range", CONTROLLERS[controller]),
            Field::PitchBias(controller) => write!(formatter, "{} pitch bias", CONTROLLERS[controller]),
            Field::PitchEgRateScaling => formatter.write_str("pitch EG rate scaling"),
            Field::UnisonDetune => formatter.write_str("unison detune"),
            Field::FootControllerAsSlider => formatter.write_str("foot controller 1 as continuous slider 1"),
        }
    }
}

/// What a supplement parameter's number names.
#[derive(Debug, Clone, Copy)]
enum Field {
    /// The scaling mode of the operator of this index, 0 for OP1.
    ScalingMode(usize),
    /// The amplitude modulation sensitivity of the operator of this index.
    AmplitudeSensitivity(usize),
    PitchEgRange,
    LfoTrigger,
    PitchEgVelocity,
    PolyphonyAndUnison,
    PitchBendRange,
    PitchBendStep,
    PitchBendMode,
    RandomPitch,
    PortamentoMode,
    PortamentoStep,
    PortamentoTime,
    /// The range of a controller ([`Supplement::controller`]'s index) for a
    /// destination, 0 the pitch, 1 the amplitude, 2 the EG bias.
    Modulation(usize, usize),
    /// The volume range of a controller.
    Volume(usize),
    /// The pitch bias of a controller.
    PitchBias(usize),
    PitchEgRateScaling,
    UnisonDetune,
    FootControllerAsSlider,
}
