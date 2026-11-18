// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The DX7II's performances, its system set-up, its micro tunings and its
//! fractional scalings, every parameter of which is typed.
//!
//! A [`Performance`] is the DX7II's way of playing its voices, the PCED and
//! PMEM formats of \[DX7II-MANUAL\]: one voice, or two, A and B, together or
//! split across the keys, with the micro tuning, the foot switches, the note
//! shifts, the volume and its balance, the continuous sliders' assignments, and
//! the pan with its envelope; and a name of twenty letters. The DX7s plays one
//! voice only \[DX7S-MANUAL\]. The [`SystemSetup`] is how the instrument talks
//! MIDI: its channels, its controllers' numbers, its program changes and their
//! table, its cartridge banks, its memory protect and its master tune. A
//! [`MicroTuning`] gives each of the 128 keys its pitch, and a
//! [`FractionalScaling`] gives each operator a level for each of 40 groups of
//! keys, in place of the DX7's level scaling. What each does is Keptone's
//! own, for the engine and the host to say (docs/DESIGN.md §6.4); these types
//! hold them, each number a [`Value`] of its scale, a [`KeyPitch`] or an
//! [`Index`] that names a thing, each choice an enumeration and each switch a
//! `bool`, and the offset of a fractional scaling its byte, which holds it in
//! two's complement. Where the DX7II's manual and the DX7s's give a
//! parameter different ranges, it takes them both.
//!
//! [`PerformanceParameter`] numbers a performance's parameters as the
//! parameter changes of the performance do, 0 to 50, and [`SystemParameter`]
//! the system set-up's bytes, 0 to 101, of which 0 to 19 are the system
//! set-up's parameter changes' 64 to 83. The DX7s's dump of its system set-up
//! is of 85 bytes, without the 17 that have no name
//! ([`SystemParameter::in_dx7s_dump`]). [`ScalingParameter`] numbers a
//! fractional scaling's parameters as its dumps place them, 0 to 245.

use std::fmt;

use crate::voice::{self, Envelope, Rounding, Value};

/// A whole number from 0 to `MAX` that names a thing, a voice, a key, a
/// channel or a controller, which no value between two of them would mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Index<const MAX: u8>(u8);

impl<const MAX: u8> Index<MAX> {
    /// The index of a number, if the number is no more than `MAX`.
    #[must_use]
    pub fn new(number: u8) -> Option<Self> {
        (number <= MAX).then_some(Self(number))
    }

    /// The index of a number, or `MAX` if the number is more.
    pub(crate) const fn saturating(number: u8) -> Self {
        Self(if number > MAX { MAX } else { number })
    }

    /// Its number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Every index, from 0 to `MAX`.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..=MAX).map(Self)
    }
}

/// Which of the two channels, A and B, a switch of a performance acts on, a bit
/// each: bit 0 A, bit 1 B \[DX7II-MANUAL\].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Channels {
    /// Channel A.
    pub a: bool,
    /// Channel B.
    pub b: bool,
}

impl Channels {
    /// The switches of a step's two bits.
    const fn of(step: u8) -> Self {
        Self {
            a: step & 1 != 0,
            b: step & 2 != 0,
        }
    }

    /// Their step.
    fn step(self) -> u8 {
        u8::from(self.a) | u8::from(self.b) << 1
    }
}

/// How the keys play the two voices, by its step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayMode {
    /// 0: voice A alone.
    Single,
    /// 1: A and B together.
    Dual,
    /// 2: A below the split point and B from it.
    Split,
}

/// What the foot switch does, by its step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FootSwitchAssign {
    /// 0: sustain.
    Sustain,
    /// 1: portamento.
    Portamento,
    /// 2: key hold, sostenuto.
    KeyHold,
    /// 3: soft.
    Soft,
}

/// How the two voices are panned, by its step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanMode {
    /// 0: both to both outputs.
    Mix,
    /// 1: both panned.
    OnOn,
    /// 2: A panned, B not.
    OnOff,
    /// 3: B panned, A not.
    OffOn,
}

/// What moves the pan, by its step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanAssign {
    /// 0: the LFO.
    Lfo,
    /// 1: velocity.
    Velocity,
    /// 2: the key's number.
    Key,
}

/// Which voices the two continuous sliders act on, a bit each \[DX7S-MANUAL\]:
/// bit 0 the first on A, bit 1 the first on B, bit 2 the second on A and bit 3
/// the second on B. \[DX7II-MANUAL\] gives the step as 0 to 3 and the bits as
/// "b1,3:B,b0,2:A".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[expect(clippy::struct_excessive_bools, reason = "the step holds four switches, a bit each")]
pub struct SliderSwitches {
    /// The first slider on A.
    pub slider_1_a: bool,
    /// The first slider on B.
    pub slider_1_b: bool,
    /// The second slider on A.
    pub slider_2_a: bool,
    /// The second slider on B.
    pub slider_2_b: bool,
}

impl SliderSwitches {
    /// The switches of a step's four bits.
    const fn of(step: u8) -> Self {
        Self {
            slider_1_a: step & 1 != 0,
            slider_1_b: step & 2 != 0,
            slider_2_a: step & 4 != 0,
            slider_2_b: step & 8 != 0,
        }
    }

    /// Their step.
    fn step(self) -> u8 {
        u8::from(self.slider_1_a)
            | u8::from(self.slider_1_b) << 1
            | u8::from(self.slider_2_a) << 2
            | u8::from(self.slider_2_b) << 3
    }
}

/// A performance's name: twenty bytes of seven bits, ASCII as the dumps have
/// it, kept as they come.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PerformanceName([u8; 20]);

impl PerformanceName {
    /// The name of twenty bytes, if each is below 128.
    #[must_use]
    pub fn new(bytes: [u8; 20]) -> Option<Self> {
        bytes.iter().all(|&byte| byte < 0x80).then_some(Self(bytes))
    }

    /// Its bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 20] {
        self.0
    }
}

/// The DX7II's performance: what its PCED format holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Performance {
    /// How the keys play the two voices (0).
    pub play_mode: PlayMode,
    /// Voice A's number, 0 to 63 inside and 64 to 127 on the cartridge (1).
    pub voice_a: Index<127>,
    /// Voice B's number (2).
    pub voice_b: Index<127>,
    /// The micro tuning's table (3).
    pub micro_tuning_table: Index<74>,
    /// The micro tuning's key, 0 for C to 11 (4).
    pub micro_tuning_key: Index<11>,
    /// Which voices the micro tuning tunes (5).
    pub micro_tuning: Channels,
    /// The dual mode's detune, 0 to 7 (6).
    pub dual_detune: Value<7>,
    /// The key from which voice B plays in the split mode (7).
    pub split_point: Index<127>,
    /// Whether a voice's envelope is damped when the voice is taken for another
    /// note (8).
    pub eg_forced_damp: bool,
    /// Which voices the sustain foot switch holds (9).
    pub sustain: Channels,
    /// What the foot switch does (10).
    pub foot_switch_assign: FootSwitchAssign,
    /// Which voices the foot switch acts on (11).
    pub foot_switch: Channels,
    /// The soft pedal's range, 0 to 7 (12).
    pub soft_pedal_range: Value<7>,
    /// The note shift of voice A, and of both in the dual mode, 0 to 48, 24
    /// being none (13).
    pub note_shift_a: Value<48>,
    /// The note shift of voice B in the split mode (14).
    pub note_shift_b: Value<48>,
    /// The balance of the two voices' volumes, 0 to 100, 50 being even (15).
    pub balance: Value<100>,
    /// The total volume, 0 to 99 (16).
    pub total_volume: Value<99>,
    /// What the first continuous slider controls, 0 for nothing (17).
    pub slider_1: Index<105>,
    /// What the second continuous slider controls: 0 to 109 on the DX7II, 0 to
    /// 105 on the DX7s (18).
    pub slider_2: Index<109>,
    /// Which voices the sliders act on (19).
    pub slider_switches: SliderSwitches,
    /// How the voices are panned (20).
    pub pan_mode: PanMode,
    /// How far the pan moves, 0 to 99 (21).
    pub pan_range: Value<99>,
    /// What moves the pan (22).
    pub pan_assign: PanAssign,
    /// The pan's envelope, its rates 23 to 26 and its levels 27 to 30.
    pub pan_envelope: Envelope,
    /// The name (31 to 50).
    pub name: PerformanceName,
}

impl Performance {
    /// A performance of every step 0: the start of one whose every parameter a
    /// codec sets.
    pub(crate) const fn zero() -> Self {
        Self {
            play_mode: PlayMode::Single,
            voice_a: Index(0),
            voice_b: Index(0),
            micro_tuning_table: Index(0),
            micro_tuning_key: Index(0),
            micro_tuning: Channels::of(0),
            dual_detune: Value::ZERO,
            split_point: Index(0),
            eg_forced_damp: false,
            sustain: Channels::of(0),
            foot_switch_assign: FootSwitchAssign::Sustain,
            foot_switch: Channels::of(0),
            soft_pedal_range: Value::ZERO,
            note_shift_a: Value::ZERO,
            note_shift_b: Value::ZERO,
            balance: Value::ZERO,
            total_volume: Value::ZERO,
            slider_1: Index(0),
            slider_2: Index(0),
            slider_switches: SliderSwitches::of(0),
            pan_mode: PanMode::Mix,
            pan_range: Value::ZERO,
            pan_assign: PanAssign::Lfo,
            pan_envelope: Envelope::ZERO,
            name: PerformanceName([0; 20]),
        }
    }

    /// A parameter's value: its [`Value`], or the number of an [`Index`] or the
    /// step of a choice, a switch or a byte of the name.
    #[must_use]
    pub fn value(&self, parameter: PerformanceParameter) -> f64 {
        match parameter.0 {
            0 => f64::from(match self.play_mode {
                PlayMode::Single => 0_u8,
                PlayMode::Dual => 1,
                PlayMode::Split => 2,
            }),
            1 => f64::from(self.voice_a.0),
            2 => f64::from(self.voice_b.0),
            3 => f64::from(self.micro_tuning_table.0),
            4 => f64::from(self.micro_tuning_key.0),
            5 => f64::from(self.micro_tuning.step()),
            6 => self.dual_detune.get(),
            7 => f64::from(self.split_point.0),
            8 => f64::from(self.eg_forced_damp),
            9 => f64::from(self.sustain.step()),
            10 => f64::from(match self.foot_switch_assign {
                FootSwitchAssign::Sustain => 0_u8,
                FootSwitchAssign::Portamento => 1,
                FootSwitchAssign::KeyHold => 2,
                FootSwitchAssign::Soft => 3,
            }),
            11 => f64::from(self.foot_switch.step()),
            12 => self.soft_pedal_range.get(),
            13 => self.note_shift_a.get(),
            14 => self.note_shift_b.get(),
            15 => self.balance.get(),
            16 => self.total_volume.get(),
            17 => f64::from(self.slider_1.0),
            18 => f64::from(self.slider_2.0),
            19 => f64::from(self.slider_switches.step()),
            20 => f64::from(match self.pan_mode {
                PanMode::Mix => 0_u8,
                PanMode::OnOn => 1,
                PanMode::OnOff => 2,
                PanMode::OffOn => 3,
            }),
            21 => self.pan_range.get(),
            22 => f64::from(match self.pan_assign {
                PanAssign::Lfo => 0_u8,
                PanAssign::Velocity => 1,
                PanAssign::Key => 2,
            }),
            23..=26 => self.pan_envelope.rates[usize::from(parameter.0 - 23)].get(),
            27..=30 => self.pan_envelope.levels[usize::from(parameter.0 - 27)].get(),
            number => f64::from(self.name.0[usize::from(number - 31)]),
        }
    }

    /// The step of a parameter, as a dump or a parameter change carries it: its
    /// value rounded by `rounding`, a value beyond the parameter's scale taken
    /// as its nearer end.
    #[must_use]
    pub fn step(&self, parameter: PerformanceParameter, rounding: Rounding) -> u8 {
        rounding.apply(self.value(parameter)).min(parameter.largest())
    }

    /// Whether a parameter's value is a step, an integer on its scale, which a
    /// dump holds exactly.
    #[must_use]
    pub fn is_step(&self, parameter: PerformanceParameter) -> bool {
        voice::is_step_of(self.value(parameter), parameter.largest())
    }

    /// Sets a parameter to a step. If the step is no more than the parameter's
    /// largest ([`PerformanceParameter::largest`]), the parameter takes it and
    /// the result is true; if it is more, the parameter takes its largest and
    /// the result is false.
    pub fn set_step(&mut self, parameter: PerformanceParameter, step: u8) -> bool {
        let largest = parameter.largest();
        let clamped = step.min(largest);
        match parameter.0 {
            0 => {
                self.play_mode = match clamped {
                    0 => PlayMode::Single,
                    1 => PlayMode::Dual,
                    _ => PlayMode::Split,
                };
            }
            1 => self.voice_a = Index::saturating(clamped),
            2 => self.voice_b = Index::saturating(clamped),
            3 => self.micro_tuning_table = Index::saturating(clamped),
            4 => self.micro_tuning_key = Index::saturating(clamped),
            5 => self.micro_tuning = Channels::of(clamped),
            6 => self.dual_detune = Value::saturating(clamped),
            7 => self.split_point = Index::saturating(clamped),
            8 => self.eg_forced_damp = clamped != 0,
            9 => self.sustain = Channels::of(clamped),
            10 => {
                self.foot_switch_assign = match clamped {
                    0 => FootSwitchAssign::Sustain,
                    1 => FootSwitchAssign::Portamento,
                    2 => FootSwitchAssign::KeyHold,
                    _ => FootSwitchAssign::Soft,
                };
            }
            11 => self.foot_switch = Channels::of(clamped),
            12 => self.soft_pedal_range = Value::saturating(clamped),
            13 => self.note_shift_a = Value::saturating(clamped),
            14 => self.note_shift_b = Value::saturating(clamped),
            15 => self.balance = Value::saturating(clamped),
            16 => self.total_volume = Value::saturating(clamped),
            17 => self.slider_1 = Index::saturating(clamped),
            18 => self.slider_2 = Index::saturating(clamped),
            19 => self.slider_switches = SliderSwitches::of(clamped),
            20 => {
                self.pan_mode = match clamped {
                    0 => PanMode::Mix,
                    1 => PanMode::OnOn,
                    2 => PanMode::OnOff,
                    _ => PanMode::OffOn,
                };
            }
            21 => self.pan_range = Value::saturating(clamped),
            22 => {
                self.pan_assign = match clamped {
                    0 => PanAssign::Lfo,
                    1 => PanAssign::Velocity,
                    _ => PanAssign::Key,
                };
            }
            23..=26 => self.pan_envelope.rates[usize::from(parameter.0 - 23)] = Value::saturating(clamped),
            27..=30 => self.pan_envelope.levels[usize::from(parameter.0 - 27)] = Value::saturating(clamped),
            number => self.name.0[usize::from(number - 31)] = clamped,
        }
        step <= largest
    }
}

/// A parameter of the DX7II's performance, by the number that the parameter
/// changes of the performance give it, 0 to 50, in the order of the PCED
/// format \[DX7II-MANUAL\] p. Add-10: the play mode; the voices' numbers; the
/// micro tuning's table, key and switches; the dual detune; the split point;
/// the forced damp; the sustain switches, the foot switch's assignment and
/// switches; the soft pedal's range; the note shifts; the balance and the total
/// volume; the continuous sliders' assignments and switches; the pan's mode,
/// range, assignment and envelope; and the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PerformanceParameter(pub(crate) u8);

impl PerformanceParameter {
    /// The number of parameters: 51.
    pub const COUNT: u8 = 51;

    /// The parameter of a number, if the number is below 51.
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

    /// The largest step it takes \[DX7II-MANUAL\] \[DX7S-MANUAL\].
    #[must_use]
    pub const fn largest(self) -> u8 {
        match self.0 {
            0 | 22 => 2,
            5 | 9 | 10 | 11 | 20 => 3,
            6 | 12 => 7,
            3 => 74,
            4 => 11,
            8 => 1,
            13 | 14 => 48,
            15 => 100,
            16 | 21 | 23..=30 => 99,
            17 => 105,
            18 => 109,
            19 => 15,
            _ => 0x7f,
        }
    }
}

impl fmt::Display for PerformanceParameter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        const NAMES: [&str; 31] = [
            "play mode",
            "A voice number",
            "B voice number",
            "micro tuning table",
            "micro tuning key",
            "micro tuning switch",
            "dual detune",
            "split point",
            "EG forced damp",
            "sustain foot switch",
            "foot switch assign",
            "foot switch",
            "soft pedal range",
            "note shift A",
            "note shift B",
            "volume balance",
            "total volume",
            "continuous slider 1",
            "continuous slider 2",
            "continuous slider switch",
            "pan mode",
            "pan range",
            "pan assign",
            "pan EG rate 1",
            "pan EG rate 2",
            "pan EG rate 3",
            "pan EG rate 4",
            "pan EG level 1",
            "pan EG level 2",
            "pan EG level 3",
            "pan EG level 4",
        ];
        match NAMES.get(usize::from(self.0)) {
            Some(name) => formatter.write_str(name),
            None => write!(formatter, "name byte {}", self.0 - 30),
        }
    }
}

/// Which keys the instrument plays of the notes it receives, by its step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyFilter {
    /// 0: every note.
    Normal,
    /// 1: the odd notes.
    Odd,
    /// 2: the even notes.
    Even,
}

/// How program changes are sent, by its step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProgramChangeMode {
    /// 0: not sent.
    Off,
    /// 1: as the memories' numbers.
    Normal,
    /// 2: through the programmable table.
    Programmable,
}

/// Which memories are protected, a bit each: bit 0 the internal, bit 1 the
/// cartridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Protect {
    /// The internal memory.
    pub internal: bool,
    /// The cartridge.
    pub cartridge: bool,
}

/// The 17 bytes of the system set-up, 20 to 36, that \[DX7II-MANUAL\] leaves
/// unnamed, seven bits each, kept as they come.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SystemUnnamed([u8; 17]);

impl SystemUnnamed {
    /// The bytes, if each is below 128.
    #[must_use]
    pub fn new(bytes: [u8; 17]) -> Option<Self> {
        bytes.iter().all(|&byte| byte < 0x80).then_some(Self(bytes))
    }

    /// The bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 17] {
        self.0
    }
}

/// The DX7II's system set-up: what its system set-up dump holds, 102 bytes
/// \[DX7II-MANUAL\] p. Add-13. The DX7s's dump holds it but for the bytes that
/// have no name, in 85 bytes \[DX7S-MANUAL\] pp. Add-4, Add-14.
#[derive(Debug, Clone, PartialEq)]
#[expect(clippy::struct_excessive_bools, reason = "the system set-up holds several switches")]
pub struct SystemSetup {
    /// The channel it sends on, 0 to 15 (0).
    pub transmit_channel: Index<15>,
    /// Whether it sends channel voice messages (1).
    pub channel_messages: bool,
    /// The channel voice A receives on, 0 to 15, 16 for none (2).
    pub receive_channel_a: Index<16>,
    /// The channel voice B receives on (3).
    pub receive_channel_b: Index<16>,
    /// The omni mode (4).
    pub omni: bool,
    /// The number of the first MIDI controller it takes: 11 to 31 on the
    /// DX7II, 9 to 31 on the DX7s (5).
    pub controller_a: Index<31>,
    /// The number of the second (6).
    pub controller_b: Index<31>,
    /// The controller number of the first continuous slider: 11 to 31 on the
    /// DX7II, 5 to 31 on the DX7s (7).
    pub slider_1_controller: Index<31>,
    /// The controller number of the second (8).
    pub slider_2_controller: Index<31>,
    /// Which keys it plays of the notes it receives (9).
    pub key_filter: KeyFilter,
    /// How it sends program changes (10).
    pub program_change_mode: ProgramChangeMode,
    /// The local switch (11).
    pub local: bool,
    /// Whether it sends in blocks of 32 (12).
    pub transmit_block: bool,
    /// Whether it receives in blocks of 32 (13).
    pub receive_block: bool,
    /// The channel of its system messages, its device number, 0 to 15 (14).
    pub system_channel: Index<15>,
    /// Whether it takes system common messages (15).
    pub system_messages: bool,
    /// The cartridge banks it takes voices and performances, fractional
    /// scalings and micro tunings from (16 to 18).
    pub cartridge_banks: [Index<15>; 3],
    /// The memory protect (19). The DX7s sets it by the parameter change alone
    /// and keeps its byte in the dump as a reserved one \[DX7S-MANUAL\], which
    /// is read and written as this all the same.
    pub protect: Protect,
    /// The bytes 20 to 36 that have no name, which the DX7s's dump does not
    /// hold.
    pub unnamed: SystemUnnamed,
    /// The master tune, 0 to 127, 64 being concert pitch (37), which a DX
    /// performance parameter change's number 64 sets.
    pub master_tune: Value<127>,
    /// The programmable table of the program changes it sends, 64 numbers
    /// (38 to 101).
    pub program_changes: [Index<127>; 64],
}

impl SystemSetup {
    /// A system set-up of every byte 0: the start of one whose every parameter
    /// a codec sets.
    pub(crate) const fn zero() -> Self {
        Self {
            transmit_channel: Index(0),
            channel_messages: false,
            receive_channel_a: Index(0),
            receive_channel_b: Index(0),
            omni: false,
            controller_a: Index(0),
            controller_b: Index(0),
            slider_1_controller: Index(0),
            slider_2_controller: Index(0),
            key_filter: KeyFilter::Normal,
            program_change_mode: ProgramChangeMode::Off,
            local: false,
            transmit_block: false,
            receive_block: false,
            system_channel: Index(0),
            system_messages: false,
            cartridge_banks: [Index(0); 3],
            protect: Protect {
                internal: false,
                cartridge: false,
            },
            unnamed: SystemUnnamed([0; 17]),
            master_tune: Value::ZERO,
            program_changes: [Index(0); 64],
        }
    }

    /// A parameter's value: its [`Value`], or the number of an [`Index`] or the
    /// step of a choice, a switch or a byte.
    #[must_use]
    pub fn value(&self, parameter: SystemParameter) -> f64 {
        match parameter.0 {
            0 => f64::from(self.transmit_channel.0),
            1 => f64::from(self.channel_messages),
            2 => f64::from(self.receive_channel_a.0),
            3 => f64::from(self.receive_channel_b.0),
            4 => f64::from(self.omni),
            5 => f64::from(self.controller_a.0),
            6 => f64::from(self.controller_b.0),
            7 => f64::from(self.slider_1_controller.0),
            8 => f64::from(self.slider_2_controller.0),
            9 => f64::from(match self.key_filter {
                KeyFilter::Normal => 0_u8,
                KeyFilter::Odd => 1,
                KeyFilter::Even => 2,
            }),
            10 => f64::from(match self.program_change_mode {
                ProgramChangeMode::Off => 0_u8,
                ProgramChangeMode::Normal => 1,
                ProgramChangeMode::Programmable => 2,
            }),
            11 => f64::from(self.local),
            12 => f64::from(self.transmit_block),
            13 => f64::from(self.receive_block),
            14 => f64::from(self.system_channel.0),
            15 => f64::from(self.system_messages),
            16..=18 => f64::from(self.cartridge_banks[usize::from(parameter.0 - 16)].0),
            19 => f64::from(u8::from(self.protect.internal) | u8::from(self.protect.cartridge) << 1),
            20..=36 => f64::from(self.unnamed.0[usize::from(parameter.0 - 20)]),
            37 => self.master_tune.get(),
            number => f64::from(self.program_changes[usize::from(number - 38)].0),
        }
    }

    /// The step of a parameter, as a dump or a parameter change carries it: its
    /// value rounded by `rounding`, a value beyond the parameter's scale taken
    /// as its nearer end.
    #[must_use]
    pub fn step(&self, parameter: SystemParameter, rounding: Rounding) -> u8 {
        rounding.apply(self.value(parameter)).min(parameter.largest())
    }

    /// Whether a parameter's value is a step, an integer on its scale, which a
    /// dump holds exactly.
    #[must_use]
    pub fn is_step(&self, parameter: SystemParameter) -> bool {
        voice::is_step_of(self.value(parameter), parameter.largest())
    }

    /// Sets a parameter to a step. If the step is within the parameter's range
    /// ([`SystemParameter::smallest`] to its largest), the parameter takes it
    /// and the result is true; if it is not, the parameter takes the range's
    /// nearer end and the result is false.
    pub fn set_step(&mut self, parameter: SystemParameter, step: u8) -> bool {
        let (smallest, largest) = (parameter.smallest(), parameter.largest());
        let clamped = step.max(smallest).min(largest);
        match parameter.0 {
            0 => self.transmit_channel = Index::saturating(clamped),
            1 => self.channel_messages = clamped != 0,
            2 => self.receive_channel_a = Index::saturating(clamped),
            3 => self.receive_channel_b = Index::saturating(clamped),
            4 => self.omni = clamped != 0,
            5 => self.controller_a = Index::saturating(clamped),
            6 => self.controller_b = Index::saturating(clamped),
            7 => self.slider_1_controller = Index::saturating(clamped),
            8 => self.slider_2_controller = Index::saturating(clamped),
            9 => {
                self.key_filter = match clamped {
                    0 => KeyFilter::Normal,
                    1 => KeyFilter::Odd,
                    _ => KeyFilter::Even,
                };
            }
            10 => {
                self.program_change_mode = match clamped {
                    0 => ProgramChangeMode::Off,
                    1 => ProgramChangeMode::Normal,
                    _ => ProgramChangeMode::Programmable,
                };
            }
            11 => self.local = clamped != 0,
            12 => self.transmit_block = clamped != 0,
            13 => self.receive_block = clamped != 0,
            14 => self.system_channel = Index::saturating(clamped),
            15 => self.system_messages = clamped != 0,
            16..=18 => self.cartridge_banks[usize::from(parameter.0 - 16)] = Index::saturating(clamped),
            19 => {
                self.protect = Protect {
                    internal: clamped & 1 != 0,
                    cartridge: clamped & 2 != 0,
                };
            }
            20..=36 => self.unnamed.0[usize::from(parameter.0 - 20)] = clamped,
            37 => self.master_tune = Value::saturating(clamped),
            number => self.program_changes[usize::from(number - 38)] = Index::saturating(clamped),
        }
        (smallest..=largest).contains(&step)
    }
}

/// A parameter of the DX7II's system set-up, by its byte in the system set-up
/// dump, 0 to 101 \[DX7II-MANUAL\] p. Add-13: the parameters 64 to 83 of its
/// parameter changes at 0 to 19, the bytes that have no name at 20 to 36, the
/// master tune at 37 and the programmable program change table at 38 to 101.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemParameter(pub(crate) u8);

impl SystemParameter {
    /// The number of parameters: 102.
    pub const COUNT: u8 = 102;

    /// The number the parameter changes give the first, at byte 0.
    pub const FIRST_CHANGE: u8 = 64;

    /// The parameter of a byte, if the byte is below 102.
    #[must_use]
    pub fn new(byte: u8) -> Option<Self> {
        (byte < Self::COUNT).then_some(Self(byte))
    }

    /// The parameter of a parameter change's number, 64 to 83.
    #[must_use]
    pub fn of_change(number: u8) -> Option<Self> {
        number
            .checked_sub(Self::FIRST_CHANGE)
            .filter(|&byte| byte < 20)
            .map(Self)
    }

    /// Its byte in the dump.
    #[must_use]
    pub const fn byte(self) -> u8 {
        self.0
    }

    /// Its number in a parameter change, for the bytes 0 to 19.
    #[must_use]
    pub fn change_number(self) -> Option<u8> {
        (self.0 < 20).then(|| Self::FIRST_CHANGE + self.0)
    }

    /// Every parameter, in the order of their bytes.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..Self::COUNT).map(Self)
    }

    /// The parameters of the DX7s's dump of its system set-up, in the order of
    /// its 85 bytes \[DX7S-MANUAL\] p. Add-14: the bytes 0 to 19, then the
    /// master tune and the table, 37 to 101. It has no place for the bytes 20
    /// to 36.
    pub fn in_dx7s_dump() -> impl Iterator<Item = Self> {
        (0..20).chain(37..Self::COUNT).map(Self)
    }

    /// The smallest step it takes: 9 for the MIDI controllers' numbers and 5
    /// for the sliders', as the DX7s takes them, the DX7II taking 11
    /// \[DX7S-MANUAL\]; 0 for the others.
    #[must_use]
    pub const fn smallest(self) -> u8 {
        match self.0 {
            5 | 6 => 9,
            7 | 8 => 5,
            _ => 0,
        }
    }

    /// The largest step it takes.
    #[must_use]
    pub const fn largest(self) -> u8 {
        match self.0 {
            0 | 14 | 16..=18 => 15,
            1 | 4 | 11..=13 | 15 => 1,
            2 | 3 => 16,
            5..=8 => 31,
            9 | 10 => 2,
            19 => 3,
            _ => 0x7f,
        }
    }
}

impl fmt::Display for SystemParameter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        const NAMES: [&str; 20] = [
            "transmit channel",
            "channel voice message switch",
            "receive channel A",
            "receive channel B",
            "omni",
            "MIDI controller A",
            "MIDI controller B",
            "continuous slider 1 controller",
            "continuous slider 2 controller",
            "key filter",
            "program change mode",
            "local",
            "transmit block",
            "receive block",
            "system channel",
            "system common switch",
            "cartridge bank of voices and performances",
            "cartridge bank of fractional scalings",
            "cartridge bank of micro tunings",
            "memory protect",
        ];
        match self.0 {
            0..=19 => formatter.write_str(NAMES[usize::from(self.0)]),
            20..=36 => write!(formatter, "system byte {}", self.0),
            37 => formatter.write_str("master tune"),
            number => write!(formatter, "program change table entry {}", number - 37),
        }
    }
}

/// A key's pitch in a micro tuning: a number on the DX7II's scale of them, 0 to
/// 10794 \[DX7II-MANUAL\] p. Add-14, or beyond it, any finite binary64
/// (docs/MATH.md §0.5), which its dumps and its parameter change send as two
/// bytes, the high seven bits, no more than 84, and the low seven.
///
/// The DX7II knows only the scale's steps, the integers 0 to 10794. A value may
/// lie between them; the pitch a value gives is for the engine to say
/// (docs/DESIGN.md §6.4); and beyond them, where a dump holds the scale's nearer
/// end. A value is never NaN and never infinite, nor ever −0.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct KeyPitch(f64);

impl KeyPitch {
    /// The largest step: 10794, 84 · 128 + 42.
    pub const LARGEST: u16 = 10794;

    /// The value, if it is a finite number.
    #[must_use]
    pub fn new(value: f64) -> Option<Self> {
        // Adding +0 takes −0 to +0 and leaves every other number as it is.
        value.is_finite().then_some(Self(value + 0.0))
    }

    /// The value of a step, if the step is no more than 10794.
    #[must_use]
    pub fn from_step(step: u16) -> Option<Self> {
        (step <= Self::LARGEST).then_some(Self(f64::from(step)))
    }

    /// The value of a step, or 10794 if the step is more.
    fn saturating(step: u16) -> Self {
        Self(f64::from(step.min(Self::LARGEST)))
    }

    /// The value.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// The step the value rounds to by `rounding`, from 0 to 10794: a value
    /// beyond the scale as its nearer end.
    #[must_use]
    pub fn step(self, rounding: Rounding) -> u16 {
        rounding.apply_wide(self.0).min(Self::LARGEST)
    }

    /// Whether the value is a step, an integer on the scale, which a dump holds
    /// exactly.
    #[must_use]
    pub fn is_step(self) -> bool {
        self.0.fract() == 0.0 && (0.0..=f64::from(Self::LARGEST)).contains(&self.0)
    }
}

/// A micro tuning of the DX7II: the pitch of each of its 128 keys
/// \[DX7II-MANUAL\] p. Add-14, as its dumps hold it, the edit buffer's, those
/// of the two micro tunings in its memory and those of a cartridge.
#[derive(Debug, Clone, PartialEq)]
pub struct MicroTuning {
    /// The keys' pitches, by the keys' MIDI numbers, C-2 at 0 to G8 at 127.
    pub keys: [KeyPitch; 128],
}

impl MicroTuning {
    /// A micro tuning of every key at 0: the start of one whose every key a
    /// codec sets.
    pub(crate) const fn zero() -> Self {
        Self {
            keys: [KeyPitch(0.0); 128],
        }
    }

    /// Sets a key to a step. If the step is no more than 10794, the key takes
    /// it and the result is true; if it is more, the key takes 10794 and the
    /// result is false.
    pub fn set_step(&mut self, key: Index<127>, step: u16) -> bool {
        self.keys[usize::from(key.0)] = KeyPitch::saturating(step);
        step <= KeyPitch::LARGEST
    }
}

/// The offset of an operator's fractional scaling, on the scale of −128 to +127
/// that \[DX7II-MANUAL\] and \[DX7S-MANUAL\] give it, or beyond it: any finite
/// binary64 (docs/MATH.md §0.5), which may lie between the integers, as the
/// levels beside it may, and beyond the scale's ends (docs/DESIGN.md §11.2). It
/// is never NaN and never −0, which [`Offset::new`] takes as 0, so that equal
/// offsets have equal bits.
///
/// The dumps and the parameter change send it as a byte, any of 0 to 255,
/// which holds it in two's complement, as the manuals do not say: dumps of
/// DX7II banks hold 0x00 in the offset of every operator in fractional mode
/// \[DX7II-DUMPS\], which two's complement reads as no offset. A byte read gives
/// an integer, which is written back as the same byte, so that a dump comes
/// back byte for byte; an offset between the integers is written as the one
/// that `syx.export_rounding` rounds it to, and one beyond the scale as its
/// nearer end. What the offset does to the sound is for the engine to say
/// (docs/DESIGN.md §6.4).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Offset(f64);

impl Offset {
    /// No offset.
    pub const ZERO: Self = Self(0.0);

    /// The offset, if it is a finite number.
    #[must_use]
    pub fn new(value: f64) -> Option<Self> {
        // Adding +0 takes −0 to +0 and leaves every other number as it is.
        value.is_finite().then_some(Self(value + 0.0))
    }

    /// The offset that a byte holds: the byte in two's complement.
    #[must_use]
    pub fn of_byte(byte: u8) -> Self {
        Self(f64::from(byte.cast_signed()))
    }

    /// The offset.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// The byte that holds the offset rounded by `rounding`, in two's
    /// complement: an offset beyond the scale as its nearer end.
    #[must_use]
    pub fn byte(self, rounding: Rounding) -> u8 {
        // 128 above the offset, taken into 0 to 255, rounds as the offset
        // does, 128 being even; the byte is it with its highest bit flipped:
        // −128 is 0x80, −1 0xFF, 0 0x00 and +127 0x7F.
        rounding.apply(self.0 + 128.0) ^ 0x80
    }

    /// Whether the offset is a step, an integer from −128 to +127, which a
    /// dump holds exactly.
    #[must_use]
    pub fn is_step(self) -> bool {
        self.0.fract() == 0.0 && (-128.0..=127.0).contains(&self.0)
    }
}

/// One operator's fractional scaling \[DX7II-MANUAL\] p. Add-15: an offset, and
/// a level from 0 to 255 for each of 40 groups of keys, the first C-2 to C-1,
/// the next 38 three keys each, from C#-1 to F#8, and the last G8 alone. An
/// operator whose scaling mode is fractional takes it in place of the DX7's
/// level scaling ([`crate::supplement::ScalingMode`]).
#[derive(Debug, Clone, PartialEq)]
pub struct OperatorScaling {
    /// The offset.
    pub offset: Offset,
    /// The groups' levels, from the lowest group.
    pub levels: [Value<255>; 40],
}

/// A fractional scaling of the DX7II: the scaling of each operator
/// \[DX7II-MANUAL\] p. Add-15, as its dumps hold it, the edit buffer's and
/// those of a cartridge.
#[derive(Debug, Clone, PartialEq)]
pub struct FractionalScaling {
    /// The operators' scalings, OP1 first, as in a voice; the dumps and the
    /// parameter change give them from OP6.
    pub operators: [OperatorScaling; 6],
}

impl FractionalScaling {
    /// A fractional scaling of every byte 0: the start of one whose every
    /// parameter a codec sets.
    pub(crate) fn zero() -> Self {
        Self {
            operators: std::array::from_fn(|_| OperatorScaling {
                offset: Offset::ZERO,
                levels: [Value::ZERO; 40],
            }),
        }
    }

    /// A parameter's value: the offset, on the scale of −128 to +127, or a
    /// group's level, on the scale of 0 to 255.
    #[must_use]
    pub fn value(&self, parameter: ScalingParameter) -> f64 {
        let scaling = &self.operators[parameter.place()];
        match parameter.group() {
            0 => scaling.offset.get(),
            group => scaling.levels[usize::from(group - 1)].get(),
        }
    }

    /// The step of a parameter, as a dump or a parameter change carries it: its
    /// value rounded by `rounding`, the offset's in two's complement, and a
    /// value beyond its scale taken as the scale's nearer end.
    #[must_use]
    pub fn step(&self, parameter: ScalingParameter, rounding: Rounding) -> u8 {
        let scaling = &self.operators[parameter.place()];
        match parameter.group() {
            0 => scaling.offset.byte(rounding),
            group => rounding.apply(scaling.levels[usize::from(group - 1)].get()),
        }
    }

    /// Whether a parameter's value is a step, an integer on its scale, which a
    /// dump holds exactly.
    #[must_use]
    pub fn is_step(&self, parameter: ScalingParameter) -> bool {
        let scaling = &self.operators[parameter.place()];
        match parameter.group() {
            0 => scaling.offset.is_step(),
            group => voice::is_step_of(scaling.levels[usize::from(group - 1)].get(), u8::MAX),
        }
    }

    /// Sets a parameter to a step, which every byte is.
    pub fn set_step(&mut self, parameter: ScalingParameter, step: u8) {
        let scaling = &mut self.operators[parameter.place()];
        match parameter.group() {
            0 => scaling.offset = Offset::of_byte(step),
            group => scaling.levels[usize::from(group - 1)] = Value::saturating(step),
        }
    }
}

/// A parameter of a fractional scaling, by its place in the dumps, 0 to 245
/// \[DX7II-MANUAL\] p. Add-15: 41 for each operator from OP6, its offset and
/// then the levels of its 40 groups of keys from the lowest. A parameter change
/// gives it by the operator's number, 0 for OP6 to 5 for OP1, and the group's,
/// 0 for the offset and 1 to 40 for the groups of keys \[DX7S-MANUAL\] p. Add-3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScalingParameter(u8);

impl ScalingParameter {
    /// The number of parameters: 246.
    pub const COUNT: u8 = 246;

    /// The parameter of a place, if the place is below 246.
    #[must_use]
    pub fn new(place: u8) -> Option<Self> {
        (place < Self::COUNT).then_some(Self(place))
    }

    /// The parameter of an operator's number, 0 for OP6 to 5 for OP1, and a
    /// group's, 0 for the offset and 1 to 40 for the groups of keys, if they
    /// name one.
    #[must_use]
    pub fn of(operator: u8, group: u8) -> Option<Self> {
        (operator < 6 && group < 41).then(|| Self(operator * 41 + group))
    }

    /// Its place in the dumps.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The number of its operator, 0 for OP6 to 5 for OP1.
    #[must_use]
    pub const fn operator(self) -> u8 {
        self.0 / 41
    }

    /// Its group: 0 for the offset, 1 to 40 for the groups of keys from the
    /// lowest.
    #[must_use]
    pub const fn group(self) -> u8 {
        self.0 % 41
    }

    /// The place of its operator in [`FractionalScaling::operators`].
    fn place(self) -> usize {
        usize::from(5 - self.operator())
    }

    /// Every parameter, in the order of the dumps.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..Self::COUNT).map(Self)
    }
}

impl fmt::Display for ScalingParameter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let operator = 6 - self.operator();
        match self.group() {
            0 => write!(formatter, "OP{operator} offset"),
            group => write!(formatter, "OP{operator} key group {group}"),
        }
    }
}
