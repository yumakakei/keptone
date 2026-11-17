// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The TX802's own data: its performances, of eight tone generators each
//! playing a voice of its own, and its system set-up, every parameter of which
//! is a range type \[TX802-SYSEX\].
//!
//! The TX802 is the DX7II's tone generator in a rack, and its voices, their
//! supplements, its micro tunings and its fractional scalings are the DX7II's
//! ([`crate::voice`], [`crate::supplement`], [`crate::dx7ii`]). What it has of
//! its own is a [`Performance`] of eight [`ToneGenerator`]s, each playing a
//! voice on a channel over a range of keys, with its detune, volume, output,
//! note shift, forced damp, key assign group, micro tuning table and channel
//! offset, and a [`SystemSetup`]: the channels and switches of its reception,
//! its device number, its banks, its master tune, the table that assigns the
//! controllers' numbers, and that of the programs that select its
//! performances. What each does is Keptone's own, for the engine and the host
//! to say (docs/DESIGN.md §6.4); these types hold them, each number a
//! [`Value`] of its scale or an [`Index`] that names a thing, each choice an
//! enumeration and each switch a `bool`.
//!
//! [`PerformanceParameter`] numbers a performance's parameters as its edit
//! buffer and its parameter changes do, 0 to 115, and [`SystemParameter`] the
//! system set-up's bytes as its dump holds them, 0 to 262.

use std::fmt;

use crate::dx7ii::Index;
use crate::voice::{Rounding, Value};

/// Where a tone generator's sound goes, by its step: the TX802's two outputs,
/// I and II.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Output {
    /// 0: to neither.
    Off,
    /// 1: to output I.
    First,
    /// 2: to output II.
    Second,
    /// 3: to both.
    Both,
}

/// One of a performance's eight tone generators: what the performance holds of
/// it \[TX802-SYSEX\] Table 3.
#[derive(Debug, Clone, PartialEq)]
pub struct ToneGenerator {
    /// Its voice channel offset, 0 to 7 (VCHOFS).
    pub channel_offset: Index<7>,
    /// The channel it receives on, 0 to 15, 16 for omni (RXCH).
    pub receive_channel: Index<16>,
    /// Its voice: 0 to 63 inside, 64 to 127 on the cartridge, 128 to 191 in
    /// the preset bank A and 192 to 255 in B (VNUM).
    pub voice: Index<255>,
    /// Its detune, 0 to 14, 7 being none (DETUNE).
    pub detune: Value<14>,
    /// Its volume, 0 to 99 (OUTVOL).
    pub volume: Value<99>,
    /// Its output (OUTCH).
    pub output: Output,
    /// The lowest key it plays, C-2 at 0 to G8 at 127 (NLMTL).
    pub note_low: Index<127>,
    /// The highest key it plays (NLMTH).
    pub note_high: Index<127>,
    /// Its note shift, 0 to 48, 24 being none (NSHFT).
    pub note_shift: Value<48>,
    /// Whether its envelopes are damped when its voices are taken for other
    /// notes (FDAMP).
    pub forced_damp: bool,
    /// Its key assign group switch (KASG).
    pub key_assign_group: bool,
    /// Its micro tuning table, 0 to 254 (MTTNUM).
    pub micro_tuning: Index<254>,
}

impl ToneGenerator {
    /// A tone generator of every step 0.
    const fn zero() -> Self {
        Self {
            channel_offset: Index::saturating(0),
            receive_channel: Index::saturating(0),
            voice: Index::saturating(0),
            detune: Value::ZERO,
            volume: Value::ZERO,
            output: Output::Off,
            note_low: Index::saturating(0),
            note_high: Index::saturating(0),
            note_shift: Value::ZERO,
            forced_damp: false,
            key_assign_group: false,
            micro_tuning: Index::saturating(0),
        }
    }
}

/// A performance's name: twenty bytes, ASCII as the dumps have it, each of
/// eight bits, as its two hexadecimal digits can hold, kept as they come.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PerformanceName([u8; 20]);

impl PerformanceName {
    /// The name of twenty bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// Its bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 20] {
        self.0
    }
}

/// The TX802's performance: what its PCED and PMEM formats hold
/// \[TX802-SYSEX\] Tables 3 and 5.
#[derive(Debug, Clone, PartialEq)]
pub struct Performance {
    /// The eight tone generators, TG1 first.
    pub tone_generators: [ToneGenerator; 8],
    /// The name (96 to 115).
    pub name: PerformanceName,
}

impl Performance {
    /// A performance of every step 0: the start of one whose every parameter a
    /// codec sets.
    pub(crate) fn zero() -> Self {
        Self {
            tone_generators: std::array::from_fn(|_| ToneGenerator::zero()),
            name: PerformanceName([0; 20]),
        }
    }

    /// A parameter's value: its [`Value`], or the number of an [`Index`] or the
    /// step of a choice, a switch or a byte of the name.
    #[must_use]
    pub fn value(&self, parameter: PerformanceParameter) -> f64 {
        let Some(generator) = parameter.tone_generator() else {
            return f64::from(self.name.0[usize::from(parameter.0 - 96)]);
        };
        let tone = &self.tone_generators[usize::from(generator)];
        match parameter.0 / 8 {
            0 => f64::from(tone.channel_offset.get()),
            1 => f64::from(tone.receive_channel.get()),
            2 => f64::from(tone.voice.get()),
            3 => tone.detune.get(),
            4 => tone.volume.get(),
            5 => f64::from(match tone.output {
                Output::Off => 0_u8,
                Output::First => 1,
                Output::Second => 2,
                Output::Both => 3,
            }),
            6 => f64::from(tone.note_low.get()),
            7 => f64::from(tone.note_high.get()),
            8 => tone.note_shift.get(),
            9 => f64::from(tone.forced_damp),
            10 => f64::from(tone.key_assign_group),
            _ => f64::from(tone.micro_tuning.get()),
        }
    }

    /// The step of a parameter, as a dump or a parameter change carries it: its
    /// value rounded by `rounding`.
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
        let largest = parameter.largest();
        let clamped = step.min(largest);
        let Some(generator) = parameter.tone_generator() else {
            self.name.0[usize::from(parameter.0 - 96)] = clamped;
            return step <= largest;
        };
        let tone = &mut self.tone_generators[usize::from(generator)];
        match parameter.0 / 8 {
            0 => tone.channel_offset = Index::saturating(clamped),
            1 => tone.receive_channel = Index::saturating(clamped),
            2 => tone.voice = Index::saturating(clamped),
            3 => tone.detune = Value::saturating(clamped),
            4 => tone.volume = Value::saturating(clamped),
            5 => {
                tone.output = match clamped {
                    0 => Output::Off,
                    1 => Output::First,
                    2 => Output::Second,
                    _ => Output::Both,
                };
            }
            6 => tone.note_low = Index::saturating(clamped),
            7 => tone.note_high = Index::saturating(clamped),
            8 => tone.note_shift = Value::saturating(clamped),
            9 => tone.forced_damp = clamped != 0,
            10 => tone.key_assign_group = clamped != 0,
            _ => tone.micro_tuning = Index::saturating(clamped),
        }
        step <= largest
    }
}

/// A parameter of the TX802's performance, by the number that its edit buffer
/// and its parameter changes give it, 0 to 115 \[TX802-SYSEX\] Table 3: for
/// each of the twelve parameters of a tone generator in turn, eight numbers,
/// TG1's first, the voice channel offset, the receive channel, the voice, the
/// detune, the volume, the output, the lowest and the highest key, the note
/// shift, the forced damp, the key assign group and the micro tuning table;
/// then the twenty bytes of the name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PerformanceParameter(pub(crate) u8);

impl PerformanceParameter {
    /// The number of parameters: 116.
    pub const COUNT: u8 = 116;

    /// The parameter of a number, if the number is below 116.
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

    /// The tone generator it belongs to, 0 for TG1 to 7, or none for a byte of
    /// the name.
    #[must_use]
    pub const fn tone_generator(self) -> Option<u8> {
        if self.0 < 96 { Some(self.0 % 8) } else { None }
    }

    /// Whether a parameter change carries it in two bytes of data, its high
    /// seven bits and its low seven: the voice and the micro tuning table,
    /// which run beyond 127 \[TX802-SYSEX\] Table 3.
    #[must_use]
    pub const fn is_wide(self) -> bool {
        matches!(self.0, 16..=23 | 88..=95)
    }

    /// The largest step it takes \[TX802-SYSEX\] Table 3.
    #[must_use]
    pub const fn largest(self) -> u8 {
        match self.0 {
            0..=7 => 7,
            8..=15 => 16,
            24..=31 => 14,
            32..=39 => 99,
            40..=47 => 3,
            48..=63 => 127,
            64..=71 => 48,
            72..=87 => 1,
            88..=95 => 254,
            _ => 0xff,
        }
    }
}

impl fmt::Display for PerformanceParameter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        const NAMES: [&str; 12] = [
            "voice channel offset",
            "receive channel",
            "voice",
            "detune",
            "volume",
            "output",
            "note limit low",
            "note limit high",
            "note shift",
            "forced damp",
            "key assign group",
            "micro tuning table",
        ];
        match self.tone_generator() {
            Some(generator) => write!(formatter, "TG{} {}", generator + 1, NAMES[usize::from(self.0 / 8)]),
            None => write!(formatter, "name byte {}", self.0 - 95),
        }
    }
}

/// What a control change's number is taken as, by an entry of the TX802's
/// table of them: one of the eight numbers \[TX802-SYSEX\] Table 6 allows, 0 for
/// none and the others the standard controllers'.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlAssign {
    /// 0: none.
    Off,
    /// 1: the modulation wheel.
    ModulationWheel,
    /// 2: the breath controller.
    Breath,
    /// 4: the foot controller.
    Foot,
    /// 5: the portamento time.
    PortamentoTime,
    /// 7: the volume.
    Volume,
    /// 64: the sustain switch.
    Sustain,
    /// 65: the portamento switch.
    Portamento,
}

impl ControlAssign {
    /// The entry of a step, if it is one of the eight.
    #[must_use]
    pub const fn of(step: u8) -> Option<Self> {
        Some(match step {
            0 => Self::Off,
            1 => Self::ModulationWheel,
            2 => Self::Breath,
            4 => Self::Foot,
            5 => Self::PortamentoTime,
            7 => Self::Volume,
            64 => Self::Sustain,
            65 => Self::Portamento,
            _ => return None,
        })
    }

    /// Its step.
    #[must_use]
    pub const fn step(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::ModulationWheel => 1,
            Self::Breath => 2,
            Self::Foot => 4,
            Self::PortamentoTime => 5,
            Self::Volume => 7,
            Self::Sustain => 64,
            Self::Portamento => 65,
        }
    }
}

/// The TX802's system set-up: what its system set-up dump holds, 263 bytes
/// \[TX802-SYSEX\] Table 6, whose numbers 1 to 263 they are. The dump leaves out
/// the internal memory's protect, Table 6's number 0, and the two micro
/// tunings in its memory, which their own dumps carry.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemSetup {
    /// The channel on which program changes select performances, 0 to 17
    /// (PRXCH, 0).
    pub performance_channel: Index<17>,
    /// The device number of its system-exclusive messages, 0 to 17 (DEVNO, 1):
    /// Table 6 gives 0 to 15, and the TX802's manual off, 1 to 16 and all
    /// \[TX802-MANUAL\] p. 32, which this takes as well.
    pub device_number: Index<17>,
    /// Which block of 32 voices a voice bank's dump goes to (VBLOK, 2).
    pub voice_block: bool,
    /// How it receives program changes, 0 to 17 (PGMSW, 3).
    pub program_changes: Index<17>,
    /// How it receives aftertouch, 0 to 17 (AFTSW, 4).
    pub aftertouch: Index<17>,
    /// How it receives pitch bends, 0 to 17 (PBSW, 5).
    pub pitch_bend: Index<17>,
    /// Which notes it receives, 0 to 2 (NOTESW, 6).
    pub notes: Index<2>,
    /// Whether the table of programs selects its performances (PRTSW, 7).
    pub program_table_switch: bool,
    /// The cartridge banks it takes its own data, fractional scalings and
    /// micro tunings from, 0 to 15 (BNK802, BNKFRAC and BNKMCT, 8 to 10).
    pub banks: [Index<15>; 3],
    /// The master tune, 0 to 127 (MTUNING, 11), which the DX performance
    /// parameter change's number 64 sets.
    pub master_tune: Value<127>,
    /// How it receives control changes, 0 to 17 (CONTSW, 12).
    pub control_changes: Index<17>,
    /// What each control change's number is taken as, from 0 (CTABLE, 13 to
    /// 134).
    pub control_table: [ControlAssign; 122],
    /// The performance that each program change's number selects, 0 to 127
    /// (PTABLE, 135 to 262).
    pub program_table: [Index<127>; 128],
}

impl SystemSetup {
    /// A system set-up of every byte 0: the start of one whose every parameter
    /// a codec sets.
    pub(crate) const fn zero() -> Self {
        Self {
            performance_channel: Index::saturating(0),
            device_number: Index::saturating(0),
            voice_block: false,
            program_changes: Index::saturating(0),
            aftertouch: Index::saturating(0),
            pitch_bend: Index::saturating(0),
            notes: Index::saturating(0),
            program_table_switch: false,
            banks: [Index::saturating(0); 3],
            master_tune: Value::ZERO,
            control_changes: Index::saturating(0),
            control_table: [ControlAssign::Off; 122],
            program_table: [Index::saturating(0); 128],
        }
    }

    /// A parameter's value: the master tune's [`Value`], or the number of an
    /// [`Index`] or the step of a switch or an entry.
    #[must_use]
    pub fn value(&self, parameter: SystemParameter) -> f64 {
        match parameter.0 {
            0 => f64::from(self.performance_channel.get()),
            1 => f64::from(self.device_number.get()),
            2 => f64::from(self.voice_block),
            3 => f64::from(self.program_changes.get()),
            4 => f64::from(self.aftertouch.get()),
            5 => f64::from(self.pitch_bend.get()),
            6 => f64::from(self.notes.get()),
            7 => f64::from(self.program_table_switch),
            8..=10 => f64::from(self.banks[usize::from(parameter.0 - 8)].get()),
            11 => self.master_tune.get(),
            12 => f64::from(self.control_changes.get()),
            13..=134 => f64::from(self.control_table[usize::from(parameter.0 - 13)].step()),
            byte => f64::from(self.program_table[usize::from(byte - 135)].get()),
        }
    }

    /// The step of a parameter, as a dump carries it: its value rounded by
    /// `rounding`.
    #[must_use]
    pub fn step(&self, parameter: SystemParameter, rounding: Rounding) -> u8 {
        rounding.apply(self.value(parameter))
    }

    /// Whether a parameter's value is a step, which a dump holds exactly.
    #[must_use]
    pub fn is_step(&self, parameter: SystemParameter) -> bool {
        self.value(parameter).fract() == 0.0
    }

    /// Sets a parameter to a step. If the step is one the parameter takes, it
    /// takes it and the result is true; if it is not, the result is false and
    /// the parameter takes its largest step, or, for an entry of the table of
    /// control changes, none.
    pub fn set_step(&mut self, parameter: SystemParameter, step: u8) -> bool {
        if let 13..=134 = parameter.0 {
            let entry = ControlAssign::of(step);
            self.control_table[usize::from(parameter.0 - 13)] = entry.unwrap_or(ControlAssign::Off);
            return entry.is_some();
        }
        let largest = parameter.largest();
        let clamped = step.min(largest);
        match parameter.0 {
            0 => self.performance_channel = Index::saturating(clamped),
            1 => self.device_number = Index::saturating(clamped),
            2 => self.voice_block = clamped != 0,
            3 => self.program_changes = Index::saturating(clamped),
            4 => self.aftertouch = Index::saturating(clamped),
            5 => self.pitch_bend = Index::saturating(clamped),
            6 => self.notes = Index::saturating(clamped),
            7 => self.program_table_switch = clamped != 0,
            8..=10 => self.banks[usize::from(parameter.0 - 8)] = Index::saturating(clamped),
            11 => self.master_tune = Value::saturating(clamped),
            12 => self.control_changes = Index::saturating(clamped),
            byte => self.program_table[usize::from(byte - 135)] = Index::saturating(clamped),
        }
        step <= largest
    }
}

/// A parameter of the TX802's system set-up, by its byte in the system set-up
/// dump, 0 to 262: Table 6's number less one \[TX802-SYSEX\].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SystemParameter(pub(crate) u16);

impl SystemParameter {
    /// The number of parameters: 263.
    pub const COUNT: u16 = 263;

    /// The parameter of a byte, if the byte is below 263.
    #[must_use]
    pub fn new(byte: u16) -> Option<Self> {
        (byte < Self::COUNT).then_some(Self(byte))
    }

    /// Its byte in the dump.
    #[must_use]
    pub const fn byte(self) -> u16 {
        self.0
    }

    /// Every parameter, in the order of their bytes.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..Self::COUNT).map(Self)
    }

    /// The largest step it takes \[TX802-SYSEX\] Table 6; an entry of the
    /// table of control changes takes the eight steps of [`ControlAssign`].
    #[must_use]
    pub const fn largest(self) -> u8 {
        match self.0 {
            0 | 1 | 3..=5 | 12 => 17,
            2 | 7 => 1,
            6 => 2,
            8..=10 => 15,
            13..=134 => 65,
            _ => 0x7f,
        }
    }
}

impl fmt::Display for SystemParameter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        const NAMES: [&str; 13] = [
            "performance select channel",
            "device number",
            "voice block",
            "program change switch",
            "aftertouch switch",
            "pitch bend switch",
            "note switch",
            "program table switch",
            "bank of the TX802's data",
            "bank of fractional scalings",
            "bank of micro tunings",
            "master tune",
            "control change switch",
        ];
        match self.0 {
            0..=12 => formatter.write_str(NAMES[usize::from(self.0)]),
            13..=134 => write!(formatter, "control table entry {}", self.0 - 13),
            byte => write!(formatter, "program table entry {}", byte - 135),
        }
    }
}
