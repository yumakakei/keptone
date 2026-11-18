// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! A DX7 voice, every parameter of which is typed.
//!
//! A [`Voice`] is what a single-voice dump of the DX7 holds, and each of the 32
//! places of a bank \[DEXED-SYSEX\]: six operators, each with its envelope, its
//! keyboard scaling, its sensitivities, its level and its frequency, and the
//! voice's pitch envelope, algorithm, feedback, LFO, transposition and name.
//! Each parameter that is a number on the DX7's scale, 0 to 99 for most, is a
//! [`Value`]: any finite binary64, which may lie between the scale's steps,
//! where a host's automation leaves it, or beyond its ends, where a user takes
//! it (docs/MATH.md §0.5, docs/DESIGN.md §7.4). Each that picks one of a few
//! things is an enumeration, and each that is on or off a `bool`. So no voice
//! can hold a number that is not finite or an enumeration's step that names
//! nothing, and every voice is one the engine can play: the codecs that make
//! voices of bytes check each byte once, and nothing after them has anything to
//! check (docs/DESIGN.md §2).
//!
//! [`Parameter`] numbers the parameters as the DX7's single-voice dump and its
//! parameter changes do, 0 to 155, and [`Voice::value`] and [`Voice::set_step`]
//! read and set them by number, for the codecs of [`crate::syx`].

use std::fmt;

/// A number on the scale of a DX7 parameter, 0 to `MAX`, or beyond it: any
/// finite binary64 (docs/MATH.md §0.5).
///
/// The DX7 and its dumps know only the scale's steps, the integers 0 to `MAX`.
/// A value may lie between them, and docs/MATH.md says what such a value means
/// for each parameter, as for a level of an envelope (§3.1) or a rate (§4.1).
/// It may lie beyond the scale's ends as well, below 0 or above `MAX`, where
/// the user takes it, whatever it means there; a dump holds it as the step
/// that [`Value::step`] gives. A value is never NaN and never infinite. Nor is
/// it ever −0, which [`Value::new`] takes as 0, so that equal values have
/// equal bits.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Value<const MAX: u8>(f64);

impl<const MAX: u8> Value<MAX> {
    /// 0, the start of every scale.
    pub(crate) const ZERO: Self = Self(0.0);

    /// The value, if it is a finite number.
    #[must_use]
    pub fn new(value: f64) -> Option<Self> {
        // Adding +0 takes −0 to +0 and leaves every other number as it is.
        value.is_finite().then_some(Self(value + 0.0))
    }

    /// The value of a step, if the step is no more than `MAX`.
    #[must_use]
    pub fn from_step(step: u8) -> Option<Self> {
        (step <= MAX).then_some(Self(f64::from(step)))
    }

    /// The value of a step, or `MAX` if the step is more.
    pub(crate) fn saturating(step: u8) -> Self {
        Self(f64::from(step.min(MAX)))
    }

    /// A number taken as a value: NaN as 0, an infinity as the largest finite
    /// binary64 of its sign, and −0 as 0. The conversions between scales give
    /// finite numbers but where they overflow binary64, and take it as a guard.
    pub(crate) fn finite(value: f64) -> Self {
        if value.is_nan() {
            Self::ZERO
        } else {
            Self(value.clamp(-f64::MAX, f64::MAX) + 0.0)
        }
    }

    /// The value.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// The step the value rounds to by `rounding`, from 0 to `MAX`: a value
    /// beyond the scale as its nearer end.
    #[must_use]
    pub fn step(self, rounding: Rounding) -> u8 {
        rounding.apply(self.0).min(MAX)
    }
}

/// How a value between steps becomes a step where only steps can be written,
/// as in a dump: the setting `syx.export_rounding` (docs/MATH.md §17). A value
/// beyond the scale becomes its nearer end, whichever the rounding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Rounding {
    /// To the nearest step, and a value halfway between two steps to the even
    /// one: binary64's own rounding, which favours neither direction. The
    /// default.
    #[default]
    Nearest,
    /// Down, to the step at or below the value.
    Floor,
}

impl Rounding {
    /// The step from 0 to 255 that a value rounds to, a value below 0 taken
    /// as 0 and one above 255 as 255.
    pub(crate) fn apply(self, value: f64) -> u8 {
        small_integer(self.integer(value.clamp(0.0, 255.0)))
    }

    /// The step from 0 to 65535 that a value rounds to, a value below 0 taken
    /// as 0 and one above 65535 as 65535.
    pub(crate) fn apply_wide(self, value: f64) -> u16 {
        wide_integer(self.integer(value.clamp(0.0, 65535.0)))
    }

    /// The integer that a value rounds to.
    fn integer(self, value: f64) -> f64 {
        match self {
            Self::Nearest => value.round_ties_even(),
            Self::Floor => value.floor(),
        }
    }
}

/// An integer of [0, 255] as a byte.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "an integer of [0, 255] converts exactly"
)]
fn small_integer(integer: f64) -> u8 {
    integer as u8
}

/// An integer of [0, 65535] as sixteen bits.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "an integer of [0, 65535] converts exactly"
)]
fn wide_integer(integer: f64) -> u16 {
    integer as u16
}

/// The curve of keyboard level scaling on one side of the break point (MATH
/// §3.2), by its step: 0 to 3, in the order below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Curve {
    /// −LIN: the level falls in proportion to the distance from the break
    /// point.
    NegativeLinear,
    /// −EXP: the level falls exponentially with the distance.
    NegativeExponential,
    /// +EXP: the level rises exponentially with the distance.
    PositiveExponential,
    /// +LIN: the level rises in proportion to the distance.
    PositiveLinear,
}

impl Curve {
    /// The curve of a step, if the step is 0 to 3.
    #[must_use]
    pub const fn from_step(step: u8) -> Option<Self> {
        match step {
            0 => Some(Self::NegativeLinear),
            1 => Some(Self::NegativeExponential),
            2 => Some(Self::PositiveExponential),
            3 => Some(Self::PositiveLinear),
            _ => None,
        }
    }

    /// The curve of a step, or the last if the step is more than 3.
    fn saturating(step: u8) -> Self {
        Self::from_step(step).unwrap_or(Self::PositiveLinear)
    }

    /// Its step.
    #[must_use]
    pub const fn step(self) -> u8 {
        match self {
            Self::NegativeLinear => 0,
            Self::NegativeExponential => 1,
            Self::PositiveExponential => 2,
            Self::PositiveLinear => 3,
        }
    }
}

/// How an operator's frequency is set, by its step: OSC MODE 0 or 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// 0: as a ratio to the key's frequency (MATH §2.2).
    Ratio,
    /// 1: as a frequency of its own, whatever the key (MATH §2.3).
    Fixed,
}

impl Mode {
    /// The mode of a step, if the step is 0 or 1.
    #[must_use]
    pub const fn from_step(step: u8) -> Option<Self> {
        match step {
            0 => Some(Self::Ratio),
            1 => Some(Self::Fixed),
            _ => None,
        }
    }

    /// The mode of a step, or the last if the step is more than 1.
    const fn saturating(step: u8) -> Self {
        if step == 0 { Self::Ratio } else { Self::Fixed }
    }

    /// Its step.
    #[must_use]
    pub const fn step(self) -> u8 {
        match self {
            Self::Ratio => 0,
            Self::Fixed => 1,
        }
    }
}

/// The LFO's waveform (MATH §6.2), by its step: 0 to 5, in the order below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Wave {
    /// A triangle.
    Triangle,
    /// A ramp down.
    SawDown,
    /// A ramp up.
    SawUp,
    /// A square.
    Square,
    /// A sine.
    Sine,
    /// Sample and hold: a value at random each cycle.
    SampleAndHold,
}

impl Wave {
    /// The wave of a step, if the step is 0 to 5.
    #[must_use]
    pub const fn from_step(step: u8) -> Option<Self> {
        match step {
            0 => Some(Self::Triangle),
            1 => Some(Self::SawDown),
            2 => Some(Self::SawUp),
            3 => Some(Self::Square),
            4 => Some(Self::Sine),
            5 => Some(Self::SampleAndHold),
            _ => None,
        }
    }

    /// The wave of a step, or the last if the step is more than 5.
    fn saturating(step: u8) -> Self {
        Self::from_step(step).unwrap_or(Self::SampleAndHold)
    }

    /// Its step.
    #[must_use]
    pub const fn step(self) -> u8 {
        match self {
            Self::Triangle => 0,
            Self::SawDown => 1,
            Self::SawUp => 2,
            Self::Square => 3,
            Self::Sine => 4,
            Self::SampleAndHold => 5,
        }
    }
}

/// One of the DX7's 32 algorithms (MATH §8.1), by the number the DX7's panel
/// gives it, 1 to 32. Its step, in a dump, is one less.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Algorithm(u8);

impl Algorithm {
    /// The algorithm of a number, if the number is 1 to 32.
    #[must_use]
    pub fn new(number: u8) -> Option<Self> {
        (1..=32).contains(&number).then_some(Self(number))
    }

    /// The algorithm of a step, if the step is 0 to 31.
    #[must_use]
    pub const fn from_step(step: u8) -> Option<Self> {
        if step < 32 { Some(Self(step + 1)) } else { None }
    }

    /// The algorithm of a step, or the last if the step is more than 31.
    fn saturating(step: u8) -> Self {
        Self(step.min(31) + 1)
    }

    /// Its number, 1 to 32.
    #[must_use]
    pub const fn number(self) -> u8 {
        self.0
    }

    /// Its step, 0 to 31.
    #[must_use]
    pub const fn step(self) -> u8 {
        self.0 - 1
    }
}

/// A voice's name: ten bytes, each below 128, as the dumps hold them.
///
/// Every byte is kept as it is, a code below 32 too, so that a name comes back
/// from a dump as it went in; what character each code shows as is the
/// editor's to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Name([u8; 10]);

impl Name {
    /// The name of ten bytes, if each is below 128.
    #[must_use]
    pub fn new(bytes: [u8; 10]) -> Option<Self> {
        bytes.iter().all(|&byte| byte < 0x80).then_some(Self(bytes))
    }

    /// Its bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 10] {
        self.0
    }
}

/// An envelope's four rates and four levels, R1 to R4 and L1 to L4, each 0 to
/// 99: an operator's (MATH §4) or the pitch's (MATH §5).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Envelope {
    /// R1 to R4: how fast each segment moves.
    pub rates: [Value<99>; 4],
    /// L1 to L4: the level each segment moves to.
    pub levels: [Value<99>; 4],
}

impl Envelope {
    /// Every rate and level 0.
    pub(crate) const ZERO: Self = Self {
        rates: [Value::ZERO; 4],
        levels: [Value::ZERO; 4],
    };
}

/// One of a voice's six operators.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Operator {
    /// Its envelope (MATH §4).
    pub envelope: Envelope,
    /// The break point of its keyboard level scaling, 0 to 99: the key the
    /// scaling reaches out from on either side, 39 being C3 (MATH §3.2).
    pub break_point: Value<99>,
    /// The depth of the scaling below the break point, 0 to 99.
    pub left_depth: Value<99>,
    /// The depth of the scaling above the break point, 0 to 99.
    pub right_depth: Value<99>,
    /// The curve of the scaling below the break point.
    pub left_curve: Curve,
    /// The curve of the scaling above the break point.
    pub right_curve: Curve,
    /// How much higher keys speed its envelope up, 0 to 7 (MATH §4.1).
    pub rate_scaling: Value<7>,
    /// How much the LFO and the controllers lower its amplitude, 0 to 3 (MATH
    /// §7.2).
    pub amplitude_sensitivity: Value<3>,
    /// How much a key's velocity sets its level, 0 to 7 (MATH §3.2).
    pub velocity_sensitivity: Value<7>,
    /// Its output level, 0 to 99 (MATH §3.2).
    pub output_level: Value<99>,
    /// Whether its frequency is a ratio to the key's or one of its own.
    pub mode: Mode,
    /// The coarse part of its frequency, 0 to 31 (MATH §2.2, §2.3).
    pub coarse: Value<31>,
    /// The fine part of its frequency, 0 to 99.
    pub fine: Value<99>,
    /// Its detune, 0 to 14, 7 being none (MATH §2.2).
    pub detune: Value<14>,
    /// Whether it sounds: its switch on the DX7, which a parameter change
    /// carries and no dump holds.
    pub on: bool,
}

impl Operator {
    /// Every step 0, and the operator on.
    const ZERO: Self = Self {
        envelope: Envelope::ZERO,
        break_point: Value::ZERO,
        left_depth: Value::ZERO,
        right_depth: Value::ZERO,
        left_curve: Curve::NegativeLinear,
        right_curve: Curve::NegativeLinear,
        rate_scaling: Value::ZERO,
        amplitude_sensitivity: Value::ZERO,
        velocity_sensitivity: Value::ZERO,
        output_level: Value::ZERO,
        mode: Mode::Ratio,
        coarse: Value::ZERO,
        fine: Value::ZERO,
        detune: Value::ZERO,
        on: true,
    };

    /// The value of one of its parameters, as [`Voice::value`] gives it.
    fn value(&self, field: OperatorField) -> f64 {
        match field {
            OperatorField::Rate(i) => self.envelope.rates[i].get(),
            OperatorField::Level(i) => self.envelope.levels[i].get(),
            OperatorField::BreakPoint => self.break_point.get(),
            OperatorField::LeftDepth => self.left_depth.get(),
            OperatorField::RightDepth => self.right_depth.get(),
            OperatorField::LeftCurve => f64::from(self.left_curve.step()),
            OperatorField::RightCurve => f64::from(self.right_curve.step()),
            OperatorField::RateScaling => self.rate_scaling.get(),
            OperatorField::AmplitudeSensitivity => self.amplitude_sensitivity.get(),
            OperatorField::VelocitySensitivity => self.velocity_sensitivity.get(),
            OperatorField::OutputLevel => self.output_level.get(),
            OperatorField::Mode => f64::from(self.mode.step()),
            OperatorField::Coarse => self.coarse.get(),
            OperatorField::Fine => self.fine.get(),
            OperatorField::Detune => self.detune.get(),
        }
    }

    /// Sets one of its parameters to a value, as [`Voice::set_value`] does.
    fn set_value(&mut self, field: OperatorField, value: f64) -> bool {
        let curve = |value| step_of(value, 4).and_then(Curve::from_step);
        match field {
            OperatorField::Rate(i) => set(&mut self.envelope.rates[i], value),
            OperatorField::Level(i) => set(&mut self.envelope.levels[i], value),
            OperatorField::BreakPoint => set(&mut self.break_point, value),
            OperatorField::LeftDepth => set(&mut self.left_depth, value),
            OperatorField::RightDepth => set(&mut self.right_depth, value),
            OperatorField::LeftCurve => curve(value).map(|curve| self.left_curve = curve).is_some(),
            OperatorField::RightCurve => curve(value).map(|curve| self.right_curve = curve).is_some(),
            OperatorField::RateScaling => set(&mut self.rate_scaling, value),
            OperatorField::AmplitudeSensitivity => set(&mut self.amplitude_sensitivity, value),
            OperatorField::VelocitySensitivity => set(&mut self.velocity_sensitivity, value),
            OperatorField::OutputLevel => set(&mut self.output_level, value),
            OperatorField::Mode => step_of(value, 2)
                .and_then(Mode::from_step)
                .map(|mode| self.mode = mode)
                .is_some(),
            OperatorField::Coarse => set(&mut self.coarse, value),
            OperatorField::Fine => set(&mut self.fine, value),
            OperatorField::Detune => set(&mut self.detune, value),
        }
    }

    /// Sets one of its parameters to a step, or to the parameter's largest if
    /// the step is more.
    fn set_step(&mut self, field: OperatorField, step: u8) {
        match field {
            OperatorField::Rate(i) => self.envelope.rates[i] = Value::saturating(step),
            OperatorField::Level(i) => self.envelope.levels[i] = Value::saturating(step),
            OperatorField::BreakPoint => self.break_point = Value::saturating(step),
            OperatorField::LeftDepth => self.left_depth = Value::saturating(step),
            OperatorField::RightDepth => self.right_depth = Value::saturating(step),
            OperatorField::LeftCurve => self.left_curve = Curve::saturating(step),
            OperatorField::RightCurve => self.right_curve = Curve::saturating(step),
            OperatorField::RateScaling => self.rate_scaling = Value::saturating(step),
            OperatorField::AmplitudeSensitivity => self.amplitude_sensitivity = Value::saturating(step),
            OperatorField::VelocitySensitivity => self.velocity_sensitivity = Value::saturating(step),
            OperatorField::OutputLevel => self.output_level = Value::saturating(step),
            OperatorField::Mode => self.mode = Mode::saturating(step),
            OperatorField::Coarse => self.coarse = Value::saturating(step),
            OperatorField::Fine => self.fine = Value::saturating(step),
            OperatorField::Detune => self.detune = Value::saturating(step),
        }
    }
}

/// The voice's LFO (MATH §6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lfo {
    /// Its speed, 0 to 99 (MATH §6.1).
    pub speed: Value<99>,
    /// The delay before its modulation fades in after a key, 0 to 99 (MATH
    /// §6.4).
    pub delay: Value<99>,
    /// The depth of its modulation of pitch, PMD, 0 to 99 (MATH §2.4).
    pub pitch_depth: Value<99>,
    /// The depth of its modulation of amplitude, AMD, 0 to 99 (MATH §7.2).
    pub amplitude_depth: Value<99>,
    /// Whether a key restarts it (MATH §6.5).
    pub sync: bool,
    /// Its waveform.
    pub wave: Wave,
}

/// A DX7 voice: all that a single-voice dump holds, and the operators'
/// switches.
#[derive(Debug, Clone, PartialEq)]
pub struct Voice {
    /// OP1 to OP6, in that order; the dumps hold them the other way round.
    pub operators: [Operator; 6],
    /// The pitch envelope (MATH §5).
    pub pitch_envelope: Envelope,
    /// The algorithm.
    pub algorithm: Algorithm,
    /// The feedback, 0 to 7 (MATH §9.1).
    pub feedback: Value<7>,
    /// Whether a key restarts the operators' phases: OSC KEY SYNC (MATH §2.5).
    pub oscillator_sync: bool,
    /// The LFO.
    pub lfo: Lfo,
    /// How much the LFO and the controllers modulate the pitch, PMS, 0 to 7
    /// (MATH §2.4).
    pub pitch_sensitivity: Value<7>,
    /// The transposition, 0 to 48, 24 being none (MATH §2.1).
    pub transpose: Value<48>,
    /// The name.
    pub name: Name,
}

impl Voice {
    /// A voice of every step 0, its operators on: the start of a voice whose
    /// every parameter a codec sets.
    pub(crate) const fn zero() -> Self {
        Self {
            operators: [Operator::ZERO; 6],
            pitch_envelope: Envelope::ZERO,
            algorithm: Algorithm(1),
            feedback: Value::ZERO,
            oscillator_sync: false,
            lfo: Lfo {
                speed: Value::ZERO,
                delay: Value::ZERO,
                pitch_depth: Value::ZERO,
                amplitude_depth: Value::ZERO,
                sync: false,
                wave: Wave::Triangle,
            },
            pitch_sensitivity: Value::ZERO,
            transpose: Value::ZERO,
            name: Name([0; 10]),
        }
    }

    /// The voice a part starts with (MATH §13.5): the DX7's VOICE INIT, the
    /// voice its firmware 1.8 sets up for one to be made \[AJXS-ROM\]. OP1
    /// alone is heard, at output level 99, as a sine at the key's pitch: every
    /// envelope rises at once to 99, holds it while the key is down and falls
    /// at once to 0 when it is let go, and the LFO, at speed 35, modulates
    /// nothing until a controller brings it in.
    #[must_use]
    pub fn init() -> Self {
        let full = Value::saturating(99);
        let operator = Operator {
            envelope: Envelope {
                rates: [full; 4],
                levels: [full, full, full, Value::ZERO],
            },
            coarse: Value::saturating(1),
            detune: Value::saturating(7),
            ..Operator::ZERO
        };
        let mut operators = [operator; 6];
        operators[0].output_level = full;
        Self {
            operators,
            pitch_envelope: Envelope {
                rates: [full; 4],
                levels: [Value::saturating(50); 4],
            },
            algorithm: Algorithm(1),
            feedback: Value::ZERO,
            oscillator_sync: true,
            lfo: Lfo {
                speed: Value::saturating(35),
                delay: Value::ZERO,
                pitch_depth: Value::ZERO,
                amplitude_depth: Value::ZERO,
                sync: true,
                wave: Wave::Triangle,
            },
            pitch_sensitivity: Value::saturating(3),
            transpose: Value::saturating(24),
            name: Name(*b"INIT VOICE"),
        }
    }

    /// A parameter's value: its [`Value`], or the step of an enumeration, a
    /// switch or a byte of the name; the operators' switches as the six bits
    /// that parameter 155 carries, OP1's the highest.
    #[must_use]
    pub fn value(&self, parameter: Parameter) -> f64 {
        match parameter.field() {
            Field::Operator(k, field) => self.operators[k].value(field),
            Field::PitchRate(i) => self.pitch_envelope.rates[i].get(),
            Field::PitchLevel(i) => self.pitch_envelope.levels[i].get(),
            Field::Algorithm => f64::from(self.algorithm.step()),
            Field::Feedback => self.feedback.get(),
            Field::OscillatorSync => f64::from(self.oscillator_sync),
            Field::LfoSpeed => self.lfo.speed.get(),
            Field::LfoDelay => self.lfo.delay.get(),
            Field::LfoPitchDepth => self.lfo.pitch_depth.get(),
            Field::LfoAmplitudeDepth => self.lfo.amplitude_depth.get(),
            Field::LfoSync => f64::from(self.lfo.sync),
            Field::LfoWave => f64::from(self.lfo.wave.step()),
            Field::PitchSensitivity => self.pitch_sensitivity.get(),
            Field::Transpose => self.transpose.get(),
            Field::Name(i) => f64::from(self.name.0[i]),
            Field::Switches => f64::from(
                self.operators
                    .iter()
                    .fold(0, |bits, operator| bits << 1 | u8::from(operator.on)),
            ),
        }
    }

    /// The step of a parameter, as a dump or a parameter change carries it: its
    /// value rounded by `rounding`, a value beyond the parameter's scale taken
    /// as its nearer end.
    #[must_use]
    pub fn step(&self, parameter: Parameter, rounding: Rounding) -> u8 {
        rounding.apply(self.value(parameter)).min(parameter.largest())
    }

    /// Whether a parameter's value is a step, an integer on its scale, which a
    /// dump holds exactly.
    #[must_use]
    pub fn is_step(&self, parameter: Parameter) -> bool {
        is_step_of(self.value(parameter), parameter.largest())
    }

    /// Sets a parameter to a step. If the step is no more than the parameter's
    /// largest ([`Parameter::largest`]), the parameter takes it and the result
    /// is true; if it is more, the parameter takes its largest and the result
    /// is false.
    pub fn set_step(&mut self, parameter: Parameter, step: u8) -> bool {
        match parameter.field() {
            Field::Operator(k, field) => self.operators[k].set_step(field, step),
            Field::PitchRate(i) => self.pitch_envelope.rates[i] = Value::saturating(step),
            Field::PitchLevel(i) => self.pitch_envelope.levels[i] = Value::saturating(step),
            Field::Algorithm => self.algorithm = Algorithm::saturating(step),
            Field::Feedback => self.feedback = Value::saturating(step),
            Field::OscillatorSync => self.oscillator_sync = step != 0,
            Field::LfoSpeed => self.lfo.speed = Value::saturating(step),
            Field::LfoDelay => self.lfo.delay = Value::saturating(step),
            Field::LfoPitchDepth => self.lfo.pitch_depth = Value::saturating(step),
            Field::LfoAmplitudeDepth => self.lfo.amplitude_depth = Value::saturating(step),
            Field::LfoSync => self.lfo.sync = step != 0,
            Field::LfoWave => self.lfo.wave = Wave::saturating(step),
            Field::PitchSensitivity => self.pitch_sensitivity = Value::saturating(step),
            Field::Transpose => self.transpose = Value::saturating(step),
            Field::Name(i) => self.name.0[i] = step.min(0x7f),
            Field::Switches => {
                let bits = step.min(0x3f);
                for (operator, bit) in self.operators.iter_mut().zip((0..6_u8).rev()) {
                    operator.on = bits >> bit & 1 != 0;
                }
            }
        }
        step <= parameter.largest()
    }

    /// Sets a parameter to a value, if it is one the parameter takes: any finite
    /// number for a [`Value`], on its scale, between its steps or beyond its
    /// ends; the step
    /// of an enumeration, the algorithm's among them; and 0 or 1 for a switch.
    /// The result says whether it did. A value it does not take, and every
    /// value of a byte of the name and of the switches, which are set by other
    /// means, leave the voice as it was.
    pub fn set_value(&mut self, parameter: Parameter, value: f64) -> bool {
        match parameter.field() {
            Field::Operator(k, field) => self.operators[k].set_value(field, value),
            Field::PitchRate(i) => set(&mut self.pitch_envelope.rates[i], value),
            Field::PitchLevel(i) => set(&mut self.pitch_envelope.levels[i], value),
            Field::Algorithm => step_of(value, 32)
                .and_then(Algorithm::from_step)
                .map(|algorithm| self.algorithm = algorithm)
                .is_some(),
            Field::Feedback => set(&mut self.feedback, value),
            Field::OscillatorSync => switch_of(value).map(|on| self.oscillator_sync = on).is_some(),
            Field::LfoSpeed => set(&mut self.lfo.speed, value),
            Field::LfoDelay => set(&mut self.lfo.delay, value),
            Field::LfoPitchDepth => set(&mut self.lfo.pitch_depth, value),
            Field::LfoAmplitudeDepth => set(&mut self.lfo.amplitude_depth, value),
            Field::LfoSync => switch_of(value).map(|on| self.lfo.sync = on).is_some(),
            Field::LfoWave => step_of(value, 6)
                .and_then(Wave::from_step)
                .map(|wave| self.lfo.wave = wave)
                .is_some(),
            Field::PitchSensitivity => set(&mut self.pitch_sensitivity, value),
            Field::Transpose => set(&mut self.transpose, value),
            Field::Name(_) | Field::Switches => false,
        }
    }
}

/// Sets a value of a scale to a number, if the number is finite, and says
/// whether it did.
fn set<const MAX: u8>(place: &mut Value<MAX>, value: f64) -> bool {
    Value::new(value).map(|value| *place = value).is_some()
}

/// Whether a number is a step of a scale from 0 to `largest`: an integer on
/// the scale.
pub(crate) fn is_step_of(value: f64, largest: u8) -> bool {
    value.fract() == 0.0 && (0.0..=f64::from(largest)).contains(&value)
}

/// The step a number is, if it is an integer below `count`, −0 being 0.
pub(crate) fn step_of(value: f64, count: u8) -> Option<u8> {
    (value.fract() == 0.0 && (0.0..f64::from(count)).contains(&value)).then(|| small_integer(value))
}

/// The switch a number is: 0 off and 1 on.
pub(crate) fn switch_of(value: f64) -> Option<bool> {
    step_of(value, 2).map(|step| step == 1)
}

/// A parameter of a voice, by the number that the DX7's single-voice dump and
/// its parameter changes give it \[DEXED-SYSEX\]:
///
/// - 0 to 125, the operators', 21 each from OP6 down to OP1: R1 to R4, L1 to
///   L4, the break point, the left and right depths, the left and right curves,
///   the rate scaling, the amplitude and velocity sensitivities, the output
///   level, the mode, the coarse and the fine frequency, and the detune;
/// - 126 to 144, the voice's: the pitch envelope's R1 to R4 and L1 to L4, the
///   algorithm, the feedback, the oscillators' key sync, the LFO's speed,
///   delay, pitch and amplitude depths, key sync and wave, the pitch
///   sensitivity and the transposition;
/// - 145 to 154, the bytes of the name;
/// - 155, the operators' switches, a bit each, OP1's the highest of six, which
///   only a parameter change carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Parameter(u8);

impl Parameter {
    /// The number of parameters: 156.
    pub const COUNT: u8 = 156;

    /// The number of parameters a single-voice dump holds, all but the
    /// switches: 155.
    pub const IN_DUMP: u8 = 155;

    /// The parameter of a number, if the number is below 156.
    #[must_use]
    pub fn new(number: u8) -> Option<Self> {
        (number < Self::COUNT).then_some(Self(number))
    }

    /// Its number.
    #[must_use]
    pub const fn number(self) -> u8 {
        self.0
    }

    /// Every parameter, in the order of their numbers.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..Self::COUNT).map(Self)
    }

    /// The parameters a single-voice dump holds, in its order.
    pub(crate) fn in_dump() -> impl Iterator<Item = Self> {
        (0..Self::IN_DUMP).map(Self)
    }

    /// The largest step it takes: 99 for most, 127 for a byte of the name, and
    /// 63 for the switches, all six on.
    #[must_use]
    pub fn largest(self) -> u8 {
        match self.field() {
            Field::Operator(_, field) => match field {
                OperatorField::Rate(_)
                | OperatorField::Level(_)
                | OperatorField::BreakPoint
                | OperatorField::LeftDepth
                | OperatorField::RightDepth
                | OperatorField::OutputLevel
                | OperatorField::Fine => 99,
                OperatorField::LeftCurve | OperatorField::RightCurve | OperatorField::AmplitudeSensitivity => 3,
                OperatorField::RateScaling | OperatorField::VelocitySensitivity => 7,
                OperatorField::Mode => 1,
                OperatorField::Coarse => 31,
                OperatorField::Detune => 14,
            },
            Field::PitchRate(_)
            | Field::PitchLevel(_)
            | Field::LfoSpeed
            | Field::LfoDelay
            | Field::LfoPitchDepth
            | Field::LfoAmplitudeDepth => 99,
            Field::Algorithm => 31,
            Field::Feedback | Field::PitchSensitivity => 7,
            Field::OscillatorSync | Field::LfoSync => 1,
            Field::LfoWave => 5,
            Field::Transpose => 48,
            Field::Name(_) => 0x7f,
            Field::Switches => 0x3f,
        }
    }

    /// What it names.
    fn field(self) -> Field {
        let number = self.0;
        match number {
            0..=125 => Field::Operator(5 - usize::from(number / 21), OperatorField::of(number % 21)),
            126..=129 => Field::PitchRate(usize::from(number - 126)),
            130..=133 => Field::PitchLevel(usize::from(number - 130)),
            134 => Field::Algorithm,
            135 => Field::Feedback,
            136 => Field::OscillatorSync,
            137 => Field::LfoSpeed,
            138 => Field::LfoDelay,
            139 => Field::LfoPitchDepth,
            140 => Field::LfoAmplitudeDepth,
            141 => Field::LfoSync,
            142 => Field::LfoWave,
            143 => Field::PitchSensitivity,
            144 => Field::Transpose,
            145..=154 => Field::Name(usize::from(number - 145)),
            _ => Field::Switches,
        }
    }
}

impl fmt::Display for Parameter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.field() {
            Field::Operator(k, field) => {
                write!(formatter, "OP{} ", k + 1)?;
                match field {
                    OperatorField::Rate(i) => write!(formatter, "EG rate {}", i + 1),
                    OperatorField::Level(i) => write!(formatter, "EG level {}", i + 1),
                    OperatorField::BreakPoint => formatter.write_str("break point"),
                    OperatorField::LeftDepth => formatter.write_str("left depth"),
                    OperatorField::RightDepth => formatter.write_str("right depth"),
                    OperatorField::LeftCurve => formatter.write_str("left curve"),
                    OperatorField::RightCurve => formatter.write_str("right curve"),
                    OperatorField::RateScaling => formatter.write_str("rate scaling"),
                    OperatorField::AmplitudeSensitivity => formatter.write_str("amplitude sensitivity"),
                    OperatorField::VelocitySensitivity => formatter.write_str("velocity sensitivity"),
                    OperatorField::OutputLevel => formatter.write_str("output level"),
                    OperatorField::Mode => formatter.write_str("oscillator mode"),
                    OperatorField::Coarse => formatter.write_str("coarse frequency"),
                    OperatorField::Fine => formatter.write_str("fine frequency"),
                    OperatorField::Detune => formatter.write_str("detune"),
                }
            }
            Field::PitchRate(i) => write!(formatter, "pitch EG rate {}", i + 1),
            Field::PitchLevel(i) => write!(formatter, "pitch EG level {}", i + 1),
            Field::Algorithm => formatter.write_str("algorithm"),
            Field::Feedback => formatter.write_str("feedback"),
            Field::OscillatorSync => formatter.write_str("oscillator key sync"),
            Field::LfoSpeed => formatter.write_str("LFO speed"),
            Field::LfoDelay => formatter.write_str("LFO delay"),
            Field::LfoPitchDepth => formatter.write_str("LFO pitch depth"),
            Field::LfoAmplitudeDepth => formatter.write_str("LFO amplitude depth"),
            Field::LfoSync => formatter.write_str("LFO key sync"),
            Field::LfoWave => formatter.write_str("LFO wave"),
            Field::PitchSensitivity => formatter.write_str("pitch sensitivity"),
            Field::Transpose => formatter.write_str("transpose"),
            Field::Name(i) => write!(formatter, "name byte {}", i + 1),
            Field::Switches => formatter.write_str("operator switches"),
        }
    }
}

/// What a parameter's number names: the field of a voice it reads and sets.
#[derive(Debug, Clone, Copy)]
enum Field {
    /// A parameter of the operator of this index, 0 for OP1.
    Operator(usize, OperatorField),
    /// A rate of the pitch envelope, 0 for R1.
    PitchRate(usize),
    /// A level of the pitch envelope, 0 for L1.
    PitchLevel(usize),
    Algorithm,
    Feedback,
    OscillatorSync,
    LfoSpeed,
    LfoDelay,
    LfoPitchDepth,
    LfoAmplitudeDepth,
    LfoSync,
    LfoWave,
    PitchSensitivity,
    Transpose,
    /// The byte of the name of this index, 0 for the first.
    Name(usize),
    Switches,
}

/// What a parameter of an operator names.
#[derive(Debug, Clone, Copy)]
enum OperatorField {
    /// A rate of its envelope, 0 for R1.
    Rate(usize),
    /// A level of its envelope, 0 for L1.
    Level(usize),
    BreakPoint,
    LeftDepth,
    RightDepth,
    LeftCurve,
    RightCurve,
    RateScaling,
    AmplitudeSensitivity,
    VelocitySensitivity,
    OutputLevel,
    Mode,
    Coarse,
    Fine,
    Detune,
}

impl OperatorField {
    /// The field of an operator's parameter of this offset, 0 to 20, in the
    /// order of the single-voice dump.
    fn of(offset: u8) -> Self {
        match offset {
            0..=3 => Self::Rate(usize::from(offset)),
            4..=7 => Self::Level(usize::from(offset - 4)),
            8 => Self::BreakPoint,
            9 => Self::LeftDepth,
            10 => Self::RightDepth,
            11 => Self::LeftCurve,
            12 => Self::RightCurve,
            13 => Self::RateScaling,
            14 => Self::AmplitudeSensitivity,
            15 => Self::VelocitySensitivity,
            16 => Self::OutputLevel,
            17 => Self::Mode,
            18 => Self::Coarse,
            19 => Self::Fine,
            _ => Self::Detune,
        }
    }
}
