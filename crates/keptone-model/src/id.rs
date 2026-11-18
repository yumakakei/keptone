// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The parameters' stable numbers and names: the IDs a host knows them by and
//! the names the native state writes them by (docs/DESIGN.md §7.4).
//!
//! A [`ParameterId`] holds an [`Area`] in its bits 31 to 28, a part's index in
//! bits 27 to 12 and a number within the area in bits 11 to 0, so that a
//! part's parameters keep their IDs whatever `part.count` is. The settings'
//! area belongs to no part, its index always 0, and its numbers are the
//! places of [`crate::settings::SPECS`]. Each parameter has a name as well: a
//! part's is its area's and its place in the model, as `voice.op6.eg.rate.1`,
//! `function.modulation_wheel.assign.pitch` and `part.lowest_key`; a
//! setting's is its name in docs/MATH.md §17. A number or a name once given is
//! never given to another parameter, nor taken back: what comes later takes
//! numbers not yet given.
//!
//! The voice's numbers are the DX7's, 0 to 144, as its single-voice dump and
//! its parameter changes give them \[DEXED-SYSEX\], but for the algorithm's
//! value, its number 1 to 32 rather than its step; its six operators'
//! switches, which the DX7 carries together as its parameter 155, take 155 to
//! 160, OP1's first. The bytes of the name, the DX7's 145 to 154, are no
//! parameter here: the name is data, as a fractional scaling is. The other
//! areas number their parameters in the order of the model's fields.
//! [`PartParameter`] is a parameter of a part, and [`Part::value`] and
//! [`Part::set_value`] read and set it as a binary64 of its [`Kind`].

use std::fmt;
use std::num::NonZeroU32;

use crate::dx7ii::Index;
use crate::function::{Function, Polyphony, PortamentoMode};
use crate::part::{KeyFilter, NoteShift, Part};
use crate::supplement::{BendMode, LfoTrigger, PitchEgRange, ScalingMode, Supplement};
use crate::voice::{self, Algorithm, Parameter, Value, Voice};

/// What of Keptone's a parameter belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Area {
    /// The settings (docs/MATH.md §17), which belong to no part: 0.
    Setting,
    /// The voice of a part's patch: 1.
    Voice,
    /// A part's function data: 2.
    Function,
    /// The supplement of a part's patch: 3.
    Supplement,
    /// What a part has of its own: 4.
    Part,
}

impl Area {
    /// Its number, the four highest bits of its parameters' IDs.
    #[must_use]
    pub const fn number(self) -> u8 {
        match self {
            Self::Setting => 0,
            Self::Voice => 1,
            Self::Function => 2,
            Self::Supplement => 3,
            Self::Part => 4,
        }
    }

    /// The area of a number, if one has it.
    #[must_use]
    pub const fn of_number(number: u8) -> Option<Self> {
        match number {
            0 => Some(Self::Setting),
            1 => Some(Self::Voice),
            2 => Some(Self::Function),
            3 => Some(Self::Supplement),
            4 => Some(Self::Part),
            _ => None,
        }
    }

    /// Its name, which begins the names of a part's parameters in it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Setting => "setting",
            Self::Voice => "voice",
            Self::Function => "function",
            Self::Supplement => "supplement",
            Self::Part => "part",
        }
    }
}

/// A parameter's ID: what a host knows it by, for as long as Keptone is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParameterId(u32);

impl ParameterId {
    /// How many numbers an area has room for: 2¹².
    pub const NUMBERS: u16 = 1 << 12;

    /// The ID of a number of an area in a part, if the number is below 2¹²
    /// and the part is 0 where the area is the settings'.
    #[must_use]
    pub fn new(area: Area, part: u16, number: u16) -> Option<Self> {
        (number < Self::NUMBERS && (area != Area::Setting || part == 0))
            .then(|| Self(u32::from(area.number()) << 28 | u32::from(part) << 12 | u32::from(number)))
    }

    /// The ID that 32 bits are, if they are one.
    #[must_use]
    pub fn from_bits(bits: u32) -> Option<Self> {
        let area = Area::of_number(u8::try_from(bits >> 28).ok()?)?;
        let part = u16::try_from(bits >> 12 & 0xffff).ok()?;
        let number = u16::try_from(bits & 0xfff).ok()?;
        Self::new(area, part, number)
    }

    /// Its 32 bits.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Its area.
    #[must_use]
    pub fn area(self) -> Area {
        // Only `new` makes an ID, from an area that is one.
        u8::try_from(self.0 >> 28)
            .ok()
            .and_then(Area::of_number)
            .unwrap_or(Area::Part)
    }

    /// The index of its part, 0 for the first, and for a setting.
    #[must_use]
    pub fn part(self) -> u16 {
        u16::try_from(self.0 >> 12 & 0xffff).unwrap_or_default()
    }

    /// Its number within its area.
    #[must_use]
    pub fn number(self) -> u16 {
        u16::try_from(self.0 & 0xfff).unwrap_or_default()
    }
}

impl fmt::Display for ParameterId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:#010x}", self.0)
    }
}

/// What values a parameter takes, each a binary64.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// A number from 0 to this, which may lie between the integers.
    Number(f64),
    /// A number from minus this to this, which may lie between the integers.
    Signed(f64),
    /// An integer from the first to the second.
    Integer(u32, u32),
    /// The index of one of these, each the name the native state writes it
    /// by.
    Choice(&'static [&'static str]),
    /// 0, off, or 1, on.
    Switch,
    /// A count of 32 bits, 0 for none.
    Count,
}

/// A parameter of a part: an area other than the settings', and a number in
/// it that names one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PartParameter {
    /// Its area.
    area: Area,
    /// Its number in the area.
    number: u16,
}

impl PartParameter {
    /// How many parameters a part has: 151 of the voice, 22 of the function
    /// data, 50 of the supplement and 10 of its own.
    pub const COUNT: usize = 233;

    /// The parameter of a number in an area, if the number names one.
    #[must_use]
    pub fn new(area: Area, number: u16) -> Option<Self> {
        let names = match area {
            Area::Setting => false,
            Area::Voice => number <= LAST_OF_VOICE || (FIRST_SWITCH..FIRST_SWITCH + 6).contains(&number),
            Area::Function => usize::from(number) < FUNCTION.len(),
            Area::Supplement => usize::from(number) < OPERATOR_SUPPLEMENT + SUPPLEMENT.len(),
            Area::Part => usize::from(number) < PART.len(),
        };
        names.then_some(Self { area, number })
    }

    /// Every parameter of a part, in the order of their IDs.
    pub fn all() -> impl Iterator<Item = Self> {
        let of = |area, numbers: std::ops::Range<u16>| numbers.map(move |number| Self { area, number });
        of(Area::Voice, 0..LAST_OF_VOICE + 1)
            .chain(of(Area::Voice, FIRST_SWITCH..FIRST_SWITCH + 6))
            .chain(of(Area::Function, 0..length(FUNCTION.len())))
            .chain(of(Area::Supplement, 0..length(OPERATOR_SUPPLEMENT + SUPPLEMENT.len())))
            .chain(of(Area::Part, 0..length(PART.len())))
    }

    /// The parameter of a name, if one has it.
    #[must_use]
    pub fn of_name(name: &str) -> Option<Self> {
        Self::all().find(|parameter| parameter.name() == name)
    }

    /// Its area.
    #[must_use]
    pub const fn area(self) -> Area {
        self.area
    }

    /// Its number in its area.
    #[must_use]
    pub const fn number(self) -> u16 {
        self.number
    }

    /// Its ID in the part of an index.
    #[must_use]
    pub fn id(self, part: u16) -> ParameterId {
        ParameterId(u32::from(self.area.number()) << 28 | u32::from(part) << 12 | u32::from(self.number))
    }

    /// Its name, which never changes.
    #[must_use]
    pub fn name(self) -> String {
        let n = usize::from(self.number);
        match self.area {
            Area::Voice => voice_name(self.number),
            Area::Function => format!("function.{}", FUNCTION[n].name),
            Area::Supplement if n < OPERATOR_SUPPLEMENT => {
                format!("supplement.op{}.{}", n / 2 + 1, OPERATOR_SUPPLEMENT_NAMES[n % 2])
            }
            Area::Supplement => format!("supplement.{}", SUPPLEMENT[n - OPERATOR_SUPPLEMENT].name),
            Area::Part | Area::Setting => format!("part.{}", PART[n].name),
        }
    }

    /// What values it takes.
    #[must_use]
    pub fn kind(self) -> Kind {
        let n = usize::from(self.number);
        match self.area {
            Area::Voice => voice_kind(self.number),
            Area::Function => FUNCTION[n].kind,
            Area::Supplement if n < OPERATOR_SUPPLEMENT => OPERATOR_SUPPLEMENT_KINDS[n % 2],
            Area::Supplement => SUPPLEMENT[n - OPERATOR_SUPPLEMENT].kind,
            Area::Part | Area::Setting => PART[n].kind,
        }
    }
}

impl fmt::Display for PartParameter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name())
    }
}

impl Part {
    /// A parameter's value, if the part has the parameter: the supplement's
    /// are its patch's only where the patch has one.
    #[must_use]
    pub fn value(&self, parameter: PartParameter) -> Option<f64> {
        let n = usize::from(parameter.number);
        match parameter.area {
            Area::Voice => Some(voice_value(&self.patch.voice, parameter.number)),
            Area::Function => Some((FUNCTION[n].get)(&self.function)),
            Area::Supplement => self
                .patch
                .supplement
                .as_ref()
                .map(|supplement| supplement_value(supplement, n)),
            Area::Part | Area::Setting => Some((PART[n].get)(self)),
        }
    }

    /// Sets a parameter to a value, if the part has the parameter and the
    /// value is one it takes, and says whether it did. A value out of the
    /// parameter's [`Kind`], and a supplement's parameter where the patch has
    /// no supplement, leave the part as it was.
    pub fn set_value(&mut self, parameter: PartParameter, value: f64) -> bool {
        let n = usize::from(parameter.number);
        match parameter.area {
            Area::Voice => set_voice_value(&mut self.patch.voice, parameter.number, value),
            Area::Function => (FUNCTION[n].set)(&mut self.function, value),
            Area::Supplement => self
                .patch
                .supplement
                .as_mut()
                .is_some_and(|supplement| set_supplement_value(supplement, n, value)),
            Area::Part | Area::Setting => (PART[n].set)(self, value),
        }
    }
}

/// The highest of the voice's numbers that are the DX7's, the transpose's.
const LAST_OF_VOICE: u16 = 144;

/// The number of OP1's switch; OP2's to OP6's follow it.
const FIRST_SWITCH: u16 = 155;

/// The DX7's number of the algorithm, whose value here is its number.
const ALGORITHM: u16 = 134;

/// The names of the curves of keyboard level scaling, by their steps.
const CURVES: &[&str] = &["-lin", "-exp", "+exp", "+lin"];

/// The names of an operator's modes, by their steps.
const MODES: &[&str] = &["ratio", "fixed"];

/// The names of the LFO's waves, by their steps.
const WAVES: &[&str] = &["triangle", "saw-down", "saw-up", "square", "sine", "sample-hold"];

/// The names of an operator's parameters, after `voice.opN.`, and their kinds,
/// in the order of the DX7's numbers within an operator's 21.
const OPERATOR: [(&str, Kind); 21] = [
    ("eg.rate.1", Kind::Number(99.0)),
    ("eg.rate.2", Kind::Number(99.0)),
    ("eg.rate.3", Kind::Number(99.0)),
    ("eg.rate.4", Kind::Number(99.0)),
    ("eg.level.1", Kind::Number(99.0)),
    ("eg.level.2", Kind::Number(99.0)),
    ("eg.level.3", Kind::Number(99.0)),
    ("eg.level.4", Kind::Number(99.0)),
    ("break_point", Kind::Number(99.0)),
    ("left_depth", Kind::Number(99.0)),
    ("right_depth", Kind::Number(99.0)),
    ("left_curve", Kind::Choice(CURVES)),
    ("right_curve", Kind::Choice(CURVES)),
    ("rate_scaling", Kind::Number(7.0)),
    ("amplitude_sensitivity", Kind::Number(3.0)),
    ("velocity_sensitivity", Kind::Number(7.0)),
    ("output_level", Kind::Number(99.0)),
    ("mode", Kind::Choice(MODES)),
    ("coarse", Kind::Number(31.0)),
    ("fine", Kind::Number(99.0)),
    ("detune", Kind::Number(14.0)),
];

/// The names of the voice's own parameters, after `voice.`, and their kinds,
/// the DX7's 126 to 144.
const VOICE: [(&str, Kind); 19] = [
    ("pitch_eg.rate.1", Kind::Number(99.0)),
    ("pitch_eg.rate.2", Kind::Number(99.0)),
    ("pitch_eg.rate.3", Kind::Number(99.0)),
    ("pitch_eg.rate.4", Kind::Number(99.0)),
    ("pitch_eg.level.1", Kind::Number(99.0)),
    ("pitch_eg.level.2", Kind::Number(99.0)),
    ("pitch_eg.level.3", Kind::Number(99.0)),
    ("pitch_eg.level.4", Kind::Number(99.0)),
    ("algorithm", Kind::Integer(1, 32)),
    ("feedback", Kind::Number(7.0)),
    ("oscillator_sync", Kind::Switch),
    ("lfo.speed", Kind::Number(99.0)),
    ("lfo.delay", Kind::Number(99.0)),
    ("lfo.pitch_depth", Kind::Number(99.0)),
    ("lfo.amplitude_depth", Kind::Number(99.0)),
    ("lfo.sync", Kind::Switch),
    ("lfo.wave", Kind::Choice(WAVES)),
    ("pitch_sensitivity", Kind::Number(7.0)),
    ("transpose", Kind::Number(48.0)),
];

/// The name of a voice's parameter of a number that names one.
fn voice_name(number: u16) -> String {
    let n = usize::from(number);
    match number {
        0..=125 => format!("voice.op{}.{}", 6 - n / 21, OPERATOR[n % 21].0),
        126..=LAST_OF_VOICE => format!("voice.{}", VOICE[n - 126].0),
        _ => format!("voice.op{}.on", number - FIRST_SWITCH + 1),
    }
}

/// The kind of a voice's parameter of a number that names one.
fn voice_kind(number: u16) -> Kind {
    let n = usize::from(number);
    match number {
        0..=125 => OPERATOR[n % 21].1,
        126..=LAST_OF_VOICE => VOICE[n - 126].1,
        _ => Kind::Switch,
    }
}

/// The voice's parameter that the DX7 numbers so.
fn dx7_parameter(number: u16) -> Option<Parameter> {
    u8::try_from(number).ok().and_then(Parameter::new)
}

/// The value of a voice's parameter of a number that names one.
fn voice_value(voice: &Voice, number: u16) -> f64 {
    match number {
        ALGORITHM => f64::from(voice.algorithm.number()),
        0..=LAST_OF_VOICE => dx7_parameter(number).map_or(0.0, |parameter| voice.value(parameter)),
        _ => f64::from(voice.operators[usize::from(number - FIRST_SWITCH)].on),
    }
}

/// Sets a voice's parameter of a number that names one, as
/// [`Part::set_value`] does.
fn set_voice_value(voice: &mut Voice, number: u16, value: f64) -> bool {
    match number {
        ALGORITHM => integer_of(value, 1, 32)
            .and_then(|number| u8::try_from(number).ok())
            .and_then(Algorithm::new)
            .map(|algorithm| voice.algorithm = algorithm)
            .is_some(),
        0..=LAST_OF_VOICE => dx7_parameter(number).is_some_and(|parameter| voice.set_value(parameter, value)),
        _ => voice::switch_of(value)
            .map(|on| voice.operators[usize::from(number - FIRST_SWITCH)].on = on)
            .is_some(),
    }
}

/// How a parameter of a `T` is named, read and set.
struct Accessor<T> {
    /// Its name, after its area's.
    name: &'static str,
    /// What values it takes.
    kind: Kind,
    /// Its value.
    get: fn(&T) -> f64,
    /// Sets it to a value, if the value is one it takes, and says whether it
    /// did.
    set: fn(&mut T, f64) -> bool,
}

/// An accessor of a field that is a [`Value`] of the scale up to `$max`.
macro_rules! number {
    ($name:literal, $max:literal, $($field:ident).+) => {
        Accessor {
            name: $name,
            kind: Kind::Number($max),
            get: |t| t.$($field).+.get(),
            set: |t, value| set_number(&mut t.$($field).+, value),
        }
    };
}

/// An accessor of a field that is a switch.
macro_rules! switch {
    ($name:literal, $($field:ident).+) => {
        Accessor {
            name: $name,
            kind: Kind::Switch,
            get: |t| f64::from(t.$($field).+),
            set: |t, value| voice::switch_of(value).map(|on| t.$($field).+ = on).is_some(),
        }
    };
}

/// An accessor of a field that is an enumeration of `$type`, whose steps the
/// names `$names` give.
macro_rules! choice {
    ($name:literal, $names:expr, $type:ident, $($field:ident).+) => {
        Accessor {
            name: $name,
            kind: Kind::Choice($names),
            get: |t| f64::from(t.$($field).+.step()),
            set: |t, value| {
                voice::step_of(value, count($names))
                    .and_then($type::from_step)
                    .map(|choice| t.$($field).+ = choice)
                    .is_some()
            },
        }
    };
}

/// An accessor of a field that is an [`Index`] up to `$max`.
macro_rules! index {
    ($name:literal, $max:literal, $($field:ident).+) => {
        Accessor {
            name: $name,
            kind: Kind::Integer(0, $max),
            get: |t| f64::from(t.$($field).+.get()),
            set: |t, value| index_of(value).map(|index| t.$($field).+ = index).is_some(),
        }
    };
}

/// The names of polyphony's choices.
const POLYPHONY: &[&str] = &["poly", "mono"];

/// The names of the portamento's modes.
const PORTAMENTO: &[&str] = &["retain", "follow"];

/// The function data's parameters, after `function.`.
const FUNCTION: [Accessor<Function>; 22] = [
    choice!("polyphony", POLYPHONY, Polyphony, polyphony),
    number!("pitch_bend_range", 12.0, pitch_bend_range),
    number!("pitch_bend_step", 12.0, pitch_bend_step),
    choice!("portamento_mode", PORTAMENTO, PortamentoMode, portamento_mode),
    switch!("glissando", glissando),
    number!("portamento_time", 99.0, portamento_time),
    number!("modulation_wheel.range", 99.0, modulation_wheel.range),
    switch!("modulation_wheel.assign.pitch", modulation_wheel.assign.pitch),
    switch!("modulation_wheel.assign.amplitude", modulation_wheel.assign.amplitude),
    switch!("modulation_wheel.assign.eg_bias", modulation_wheel.assign.eg_bias),
    number!("foot_controller.range", 99.0, foot_controller.range),
    switch!("foot_controller.assign.pitch", foot_controller.assign.pitch),
    switch!("foot_controller.assign.amplitude", foot_controller.assign.amplitude),
    switch!("foot_controller.assign.eg_bias", foot_controller.assign.eg_bias),
    number!("breath_controller.range", 99.0, breath_controller.range),
    switch!("breath_controller.assign.pitch", breath_controller.assign.pitch),
    switch!("breath_controller.assign.amplitude", breath_controller.assign.amplitude),
    switch!("breath_controller.assign.eg_bias", breath_controller.assign.eg_bias),
    number!("aftertouch.range", 99.0, aftertouch.range),
    switch!("aftertouch.assign.pitch", aftertouch.assign.pitch),
    switch!("aftertouch.assign.amplitude", aftertouch.assign.amplitude),
    switch!("aftertouch.assign.eg_bias", aftertouch.assign.eg_bias),
];

/// The supplement's parameters of its operators, two each from OP1's, which
/// come first.
const OPERATOR_SUPPLEMENT: usize = 12;

/// The names of an operator's parameters of the supplement, after
/// `supplement.opN.`.
const OPERATOR_SUPPLEMENT_NAMES: [&str; 2] = ["scaling_mode", "amplitude_sensitivity"];

/// The names of the scaling's modes.
const SCALING_MODES: &[&str] = &["normal", "fractional"];

/// The kinds of an operator's parameters of the supplement.
const OPERATOR_SUPPLEMENT_KINDS: [Kind; 2] = [Kind::Choice(SCALING_MODES), Kind::Number(7.0)];

/// The value of the supplement's parameter of a number that names one.
fn supplement_value(supplement: &Supplement, n: usize) -> f64 {
    match supplement.operators.get(n / 2) {
        Some(operator) if n < OPERATOR_SUPPLEMENT && n.is_multiple_of(2) => f64::from(operator.scaling_mode.step()),
        Some(operator) if n < OPERATOR_SUPPLEMENT => operator.amplitude_sensitivity.get(),
        _ => (SUPPLEMENT[n - OPERATOR_SUPPLEMENT].get)(supplement),
    }
}

/// Sets the supplement's parameter of a number that names one, as
/// [`Part::set_value`] does.
fn set_supplement_value(supplement: &mut Supplement, n: usize, value: f64) -> bool {
    match supplement.operators.get_mut(n / 2) {
        Some(operator) if n < OPERATOR_SUPPLEMENT && n.is_multiple_of(2) => voice::step_of(value, 2)
            .and_then(ScalingMode::from_step)
            .map(|mode| operator.scaling_mode = mode)
            .is_some(),
        Some(operator) if n < OPERATOR_SUPPLEMENT => set_number(&mut operator.amplitude_sensitivity, value),
        _ => (SUPPLEMENT[n - OPERATOR_SUPPLEMENT].set)(supplement, value),
    }
}

/// The names of the pitch envelope's ranges.
const PITCH_EG_RANGES: &[&str] = &["eight-octaves", "four-octaves", "one-octave", "half-octave"];

/// The names of the LFO's triggers.
const LFO_TRIGGERS: &[&str] = &["single", "multi"];

/// The names of the pitch bend's modes.
const BEND_MODES: &[&str] = &["normal", "lowest", "highest", "key-on"];

/// The supplement's parameters after its operators', after `supplement.`.
const SUPPLEMENT: [Accessor<Supplement>; 38] = [
    choice!("pitch_eg_range", PITCH_EG_RANGES, PitchEgRange, pitch_eg_range),
    choice!("lfo_trigger", LFO_TRIGGERS, LfoTrigger, lfo_trigger),
    switch!("pitch_eg_velocity", pitch_eg_velocity),
    choice!("polyphony", POLYPHONY, Polyphony, polyphony),
    switch!("unison", unison),
    number!("pitch_bend_range", 12.0, pitch_bend_range),
    number!("pitch_bend_step", 12.0, pitch_bend_step),
    choice!("pitch_bend_mode", BEND_MODES, BendMode, pitch_bend_mode),
    number!("random_pitch", 7.0, random_pitch),
    choice!("portamento_mode", PORTAMENTO, PortamentoMode, portamento_mode),
    number!("portamento_step", 12.0, portamento_step),
    number!("portamento_time", 99.0, portamento_time),
    number!("modulation_wheel.pitch", 99.0, modulation_wheel.pitch),
    number!("modulation_wheel.amplitude", 99.0, modulation_wheel.amplitude),
    number!("modulation_wheel.eg_bias", 99.0, modulation_wheel.eg_bias),
    number!("foot_controller_1.pitch", 99.0, foot_controller_1.pitch),
    number!("foot_controller_1.amplitude", 99.0, foot_controller_1.amplitude),
    number!("foot_controller_1.eg_bias", 99.0, foot_controller_1.eg_bias),
    number!("foot_controller_1_volume", 99.0, foot_controller_1_volume),
    number!("breath_controller.pitch", 99.0, breath_controller.pitch),
    number!("breath_controller.amplitude", 99.0, breath_controller.amplitude),
    number!("breath_controller.eg_bias", 99.0, breath_controller.eg_bias),
    number!("breath_controller_pitch_bias", 100.0, breath_controller_pitch_bias),
    number!("aftertouch.pitch", 99.0, aftertouch.pitch),
    number!("aftertouch.amplitude", 99.0, aftertouch.amplitude),
    number!("aftertouch.eg_bias", 99.0, aftertouch.eg_bias),
    number!("aftertouch_pitch_bias", 100.0, aftertouch_pitch_bias),
    number!("pitch_eg_rate_scaling", 7.0, pitch_eg_rate_scaling),
    number!("foot_controller_2.pitch", 99.0, foot_controller_2.pitch),
    number!("foot_controller_2.amplitude", 99.0, foot_controller_2.amplitude),
    number!("foot_controller_2.eg_bias", 99.0, foot_controller_2.eg_bias),
    number!("foot_controller_2_volume", 99.0, foot_controller_2_volume),
    number!("midi_control.pitch", 99.0, midi_control.pitch),
    number!("midi_control.amplitude", 99.0, midi_control.amplitude),
    number!("midi_control.eg_bias", 99.0, midi_control.eg_bias),
    number!("midi_control_volume", 99.0, midi_control_volume),
    number!("unison_detune", 7.0, unison_detune),
    switch!("foot_controller_1_as_slider", foot_controller_1_as_slider),
];

/// The names of the key filters.
const KEY_FILTERS: &[&str] = &["all", "odd", "even"];

/// What a part has of its own, after `part.`.
const PART: [Accessor<Part>; 10] = [
    switch!("on", on),
    switch!("omni", omni),
    index!("group", 15, group),
    index!("channel", 15, channel),
    index!("lowest_key", 127, lowest_key),
    index!("highest_key", 127, highest_key),
    choice!("key_filter", KEY_FILTERS, KeyFilter, key_filter),
    Accessor {
        name: "note_shift",
        kind: Kind::Signed(24.0),
        get: |part| part.note_shift.get(),
        set: |part, value| NoteShift::new(value).map(|shift| part.note_shift = shift).is_some(),
    },
    Accessor {
        name: "note_limit",
        kind: Kind::Count,
        get: |part| part.note_limit.map_or(0.0, |limit| f64::from(limit.get())),
        set: |part, value| {
            integer_of(value, 0, u32::MAX)
                .map(|limit| part.note_limit = NonZeroU32::new(limit))
                .is_some()
        },
    },
    switch!("alternate", alternate),
];

/// Sets a value of a scale to a number, if the number is on the scale, and
/// says whether it did.
fn set_number<const MAX: u8>(place: &mut Value<MAX>, value: f64) -> bool {
    Value::new(value).map(|value| *place = value).is_some()
}

/// The index a number is, if it is an integer from 0 to `MAX`.
fn index_of<const MAX: u8>(value: f64) -> Option<Index<MAX>> {
    integer_of(value, 0, u32::from(MAX))
        .and_then(|number| u8::try_from(number).ok())
        .and_then(Index::new)
}

/// The integer a number is, if it is one from `least` to `most`.
fn integer_of(value: f64, least: u32, most: u32) -> Option<u32> {
    (value.fract() == 0.0 && (f64::from(least)..=f64::from(most)).contains(&value)).then(|| integer(value))
}

/// An integer of [0, 2³² − 1] as 32 bits.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "an integer of [0, 2³² − 1] converts exactly"
)]
fn integer(value: f64) -> u32 {
    value as u32
}

/// How many names a choice has, as the count of steps it takes.
const fn count(names: &[&str]) -> u8 {
    // No choice has more than 255 names.
    if names.len() > 255 { 255 } else { length_u8(names.len()) }
}

/// A length of at most 255 as a byte.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "the length is at most 255"
)]
const fn length_u8(length: usize) -> u8 {
    length as u8
}

/// A table's length as the end of a range of numbers.
fn length(length: usize) -> u16 {
    u16::try_from(length).unwrap_or(u16::MAX)
}
