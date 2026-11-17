// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The DX7's function data, every parameter of which is a range type.
//!
//! [`Function`] is what the DX7 keeps beside its voices and sends and receives
//! as the parameter changes of the function group, 64 to 77 \[DX7-MANUAL\]
//! \[DEXED-SYSEX\]: whether keys play one voice or several, the pitch bend's
//! range and step, the portamento's mode, glissando and time, and the range
//! and the assignment of each of the four controllers (docs/MATH.md §2.4, §7).
//! As in a voice ([`crate::voice`]), each number is a [`Value`] of the DX7's
//! scale, which may lie between its steps, each choice an enumeration and each
//! switch a `bool`, so that no function data out of range can be made.
//! [`FunctionParameter`] numbers the parameters as the parameter changes do.

use std::fmt;

use crate::voice::{Rounding, Value};

/// Whether keys play one voice at a time or several: function parameter 64,
/// by its step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Polyphony {
    /// 0: several voices at once, POLY.
    Poly,
    /// 1: one voice at a time, MONO (MATH §7.3).
    Mono,
}

impl Polyphony {
    /// The polyphony of a step, if the step is 0 or 1.
    #[must_use]
    pub const fn from_step(step: u8) -> Option<Self> {
        match step {
            0 => Some(Self::Poly),
            1 => Some(Self::Mono),
            _ => None,
        }
    }

    /// The polyphony of a step, or the last if the step is more than 1.
    pub(crate) const fn saturating(step: u8) -> Self {
        if step == 0 { Self::Poly } else { Self::Mono }
    }

    /// Its step.
    #[must_use]
    pub const fn step(self) -> u8 {
        match self {
            Self::Poly => 0,
            Self::Mono => 1,
        }
    }
}

/// How the portamento takes the keys: function parameter 67, by its step
/// \[DEXED-SYSEX\]; MATH §7.3 says what each does in POLY and in MONO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortamentoMode {
    /// 0: RETAIN.
    Retain,
    /// 1: FOLLOW.
    Follow,
}

impl PortamentoMode {
    /// The mode of a step, if the step is 0 or 1.
    #[must_use]
    pub const fn from_step(step: u8) -> Option<Self> {
        match step {
            0 => Some(Self::Retain),
            1 => Some(Self::Follow),
            _ => None,
        }
    }

    /// The mode of a step, or the last if the step is more than 1.
    pub(crate) const fn saturating(step: u8) -> Self {
        if step == 0 { Self::Retain } else { Self::Follow }
    }

    /// Its step.
    #[must_use]
    pub const fn step(self) -> u8 {
        match self {
            Self::Retain => 0,
            Self::Follow => 1,
        }
    }
}

/// What a controller is assigned to, as the three bits of its step, 0 to 7
/// \[DEXED-SYSEX\]: bit 0 the pitch, bit 1 the amplitude, bit 2 the EG bias
/// (MATH §7.1, §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Assign {
    /// Whether the controller modulates the pitch.
    pub pitch: bool,
    /// Whether it modulates the amplitude.
    pub amplitude: bool,
    /// Whether it biases the envelopes.
    pub eg_bias: bool,
}

impl Assign {
    /// The assignment of a step, if the step is 0 to 7.
    #[must_use]
    pub const fn from_step(step: u8) -> Option<Self> {
        if step > 7 {
            return None;
        }
        Some(Self {
            pitch: step & 1 != 0,
            amplitude: step & 2 != 0,
            eg_bias: step & 4 != 0,
        })
    }

    /// The assignment of a step, or of its lowest three bits' all set if the
    /// step is more than 7.
    pub(crate) const fn saturating(step: u8) -> Self {
        let step = if step > 7 { 7 } else { step };
        Self {
            pitch: step & 1 != 0,
            amplitude: step & 2 != 0,
            eg_bias: step & 4 != 0,
        }
    }

    /// Its step.
    #[must_use]
    pub fn step(self) -> u8 {
        u8::from(self.pitch) | u8::from(self.amplitude) << 1 | u8::from(self.eg_bias) << 2
    }
}

/// One of the four controllers: its range and what it is assigned to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Controller {
    /// How far it modulates at its fullest, 0 to 99.
    pub range: Value<99>,
    /// What it modulates.
    pub assign: Assign,
}

/// The DX7's function data.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    /// Mono or poly (64).
    pub polyphony: Polyphony,
    /// The pitch bend's range in semitones, 0 to 12 (65; MATH §2.4).
    pub pitch_bend_range: Value<12>,
    /// The pitch bend's step, 0 to 12, 0 for a continuous bend (66; MATH
    /// §2.4).
    pub pitch_bend_step: Value<12>,
    /// The portamento's mode (67).
    pub portamento_mode: PortamentoMode,
    /// Whether the portamento moves by the notes of the tuning, glissando
    /// (68; MATH §7.3).
    pub glissando: bool,
    /// The portamento's time, 0 to 99 (69; MATH §7.3).
    pub portamento_time: Value<99>,
    /// The modulation wheel (70, 71).
    pub modulation_wheel: Controller,
    /// The foot controller (72, 73).
    pub foot_controller: Controller,
    /// The breath controller (74, 75).
    pub breath_controller: Controller,
    /// The aftertouch (76, 77).
    pub aftertouch: Controller,
}

impl Function {
    /// A parameter's value: its [`Value`], or the step of an enumeration, a
    /// switch or an assignment.
    #[must_use]
    pub fn value(&self, parameter: FunctionParameter) -> f64 {
        match parameter.field() {
            Field::Polyphony => f64::from(self.polyphony.step()),
            Field::PitchBendRange => self.pitch_bend_range.get(),
            Field::PitchBendStep => self.pitch_bend_step.get(),
            Field::PortamentoMode => f64::from(self.portamento_mode.step()),
            Field::Glissando => f64::from(self.glissando),
            Field::PortamentoTime => self.portamento_time.get(),
            Field::Range(controller) => self.controller(controller).range.get(),
            Field::Assign(controller) => f64::from(self.controller(controller).assign.step()),
        }
    }

    /// The step of a parameter, as a parameter change carries it: its value
    /// rounded by `rounding`.
    #[must_use]
    pub fn step(&self, parameter: FunctionParameter, rounding: Rounding) -> u8 {
        rounding.apply(self.value(parameter))
    }

    /// Whether a parameter's value is a step, which a parameter change carries
    /// exactly.
    #[must_use]
    pub fn is_step(&self, parameter: FunctionParameter) -> bool {
        self.value(parameter).fract() == 0.0
    }

    /// Sets a parameter to a step. If the step is no more than the parameter's
    /// largest ([`FunctionParameter::largest`]), the parameter takes it and the
    /// result is true; if it is more, the parameter takes its largest and the
    /// result is false.
    pub fn set_step(&mut self, parameter: FunctionParameter, step: u8) -> bool {
        match parameter.field() {
            Field::Polyphony => self.polyphony = Polyphony::saturating(step),
            Field::PitchBendRange => self.pitch_bend_range = Value::saturating(step),
            Field::PitchBendStep => self.pitch_bend_step = Value::saturating(step),
            Field::PortamentoMode => self.portamento_mode = PortamentoMode::saturating(step),
            Field::Glissando => self.glissando = step != 0,
            Field::PortamentoTime => self.portamento_time = Value::saturating(step),
            Field::Range(controller) => self.controller_mut(controller).range = Value::saturating(step),
            Field::Assign(controller) => self.controller_mut(controller).assign = Assign::saturating(step),
        }
        step <= parameter.largest()
    }

    /// The controller of an index, 0 for the modulation wheel to 3 for the
    /// aftertouch, in the order of their parameters.
    const fn controller(&self, controller: usize) -> &Controller {
        match controller {
            0 => &self.modulation_wheel,
            1 => &self.foot_controller,
            2 => &self.breath_controller,
            _ => &self.aftertouch,
        }
    }

    /// The controller of an index, to be set.
    const fn controller_mut(&mut self, controller: usize) -> &mut Controller {
        match controller {
            0 => &mut self.modulation_wheel,
            1 => &mut self.foot_controller,
            2 => &mut self.breath_controller,
            _ => &mut self.aftertouch,
        }
    }
}

/// A function parameter, by the number that the DX7's parameter changes of
/// the function group give it, 64 to 77 \[DX7-MANUAL\] \[DEXED-SYSEX\]: mono or
/// poly; the pitch bend's range and step; the portamento's mode, glissando and
/// time; and the range and the assignment of the modulation wheel, the foot
/// controller, the breath controller and the aftertouch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FunctionParameter(u8);

impl FunctionParameter {
    /// The number of the first, 64.
    pub const FIRST: u8 = 64;

    /// The number of the last, 77.
    pub const LAST: u8 = 77;

    /// The parameter of a number, if the number is 64 to 77.
    #[must_use]
    pub fn new(number: u8) -> Option<Self> {
        (Self::FIRST..=Self::LAST).contains(&number).then_some(Self(number))
    }

    /// Its number.
    #[must_use]
    pub const fn number(self) -> u8 {
        self.0
    }

    /// Every parameter, in the order of their numbers.
    pub fn all() -> impl Iterator<Item = Self> {
        (Self::FIRST..=Self::LAST).map(Self)
    }

    /// The largest step it takes: 1 for mono or poly, the portamento's mode and
    /// glissando, 12 for the pitch bend's range and step, 99 for the portamento's
    /// time and the controllers' ranges, and 7 for their assignments.
    #[must_use]
    pub fn largest(self) -> u8 {
        match self.field() {
            Field::Polyphony | Field::PortamentoMode | Field::Glissando => 1,
            Field::PitchBendRange | Field::PitchBendStep => 12,
            Field::PortamentoTime | Field::Range(_) => 99,
            Field::Assign(_) => 7,
        }
    }

    /// What it names.
    fn field(self) -> Field {
        match self.0 {
            64 => Field::Polyphony,
            65 => Field::PitchBendRange,
            66 => Field::PitchBendStep,
            67 => Field::PortamentoMode,
            68 => Field::Glissando,
            69 => Field::PortamentoTime,
            number if number % 2 == 0 => Field::Range(usize::from((number - 70) / 2)),
            number => Field::Assign(usize::from((number - 71) / 2)),
        }
    }
}

impl fmt::Display for FunctionParameter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        const CONTROLLERS: [&str; 4] = ["modulation wheel", "foot controller", "breath controller", "aftertouch"];
        match self.field() {
            Field::Polyphony => formatter.write_str("mono/poly"),
            Field::PitchBendRange => formatter.write_str("pitch bend range"),
            Field::PitchBendStep => formatter.write_str("pitch bend step"),
            Field::PortamentoMode => formatter.write_str("portamento mode"),
            Field::Glissando => formatter.write_str("glissando"),
            Field::PortamentoTime => formatter.write_str("portamento time"),
            Field::Range(controller) => write!(formatter, "{} range", CONTROLLERS[controller]),
            Field::Assign(controller) => write!(formatter, "{} assign", CONTROLLERS[controller]),
        }
    }
}

/// What a function parameter's number names.
#[derive(Debug, Clone, Copy)]
enum Field {
    Polyphony,
    PitchBendRange,
    PitchBendStep,
    PortamentoMode,
    Glissando,
    PortamentoTime,
    /// The range of the controller of this index, 0 for the modulation wheel.
    Range(usize),
    /// The assignment of the controller of this index.
    Assign(usize),
}
