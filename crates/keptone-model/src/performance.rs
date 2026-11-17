// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The performances of the DX1 and the TX7, every parameter of which is a
//! range type.
//!
//! A performance holds, for each of its two sides, A and B, what the DX7 keeps
//! as its function data ([`crate::function`]): mono or poly, the pitch bend's
//! range and step, the portamento's time, glissando and mode, and the four
//! controllers, each with a sensitivity of 0 to 15 where the DX7 has a range of
//! 0 to 99; and an attenuation of the side's output. It holds which side an
//! instrument that plays one takes, how the keys are assigned to the sides,
//! and a name of thirty letters \[TX7-MANUAL\]. The DX7 takes side A's as its
//! function data from a dump of one performance \[DX7-MANUAL\] \[AJXS-ROM\],
//! and the TX7 takes the side that the voice memory select names, into the
//! function memory of its voice; the TX7 keeps 64 performances, and dumps them
//! packed into 64 bytes each. [`Side::function`] gives a side's function data,
//! its sensitivities taken to ranges as `syx.performance_sensitivity` says
//! (docs/MATH.md §7.4), and [`Side::of_function`] the side of function data.
//!
//! [`PerformanceParameter`] numbers a performance's parameters as the dump of
//! one performance lays them out, 0 to 93, and [`DxPerformanceParameter`]
//! those of the DX performance parameter change, which the TX7 receives. As in
//! a voice, each number is a [`Value`] of its scale, which may lie between its
//! steps, each choice an enumeration and each switch a `bool`.

use std::fmt;

use crate::function::{Assign, Controller, Function, Polyphony, PortamentoMode};
use crate::voice::{Rounding, Value};

/// A controller of a side: its sensitivity and what it is assigned to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Effect {
    /// How far it modulates at its fullest, 0 to 15.
    pub sensitivity: Value<15>,
    /// What it modulates.
    pub assign: Assign,
}

impl Effect {
    /// No sensitivity, assigned to nothing.
    const ZERO: Self = Self {
        sensitivity: Value::ZERO,
        assign: Assign {
            pitch: false,
            amplitude: false,
            eg_bias: false,
        },
    };
}

/// One side of a performance, A or B: the DX7's function data, on the
/// performance's scales, and an attenuation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Side {
    /// Mono or poly.
    pub polyphony: Polyphony,
    /// The pitch bend's range in semitones, 0 to 12.
    pub pitch_bend_range: Value<12>,
    /// The pitch bend's step, 0 to 12.
    pub pitch_bend_step: Value<12>,
    /// The portamento's time, 0 to 99.
    pub portamento_time: Value<99>,
    /// Whether the portamento moves by notes, glissando.
    pub glissando: bool,
    /// The portamento's mode.
    pub portamento_mode: PortamentoMode,
    /// The modulation wheel.
    pub modulation_wheel: Effect,
    /// The foot controller.
    pub foot_controller: Effect,
    /// The aftertouch.
    pub aftertouch: Effect,
    /// The breath controller.
    pub breath_controller: Effect,
    /// How far the side's output is attenuated, 0 to 7: the audio output level
    /// attenuator \[TX7-MANUAL\].
    pub attenuation: Value<7>,
}

impl Side {
    /// A side of every step 0.
    const ZERO: Self = Self {
        polyphony: Polyphony::Poly,
        pitch_bend_range: Value::ZERO,
        pitch_bend_step: Value::ZERO,
        portamento_time: Value::ZERO,
        glissando: false,
        portamento_mode: PortamentoMode::Retain,
        modulation_wheel: Effect::ZERO,
        foot_controller: Effect::ZERO,
        aftertouch: Effect::ZERO,
        breath_controller: Effect::ZERO,
        attenuation: Value::ZERO,
    };

    /// The DX7's function data of the side: each field as it is, and each
    /// controller's range its sensitivity taken to the scale of 0 to 99 by
    /// `scale` (docs/MATH.md §7.4). The attenuation is not function data.
    #[must_use]
    pub fn function(&self, scale: SensitivityScale) -> Function {
        let controller = |effect: Effect| Controller {
            range: scale.range(effect.sensitivity),
            assign: effect.assign,
        };
        Function {
            polyphony: self.polyphony,
            pitch_bend_range: self.pitch_bend_range,
            pitch_bend_step: self.pitch_bend_step,
            portamento_mode: self.portamento_mode,
            glissando: self.glissando,
            portamento_time: self.portamento_time,
            modulation_wheel: controller(self.modulation_wheel),
            foot_controller: controller(self.foot_controller),
            breath_controller: controller(self.breath_controller),
            aftertouch: controller(self.aftertouch),
        }
    }

    /// The side of function data and an attenuation: each field as it is, and
    /// each controller's sensitivity its range taken to the scale of 0 to 15
    /// as 15r/99, which a dump rounds to a step as `syx.export_rounding` says
    /// (docs/MATH.md §7.4).
    #[must_use]
    pub fn of_function(function: &Function, attenuation: Value<7>) -> Self {
        let effect = |controller: Controller| Effect {
            sensitivity: Value::clamped(15.0 * controller.range.get() / 99.0),
            assign: controller.assign,
        };
        Self {
            polyphony: function.polyphony,
            pitch_bend_range: function.pitch_bend_range,
            pitch_bend_step: function.pitch_bend_step,
            portamento_time: function.portamento_time,
            glissando: function.glissando,
            portamento_mode: function.portamento_mode,
            modulation_wheel: effect(function.modulation_wheel),
            foot_controller: effect(function.foot_controller),
            aftertouch: effect(function.aftertouch),
            breath_controller: effect(function.breath_controller),
            attenuation,
        }
    }

    /// A field's value: its [`Value`], or the step of an enumeration, a switch
    /// or an assignment.
    fn value(&self, field: SideField) -> f64 {
        match field {
            SideField::Polyphony => f64::from(self.polyphony.step()),
            SideField::PitchBendRange => self.pitch_bend_range.get(),
            SideField::PitchBendStep => self.pitch_bend_step.get(),
            SideField::PortamentoTime => self.portamento_time.get(),
            SideField::Glissando => f64::from(self.glissando),
            SideField::PortamentoMode => f64::from(self.portamento_mode.step()),
            SideField::Sensitivity(controller) => self.effect(controller).sensitivity.get(),
            SideField::Assign(controller) => f64::from(self.effect(controller).assign.step()),
            SideField::Attenuation => self.attenuation.get(),
        }
    }

    /// Sets a field to a step, or to its largest if the step is more.
    fn set_step(&mut self, field: SideField, step: u8) {
        match field {
            SideField::Polyphony => self.polyphony = Polyphony::saturating(step),
            SideField::PitchBendRange => self.pitch_bend_range = Value::saturating(step),
            SideField::PitchBendStep => self.pitch_bend_step = Value::saturating(step),
            SideField::PortamentoTime => self.portamento_time = Value::saturating(step),
            SideField::Glissando => self.glissando = step != 0,
            SideField::PortamentoMode => self.portamento_mode = PortamentoMode::saturating(step),
            SideField::Sensitivity(controller) => self.effect_mut(controller).sensitivity = Value::saturating(step),
            SideField::Assign(controller) => self.effect_mut(controller).assign = Assign::saturating(step),
            SideField::Attenuation => self.attenuation = Value::saturating(step),
        }
    }

    /// The controller of an index, in the order of the dump: 0 for the
    /// modulation wheel, then the foot controller, the aftertouch and the
    /// breath controller.
    const fn effect(&self, controller: usize) -> &Effect {
        match controller {
            0 => &self.modulation_wheel,
            1 => &self.foot_controller,
            2 => &self.aftertouch,
            _ => &self.breath_controller,
        }
    }

    /// The controller of an index, to be set.
    const fn effect_mut(&mut self, controller: usize) -> &mut Effect {
        match controller {
            0 => &mut self.modulation_wheel,
            1 => &mut self.foot_controller,
            2 => &mut self.aftertouch,
            _ => &mut self.breath_controller,
        }
    }
}

/// How a sensitivity of 0 to 15 becomes a range of 0 to 99: the setting
/// `syx.performance_sensitivity` (docs/MATH.md §7.4, §17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SensitivityScale {
    /// In proportion, 99s/15, 15 giving 99 and the steps between giving
    /// values between the range's steps: the default.
    #[default]
    Linear,
    /// As the DX7's firmware takes it, ⌊53s/8⌋: 0, 6, 13, 19, …, 92, 99
    /// \[AJXS-ROM\].
    Firmware,
}

impl SensitivityScale {
    /// The range of a sensitivity.
    #[must_use]
    pub fn range(self, sensitivity: Value<15>) -> Value<99> {
        let s = sensitivity.get();
        // 99s and 53s are exact for every step, and so is the division by 8;
        // the division by 15 is correctly rounded, and 15 gives 99 exactly.
        let range = match self {
            Self::Linear => 99.0 * s / 15.0,
            Self::Firmware => (53.0 * s / 8.0).floor(),
        };
        Value::clamped(range)
    }
}

/// Which side an instrument that plays one takes: the voice memory select, 0
/// or 1, which \[TX7-MANUAL\] names A and B in that order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VoiceSelect {
    /// 0: side A.
    A,
    /// 1: side B.
    B,
}

impl VoiceSelect {
    /// The side of a step, if the step is 0 or 1.
    #[must_use]
    pub const fn from_step(step: u8) -> Option<Self> {
        match step {
            0 => Some(Self::A),
            1 => Some(Self::B),
            _ => None,
        }
    }

    /// The side of a step, or B if the step is more than 1.
    const fn saturating(step: u8) -> Self {
        if step == 0 { Self::A } else { Self::B }
    }

    /// Its step.
    #[must_use]
    pub const fn step(self) -> u8 {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }
}

/// How the keys are assigned to the sides, by its step \[TX7-MANUAL\]: a side
/// alone, both on every key, or each on a part of the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAssign {
    /// 0: SINGLE, the side the voice memory select names.
    Single,
    /// 1: DUAL.
    Dual,
    /// 2: SPLIT.
    Split,
}

impl KeyAssign {
    /// The mode of a step, if the step is 0 to 2.
    #[must_use]
    pub const fn from_step(step: u8) -> Option<Self> {
        match step {
            0 => Some(Self::Single),
            1 => Some(Self::Dual),
            2 => Some(Self::Split),
            _ => None,
        }
    }

    /// The mode of a step, or the last if the step is more than 2.
    pub(crate) const fn saturating(step: u8) -> Self {
        match step {
            0 => Self::Single,
            1 => Self::Dual,
            _ => Self::Split,
        }
    }

    /// Its step.
    #[must_use]
    pub const fn step(self) -> u8 {
        match self {
            Self::Single => 0,
            Self::Dual => 1,
            Self::Split => 2,
        }
    }
}

/// A performance's name: thirty bytes of seven bits, ASCII as the dumps have
/// it, kept as they come.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PerformanceName([u8; 30]);

impl PerformanceName {
    /// The name of thirty bytes, if each is below 128.
    #[must_use]
    pub fn new(bytes: [u8; 30]) -> Option<Self> {
        bytes.iter().all(|&byte| byte < 0x80).then_some(Self(bytes))
    }

    /// Its bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 30] {
        self.0
    }
}

/// The 33 bytes of the dump of one performance that \[TX7-MANUAL\] leaves
/// unnamed, in their order, seven bits each, kept as they come: of each side's
/// thirty, bytes 0, 1, 8, 17 to 25 and 27 to 29, then 60, 62 and 63. The DX7's
/// firmware takes byte 0 as the number of a voice to select \[AJXS-ROM\].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Unnamed([u8; 33]);

impl Unnamed {
    /// Every byte 0.
    pub const ZERO: Self = Self([0; 33]);

    /// The bytes, if each is below 128.
    #[must_use]
    pub fn new(bytes: [u8; 33]) -> Option<Self> {
        bytes.iter().all(|&byte| byte < 0x80).then_some(Self(bytes))
    }

    /// The bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 33] {
        self.0
    }
}

/// A performance of the DX1 and the TX7.
///
/// The dump of one performance holds all of it but the key assign mode, which
/// a performance read from it has as [`KeyAssign::Single`]; the dump of 64
/// holds all of it but the unnamed bytes, which a performance read from it has
/// as 0.
#[derive(Debug, Clone, PartialEq)]
pub struct Performance {
    /// Side A, then side B.
    pub sides: [Side; 2],
    /// Which side an instrument that plays one takes.
    pub voice_select: VoiceSelect,
    /// How the keys are assigned to the sides.
    pub key_assign: KeyAssign,
    /// The name.
    pub name: PerformanceName,
    /// The bytes of the dump of one performance that have no name.
    pub unnamed: Unnamed,
}

impl Performance {
    /// A performance of every step 0: the start of one whose every parameter
    /// a codec sets.
    pub(crate) const fn zero() -> Self {
        Self {
            sides: [Side::ZERO; 2],
            voice_select: VoiceSelect::A,
            key_assign: KeyAssign::Single,
            name: PerformanceName([0; 30]),
            unnamed: Unnamed::ZERO,
        }
    }

    /// A parameter's value: its [`Value`], or the step of an enumeration, a
    /// switch, an assignment or a byte.
    #[must_use]
    pub fn value(&self, parameter: PerformanceParameter) -> f64 {
        match parameter.field() {
            Field::Side(side, field) => self.sides[side].value(field),
            Field::VoiceSelect => f64::from(self.voice_select.step()),
            Field::Name(i) => f64::from(self.name.0[i]),
            Field::Unnamed(i) => f64::from(self.unnamed.0[i]),
        }
    }

    /// The step of a parameter, as a dump carries it: its value rounded by
    /// `rounding`.
    #[must_use]
    pub fn step(&self, parameter: PerformanceParameter, rounding: Rounding) -> u8 {
        rounding.apply(self.value(parameter))
    }

    /// Whether a parameter's value is a step, which a dump holds exactly.
    #[must_use]
    pub fn is_step(&self, parameter: PerformanceParameter) -> bool {
        self.value(parameter).fract() == 0.0
    }

    /// Sets a parameter to a step. If the step is no more than the parameter's
    /// largest ([`PerformanceParameter::largest`]), the parameter takes it and
    /// the result is true; if it is more, the parameter takes its largest and
    /// the result is false.
    pub fn set_step(&mut self, parameter: PerformanceParameter, step: u8) -> bool {
        match parameter.field() {
            Field::Side(side, field) => self.sides[side].set_step(field, step),
            Field::VoiceSelect => self.voice_select = VoiceSelect::saturating(step),
            Field::Name(i) => self.name.0[i] = step.min(0x7f),
            Field::Unnamed(i) => self.unnamed.0[i] = step.min(0x7f),
        }
        step <= parameter.largest()
    }
}

/// A parameter of a performance, by its byte in the dump of one performance,
/// 0 to 93, as \[TX7-MANUAL\] numbers them: side A's thirty bytes, 0 to 29,
/// with mono/poly at 2, the pitch bend's range and step at 3 and 4, the
/// portamento's time, glissando and mode at 5 to 7, the sensitivity and the
/// assignment of the modulation wheel, the foot controller, the aftertouch and
/// the breath controller at 9 to 16, and the attenuation at 26; side B's,
/// 30 to 59, in the same order; the voice memory select at 61; the name at
/// 64 to 93; and the bytes that have no name ([`Unnamed`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PerformanceParameter(pub(crate) u8);

impl PerformanceParameter {
    /// The number of parameters: 94.
    pub const COUNT: u8 = 94;

    /// The parameter of a number, if the number is below 94.
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

    /// The largest step it takes: as \[TX7-MANUAL\] gives it for a named
    /// parameter, and 127 for a byte of the name or a byte with no name.
    #[must_use]
    pub fn largest(self) -> u8 {
        match self.field() {
            Field::Side(_, field) => field.largest(),
            Field::VoiceSelect => 1,
            Field::Name(_) | Field::Unnamed(_) => 0x7f,
        }
    }

    /// What it names.
    fn field(self) -> Field {
        match self.0 {
            0..=59 => {
                let side = usize::from(self.0 / 30);
                let offset = self.0 % 30;
                match SideField::at(offset) {
                    Some(field) => Field::Side(side, field),
                    None => Field::Unnamed(15 * side + unnamed_index(offset)),
                }
            }
            60 => Field::Unnamed(30),
            61 => Field::VoiceSelect,
            62 | 63 => Field::Unnamed(usize::from(self.0 - 31)),
            number => Field::Name(usize::from(number - 64)),
        }
    }
}

/// The place among a side's fifteen unnamed bytes of an offset that names
/// nothing: 0, 1, 8, 17 to 25 and 27 to 29, in that order.
fn unnamed_index(offset: u8) -> usize {
    usize::from(match offset {
        0 | 1 => offset,
        8 => 2,
        17..=25 => offset - 14,
        _ => offset - 15,
    })
}

impl fmt::Display for PerformanceParameter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.field() {
            Field::Side(side, field) => write!(formatter, "voice {} {field}", ["A", "B"][side]),
            Field::VoiceSelect => formatter.write_str("voice memory select"),
            Field::Name(i) => write!(formatter, "name byte {}", i + 1),
            Field::Unnamed(_) => write!(formatter, "byte {}", self.0),
        }
    }
}

/// What a performance parameter's number names.
#[derive(Debug, Clone, Copy)]
enum Field {
    /// A field of the side of this index, 0 for A.
    Side(usize, SideField),
    VoiceSelect,
    /// The byte of the name of this index, 0 for the first.
    Name(usize),
    /// The unnamed byte of this index ([`Unnamed`]).
    Unnamed(usize),
}

/// A field of a side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SideField {
    Polyphony,
    PitchBendRange,
    PitchBendStep,
    PortamentoTime,
    Glissando,
    PortamentoMode,
    /// The sensitivity of the controller of this index, in the order of the
    /// dump: 0 for the modulation wheel, then the foot controller, the
    /// aftertouch and the breath controller.
    Sensitivity(usize),
    /// The assignment of the controller of this index.
    Assign(usize),
    Attenuation,
}

impl SideField {
    /// The field at an offset of a side's thirty bytes, if one lies there.
    fn at(offset: u8) -> Option<Self> {
        match offset {
            2 => Some(Self::Polyphony),
            3 => Some(Self::PitchBendRange),
            4 => Some(Self::PitchBendStep),
            5 => Some(Self::PortamentoTime),
            6 => Some(Self::Glissando),
            7 => Some(Self::PortamentoMode),
            9 | 11 | 13 | 15 => Some(Self::Sensitivity(usize::from((offset - 9) / 2))),
            10 | 12 | 14 | 16 => Some(Self::Assign(usize::from((offset - 10) / 2))),
            26 => Some(Self::Attenuation),
            _ => None,
        }
    }

    /// The largest step it takes \[TX7-MANUAL\].
    const fn largest(self) -> u8 {
        match self {
            Self::Polyphony | Self::Glissando | Self::PortamentoMode => 1,
            Self::PitchBendRange | Self::PitchBendStep => 12,
            Self::PortamentoTime => 99,
            Self::Sensitivity(_) => 15,
            Self::Assign(_) | Self::Attenuation => 7,
        }
    }
}

impl fmt::Display for SideField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        const CONTROLLERS: [&str; 4] = ["modulation wheel", "foot controller", "aftertouch", "breath controller"];
        match *self {
            Self::Polyphony => formatter.write_str("poly/mono"),
            Self::PitchBendRange => formatter.write_str("pitch bend range"),
            Self::PitchBendStep => formatter.write_str("pitch bend step"),
            Self::PortamentoTime => formatter.write_str("portamento time"),
            Self::Glissando => formatter.write_str("portamento/glissando"),
            Self::PortamentoMode => formatter.write_str("portamento mode"),
            Self::Sensitivity(controller) => write!(formatter, "{} sensitivity", CONTROLLERS[controller]),
            Self::Assign(controller) => write!(formatter, "{} assign", CONTROLLERS[controller]),
            Self::Attenuation => formatter.write_str("attenuation"),
        }
    }
}

/// A parameter of the DX performance parameter change, which the TX7 receives,
/// by its number \[TX7-MANUAL\]: 1, the source select, the channel it receives
/// on, 1 to 16; 2 to 7, 9 to 16 and 26, a side's fields at the offsets of
/// [`PerformanceParameter`]; and 64, the master tuning, 0 to 127 with concert
/// pitch at 64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DxPerformanceParameter(u8);

impl DxPerformanceParameter {
    /// The number of the source select.
    const SOURCE_SELECT: u8 = 1;

    /// The number of the master tuning.
    const MASTER_TUNING: u8 = 64;

    /// The parameter of a number, if the number names one: 1 to 7, 9 to 16,
    /// 26 or 64.
    #[must_use]
    pub fn new(number: u8) -> Option<Self> {
        (number == Self::SOURCE_SELECT || number == Self::MASTER_TUNING || SideField::at(number).is_some())
            .then_some(Self(number))
    }

    /// Its number.
    #[must_use]
    pub const fn number(self) -> u8 {
        self.0
    }

    /// Every parameter, in the order of their numbers.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..=Self::MASTER_TUNING).filter_map(Self::new)
    }

    /// The smallest step it takes: 1 for the source select, 0 for the others.
    #[must_use]
    pub const fn smallest(self) -> u8 {
        if self.0 == Self::SOURCE_SELECT { 1 } else { 0 }
    }

    /// The largest step it takes: 16 for the source select, 127 for the master
    /// tuning, and a side's field's largest for the others.
    #[must_use]
    pub fn largest(self) -> u8 {
        match SideField::at(self.0) {
            Some(field) => field.largest(),
            None if self.0 == Self::SOURCE_SELECT => 16,
            None => 0x7f,
        }
    }

    /// The parameter of side A that it sets, for 2 to 26.
    #[must_use]
    pub fn side(self) -> Option<PerformanceParameter> {
        SideField::at(self.0).map(|_| PerformanceParameter(self.0))
    }
}

impl fmt::Display for DxPerformanceParameter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match SideField::at(self.0) {
            Some(field) => write!(formatter, "{field}"),
            None if self.0 == Self::SOURCE_SELECT => formatter.write_str("source select"),
            None => formatter.write_str("master tuning"),
        }
    }
}
