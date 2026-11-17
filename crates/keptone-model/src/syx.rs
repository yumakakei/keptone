// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The DX7's system-exclusive messages, read and written byte for byte as
//! \[DEXED-SYSEX\], \[DX7-MANUAL\], \[DX7II-MANUAL\] and \[TX7-MANUAL\]
//! lay them out, as `.syx` files and hosts hold them: those the DX7 takes, and
//! those of its family that it does not, which Keptone takes as well, being
//! upward compatible with the DX7 rather than a copy of it (docs/DESIGN.md
//! §11.2):
//!
//! - the dumps of voices: the single-voice dump, the 155 parameters of one voice
//!   a byte each, and the 32-voice dump, a bank of 32 voices of 128 bytes each,
//!   two or three parameters packed into some of the bytes; and the DX7II's
//!   supplements of voices ([`crate::supplement`]), one in 49 bytes and 32
//!   packed into 35 bytes each;
//! - the dumps of the performances of the DX1 and the TX7
//!   ([`crate::performance`]): one performance in 94 bytes, under format
//!   number 1, and under 2, which \[DX7-MANUAL\] prints for it, and 64
//!   performances packed into 64 bytes each; and the TX7's condition
//!   acknowledge, which says what it is and how it is set;
//! - the parameter changes: of a voice parameter, of a function parameter, of a
//!   button of the DX7's panel, which its firmware takes as pressed or released
//!   \[AJXS-ROM\], of a switch of the DX7II's panel by its switch remote and of
//!   a parameter of its supplement, and the TX7's, of a performance's
//!   parameter, of one of its own settings and of the operator its panel
//!   edits;
//! - the dump requests of the DX7II and the TX7, which the DX7 itself does not
//!   answer, and Keptone does (docs/DESIGN.md §15).
//!
//! [`read`] finds every message in a file, not only the first: a file may hold
//! several, one after another, with bytes that belong to no message before,
//! between and after them, and a message may be cut short (docs/DESIGN.md
//! §11.2). It gives each message decoded by [`decode`], or the reason it is
//! not. A dump's checksum is checked: a wrong one is reported, and the dump
//! decoded all the same only if `syx.accept_bad_checksum` allows it. Every byte
//! is checked against the range of its parameter, a packed byte for bits its
//! format leaves unused, and a switch's data for 0 or 127: what is not as it
//! should be is reported, and either taken as the nearer end of the range, the
//! bits ignored and the switch pressed, or the message refused, as
//! `syx.out_of_range` says (docs/MATH.md §17). A message is decoded whole or
//! refused whole.
//!
//! [`write_voice`], [`write_bank`], [`write_supplement`], [`write_supplements`],
//! [`write_performance`], [`write_performances`], [`write_condition`],
//! [`write_change`] and [`write_request`] write the messages, each value rounded to a step as
//! `syx.export_rounding` says; the dumps say what they could not hold of what
//! they were written from. A message that is valid -- its checksum right, every
//! byte in range, no unused bit set and a switch's data 0 or 127 -- comes back
//! from them byte for byte as it was read (MATH §16), but for a performance
//! under format 2, which they write under 1.
//!
//! Every function here takes any bytes and gives a value or an error, never a
//! panic, and allocates in proportion to the bytes it is given: [`read`] goes
//! through a file a message at a time, and a dump decoded takes no more than
//! ten times the bytes of its message, 1056 for each voice, 280 for each
//! performance and 344 for each supplement on a 64-bit system.

use std::fmt;

use crate::function::{Function, FunctionParameter};
use crate::performance::{DxPerformanceParameter, KeyAssign, Performance, PerformanceParameter, Unnamed};
use crate::supplement::{Supplement, SupplementParameter};
use crate::voice::{Parameter, Rounding, Voice};

/// The byte that starts a system-exclusive message.
const START: u8 = 0xf0;

/// The byte that ends a system-exclusive message.
const END: u8 = 0xf7;

/// Yamaha's manufacturer ID.
const YAMAHA: u8 = 0x43;

/// The sub-status of a dump, in the high four bits of the byte after the
/// maker's ID; its low four are the channel.
const DUMP: u8 = 0;

/// The sub-status of a parameter change.
const CHANGE: u8 = 1;

/// The sub-status of a dump request \[DX7II-MANUAL\].
const REQUEST: u8 = 2;

/// The format number of a single-voice dump.
const VOICE_FORMAT: u8 = 0;

/// The format number of the dump of one performance \[TX7-MANUAL\].
const PERFORMANCE_FORMAT: u8 = 1;

/// The format number of the dump of 64 performances \[TX7-MANUAL\], under
/// which \[DX7-MANUAL\] prints the dump of one as well.
const PERFORMANCES_FORMAT: u8 = 2;

/// The format number of the DX7II's supplement of a voice \[DX7II-MANUAL\].
const SUPPLEMENT_FORMAT: u8 = 5;

/// The format number of the DX7II's 32 supplements, packed.
const SUPPLEMENTS_FORMAT: u8 = 6;

/// The format number of a 32-voice dump.
const BANK_FORMAT: u8 = 9;

/// The format number of the TX7's condition acknowledge \[TX7-MANUAL\].
const CONDITION_FORMAT: u8 = 0x7d;

/// The format number of the DX7II's universal bulk dump, which names what it
/// holds.
const UNIVERSAL_FORMAT: u8 = 0x7e;

/// The parameter group of a voice parameter, in the high five bits of a
/// parameter change's group byte; its low two are the parameter number's high
/// bits \[DEXED-SYSEX\].
const VOICE_GROUP: u8 = 0;

/// The parameter group of the DX performance parameter change \[TX7-MANUAL\].
const PERFORMANCE_GROUP: u8 = 1;

/// The parameter group of a function parameter, and of a button of the DX7's
/// panel \[AJXS-ROM\].
const FUNCTION_GROUP: u8 = 2;

/// The parameter group of the TX function parameter change, whose group byte's
/// low bits are 1 \[TX7-MANUAL\].
const TX_GROUP: u8 = 4;

/// The parameter group of the DX7II's own parameters: with the group byte's
/// low bits 0, of the supplement \[DX7II-MANUAL\].
const DX7II_GROUP: u8 = 6;

/// The group byte's low bits and the number of the voice group's parameter
/// 156, 1 · 128 + 28, which selects the operator a panel edits \[TX7-MANUAL\].
const OPERATOR_SELECT: [u8; 2] = [0x01, 0x1c];

/// The group byte of the DX7II's switch remote, group 6 with its low bits 3
/// \[DX7II-MANUAL\].
const SWITCH_REMOTE: u8 = 0x1b;

/// A switch's data when it is pressed; 0 when it is released.
const PRESSED: u8 = 0x7f;

/// The bytes of a single-voice dump's data, a parameter each:
/// [`Parameter::IN_DUMP`].
const VOICE_BYTES: usize = 155;

/// The bytes of a single-voice dump's data that the operators' parameters
/// take: 6 · 21.
const OPERATOR_STEPS: usize = 126;

/// The bytes of one voice of a 32-voice dump.
const PACKED_BYTES: usize = 128;

/// The bytes of a 32-voice dump's data: 32 voices of 128.
const BANK_BYTES: usize = 32 * PACKED_BYTES;

/// The byte count a single-voice dump's header gives, in two bytes of seven
/// bits, the high first: 155 = 1 · 128 + 27.
const VOICE_COUNT: [u8; 2] = [0x01, 0x1b];

/// The byte count a 32-voice dump's header gives: 4096 = 32 · 128 + 0.
const BANK_COUNT: [u8; 2] = [0x20, 0x00];

/// The bytes of the data of the dump of one performance.
const PERFORMANCE_BYTES: usize = 94;

/// The bytes of one performance of the dump of 64.
const PACKED_PERFORMANCE_BYTES: usize = 64;

/// The bytes of the data of the dump of 64 performances.
const PERFORMANCES_BYTES: usize = 64 * PACKED_PERFORMANCE_BYTES;

/// The bytes of the condition acknowledge's data.
const CONDITION_BYTES: usize = 16;

/// The byte count of the dump of one performance: 94 = 0 · 128 + 94.
const PERFORMANCE_COUNT: [u8; 2] = [0x00, 0x5e];

/// The byte count of the dump of 64 performances: 4096 = 32 · 128 + 0.
const PERFORMANCES_COUNT: [u8; 2] = [0x20, 0x00];

/// The byte count of the condition acknowledge: 16 = 0 · 128 + 16.
const CONDITION_COUNT: [u8; 2] = [0x00, 0x10];

/// The bytes of the data of the dump of one supplement, a parameter each.
const SUPPLEMENT_BYTES: usize = 49;

/// The bytes of one supplement of the dump of 32.
const PACKED_SUPPLEMENT_BYTES: usize = 35;

/// The bytes of the data of the dump of 32 supplements.
const SUPPLEMENTS_BYTES: usize = 32 * PACKED_SUPPLEMENT_BYTES;

/// The byte count of the dump of one supplement: 49 = 0 · 128 + 49.
const SUPPLEMENT_COUNT: [u8; 2] = [0x00, 0x31];

/// The byte count of the dump of 32 supplements: 1120 = 8 · 128 + 96.
const SUPPLEMENTS_COUNT: [u8; 2] = [0x08, 0x60];

/// The bits that each byte of a packed voice uses \[DEXED-SYSEX\]; a valid dump
/// leaves the others clear. Of each operator's 17 bytes, byte 11 holds the two
/// curves in four bits, byte 13 the two sensitivities in five and byte 15 the
/// coarse frequency and the mode in six; byte 110 holds the algorithm in five
/// bits, and byte 111 the oscillators' key sync and the feedback in four. Every
/// other byte uses all seven.
const USED: [u8; PACKED_BYTES] = used();

/// The bits each byte of a packed voice uses: [`USED`].
const fn used() -> [u8; PACKED_BYTES] {
    let mut used = [0x7f; PACKED_BYTES];
    let mut operator = 0;
    while operator < 6 {
        used[17 * operator + 11] = 0x0f;
        used[17 * operator + 13] = 0x1f;
        used[17 * operator + 15] = 0x3f;
        operator += 1;
    }
    used[110] = 0x1f;
    used[111] = 0x0f;
    used
}

/// The bits that each byte of a packed performance uses \[TX7-MANUAL\]; a
/// valid dump leaves the others clear. Of each side's sixteen bytes, side A's
/// first and side B's from byte 16, byte 0 holds mono/poly in bit 6; byte 1
/// the pitch bend step's low three bits in bits 6–4 and the range in bits 3–0;
/// byte 2 the portamento time; byte 3 the portamento mode and glissando in
/// bits 1 and 0; bytes 4 to 7 the assignment in bits 6–4 and the sensitivity
/// in bits 3–0 of the modulation wheel, the foot controller, the aftertouch
/// and the breath controller; byte 14 the attenuation in bits 2–0; byte 15 the
/// pitch bend step's high bit in bit 6; and bytes 8 to 13 nothing. Byte 32
/// holds the voice memory select in bit 2 and the key assign mode in bits 1–0,
/// 33 nothing, and 34 to 63 the name.
const PERFORMANCE_USED: [u8; PACKED_PERFORMANCE_BYTES] = performance_used();

/// The bits each byte of a packed performance uses: [`PERFORMANCE_USED`].
const fn performance_used() -> [u8; PACKED_PERFORMANCE_BYTES] {
    const SIDE: [u8; 16] = [
        0x40, 0x7f, 0x7f, 0x03, 0x7f, 0x7f, 0x7f, 0x7f, 0, 0, 0, 0, 0, 0, 0x07, 0x40,
    ];
    let mut used = [0x7f; PACKED_PERFORMANCE_BYTES];
    let mut byte = 0;
    while byte < 16 {
        used[byte] = SIDE[byte];
        used[16 + byte] = SIDE[byte];
        byte += 1;
    }
    used[32] = 0x07;
    used[33] = 0;
    used
}

/// The bits that each byte of a packed supplement uses \[DX7II-MANUAL\] p.
/// Add-12; a valid dump leaves the others clear. Byte 0 holds the operators'
/// scaling modes, OP1's in bit 5 to OP6's in bit 0; bytes 1 to 3 their
/// amplitude modulation sensitivities, three bits each, OP5's and OP6's, OP3's
/// and OP4's, OP1's and OP2's, the first in bits 5–3; byte 4 the random pitch
/// fluctuation in bits 6–4, velocity's switch of the pitch envelope in bit 3,
/// the LFO's key trigger in bit 2 and the pitch envelope's range in bits 1–0;
/// byte 5 the pitch bend's range in bits 5–2 and mono/poly with unison in bits
/// 1–0; byte 6 the pitch bend's mode in bits 5–4 and its step in bits 3–0;
/// byte 7 the portamento's step in bits 4–1 and its mode in bit 0; bytes 8 to
/// 23 the portamento's time and the controllers' ranges; byte 24 the pitch
/// envelope's rate scaling in bits 2–0; byte 25 nothing; bytes 26 to 33 the
/// ranges of the second foot controller and the MIDI controller; and byte 34
/// the first foot controller's switch to the slider in bit 3 and the unison's
/// detune in bits 2–0. The manual draws a line in byte 5 between bits 4 and 3,
/// but the pitch bend's range, 0 to 12, takes four bits, and the byte has six
/// for it and mono/poly with unison, as \[DX7S-MANUAL\] p. Add-13 draws them.
const SUPPLEMENT_USED: [u8; PACKED_SUPPLEMENT_BYTES] = supplement_used();

/// The bits each byte of a packed supplement uses: [`SUPPLEMENT_USED`].
const fn supplement_used() -> [u8; PACKED_SUPPLEMENT_BYTES] {
    let mut used = [0x7f; PACKED_SUPPLEMENT_BYTES];
    used[0] = 0x3f;
    used[1] = 0x3f;
    used[2] = 0x3f;
    used[3] = 0x3f;
    used[5] = 0x3f;
    used[6] = 0x3f;
    used[7] = 0x1f;
    used[24] = 0x07;
    used[25] = 0;
    used[34] = 0x0f;
    used
}

/// A MIDI channel, 0 to 15, which a message's header holds in its low four
/// bits. The DX7 shows it as 1 to 16.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Channel(u8);

impl Channel {
    /// The channel of a number, if the number is below 16.
    #[must_use]
    pub fn new(channel: u8) -> Option<Self> {
        (channel < 16).then_some(Self(channel))
    }

    /// Its number, 0 to 15.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// How messages are read: two of the settings of docs/MATH.md §17.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Options {
    /// `syx.accept_bad_checksum`: whether a dump whose checksum is wrong is
    /// decoded all the same, the default, or refused. A wrong checksum is
    /// reported either way.
    pub accept_bad_checksum: bool,
    /// `syx.out_of_range`: what becomes of a byte beyond its parameter's range,
    /// a packed byte with unused bits set, or a switch's data neither 0 nor
    /// 127.
    pub out_of_range: OutOfRange,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            accept_bad_checksum: true,
            out_of_range: OutOfRange::Clamp,
        }
    }
}

/// What becomes of a byte out of range: the setting `syx.out_of_range`. A byte
/// out of range is reported either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum OutOfRange {
    /// A step beyond its parameter's range is taken as the range's end, the
    /// largest step, bits that a packed byte leaves unused are ignored, and a
    /// switch's data other than 0 is taken as pressed, as the DX7's firmware
    /// takes it \[AJXS-ROM\]: the default.
    #[default]
    Clamp,
    /// The message is refused.
    Reject,
}

/// Something a message held that was not as its format has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Finding {
    /// Its checksum is not the one its data have.
    Checksum {
        /// The checksum its data have.
        expected: u8,
        /// The checksum it ends with.
        found: u8,
    },
    /// A step beyond its parameter's range ([`Parameter::largest`]).
    OutOfRange {
        /// The place of the voice: 0 in a single-voice dump or a parameter
        /// change, 0 to 31 in a bank.
        voice: u8,
        /// The parameter.
        parameter: Parameter,
        /// The step it held.
        step: u8,
    },
    /// A byte of a packed voice with bits set that the format leaves unused.
    UnusedBits {
        /// The place of the voice in the bank, 0 to 31.
        voice: u8,
        /// The place of the byte among the voice's 128.
        byte: u8,
        /// The bits it set that are unused.
        bits: u8,
    },
    /// A step of a function parameter beyond its range
    /// ([`FunctionParameter::largest`]).
    FunctionOutOfRange {
        /// The parameter.
        parameter: FunctionParameter,
        /// The step it held.
        step: u8,
    },
    /// A switch's data neither 0, released, nor 127, pressed.
    SwitchData {
        /// The data it held.
        data: u8,
    },
    /// A step of a performance's parameter beyond its range
    /// ([`PerformanceParameter::largest`]).
    PerformanceOutOfRange {
        /// The place of the performance: 0 in the dump of one, 0 to 63 in the
        /// dump of 64.
        performance: u8,
        /// The parameter.
        parameter: PerformanceParameter,
        /// The step it held.
        step: u8,
    },
    /// A key assign mode beyond its range, 0 to 2, in the dump of 64
    /// performances.
    KeyAssignOutOfRange {
        /// The place of the performance, 0 to 63.
        performance: u8,
        /// The step it held.
        step: u8,
    },
    /// A byte of a packed performance with bits set that the format leaves
    /// unused.
    PerformanceUnusedBits {
        /// The place of the performance, 0 to 63.
        performance: u8,
        /// The place of the byte among the performance's 64.
        byte: u8,
        /// The bits it set that are unused.
        bits: u8,
    },
    /// The dump of one performance under format number 2, as \[DX7-MANUAL\]
    /// prints it, where the TX7 and the DX7's firmware have 1 and 2 is the dump
    /// of 64 \[TX7-MANUAL\] \[AJXS-ROM\]: read all the same, whatever the
    /// options say, and written under 1.
    PerformanceFormat,
    /// A step of a parameter of the DX performance parameter change outside
    /// its range ([`DxPerformanceParameter::smallest`] to its largest).
    DxPerformanceOutOfRange {
        /// The parameter.
        parameter: DxPerformanceParameter,
        /// The step it held.
        step: u8,
    },
    /// A step of a TX function parameter beyond its range
    /// ([`TxFunctionParameter::largest`]).
    TxFunctionOutOfRange {
        /// The parameter.
        parameter: TxFunctionParameter,
        /// The step it held.
        step: u8,
    },
    /// An operator select beyond its range, 0 to 5.
    OperatorSelectOutOfRange {
        /// The step it held.
        step: u8,
    },
    /// A receive channel of the condition acknowledge beyond its range, 0 to
    /// 15.
    ConditionChannel {
        /// The channel it held.
        channel: u8,
    },
    /// A step of a supplement's parameter beyond its range
    /// ([`SupplementParameter::largest`]).
    SupplementOutOfRange {
        /// The place of the supplement: 0 in the dump of one or a parameter
        /// change, 0 to 31 in the dump of 32.
        supplement: u8,
        /// The parameter.
        parameter: SupplementParameter,
        /// The step it held.
        step: u8,
    },
    /// A byte of a packed supplement with bits set that the format leaves
    /// unused.
    SupplementUnusedBits {
        /// The place of the supplement, 0 to 31.
        supplement: u8,
        /// The place of the byte among the supplement's 35.
        byte: u8,
        /// The bits it set that are unused.
        bits: u8,
    },
}

impl fmt::Display for Finding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Checksum { expected, found } => {
                write!(
                    formatter,
                    "the checksum is {found:#04x}, where the data have {expected:#04x}"
                )
            }
            Self::OutOfRange { voice, parameter, step } => write!(
                formatter,
                "voice {}: {parameter} is {step}, beyond its largest, {}",
                u16::from(voice) + 1,
                parameter.largest()
            ),
            Self::UnusedBits { voice, byte, bits } => write!(
                formatter,
                "voice {}: byte {byte} sets unused bits {bits:#04x}",
                u16::from(voice) + 1
            ),
            Self::FunctionOutOfRange { parameter, step } => write!(
                formatter,
                "{parameter} is {step}, beyond its largest, {}",
                parameter.largest()
            ),
            Self::SwitchData { data } => write!(formatter, "a switch's data is {data:#04x}, neither 0 nor 0x7f"),
            Self::PerformanceOutOfRange {
                performance,
                parameter,
                step,
            } => write!(
                formatter,
                "performance {}: {parameter} is {step}, beyond its largest, {}",
                u16::from(performance) + 1,
                parameter.largest()
            ),
            Self::KeyAssignOutOfRange { performance, step } => write!(
                formatter,
                "performance {}: the key assign mode is {step}, beyond its largest, 2",
                u16::from(performance) + 1
            ),
            Self::PerformanceUnusedBits {
                performance,
                byte,
                bits,
            } => write!(
                formatter,
                "performance {}: byte {byte} sets unused bits {bits:#04x}",
                u16::from(performance) + 1
            ),
            Self::PerformanceFormat => formatter.write_str(
                "a performance of 94 bytes under format number 2, as the DX7's manual prints it, where the TX7 has 1",
            ),
            Self::DxPerformanceOutOfRange { parameter, step } => write!(
                formatter,
                "{parameter} is {step}, outside its range, {} to {}",
                parameter.smallest(),
                parameter.largest()
            ),
            Self::TxFunctionOutOfRange { parameter, step } => write!(
                formatter,
                "{parameter} is {step}, beyond its largest, {}",
                parameter.largest()
            ),
            Self::OperatorSelectOutOfRange { step } => {
                write!(formatter, "the operator select is {step}, beyond its largest, 5")
            }
            Self::ConditionChannel { channel } => {
                write!(formatter, "the receive channel is {channel}, beyond its largest, 15")
            }
            Self::SupplementOutOfRange {
                supplement,
                parameter,
                step,
            } => write!(
                formatter,
                "supplement {}: {parameter} is {step}, beyond its largest, {}",
                u16::from(supplement) + 1,
                parameter.largest()
            ),
            Self::SupplementUnusedBits { supplement, byte, bits } => write!(
                formatter,
                "supplement {}: byte {byte} sets unused bits {bits:#04x}",
                u16::from(supplement) + 1
            ),
        }
    }
}

/// Why a message is not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Error {
    /// The bytes are not one system-exclusive message: F0, bytes below 0x80,
    /// and F7.
    NotSysEx,
    /// A message that does not end: the bytes run out, or another status byte
    /// comes, before its F7.
    Unterminated,
    /// A message that this reader does not take: another maker's, or one of
    /// Yamaha's of another sub-status, of another format of dump or request,
    /// of another parameter group, or of a parameter or switch beyond those
    /// that the DX7, the DX7II's switch remote and the TX7 number.
    Unsupported,
    /// A message whose byte count or length is not its kind's: a single-voice
    /// dump holds 155 bytes of data in a message of 163, a 32-voice dump 4096
    /// in 4104, the dump of one supplement 49 in 57, the dump of 32 1120 in
    /// 1128, the dump of one performance 94 in 102, the dump of 64 4096 in
    /// 4104, the condition acknowledge 16 in 24, a parameter change one byte
    /// of data in 7, and a dump request is of 5 bytes, or 15 for a universal
    /// bulk dump's.
    Length,
    /// A message refused for what it held, as the options have it: a wrong
    /// checksum if `syx.accept_bad_checksum` is false, or, if `syx.out_of_range`
    /// is `reject`, a step out of range, unused bits set or a switch's data
    /// neither 0 nor 127; the first found.
    Refused(Finding),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSysEx => formatter.write_str("not a system-exclusive message"),
            Self::Unterminated => formatter.write_str("a system-exclusive message that does not end"),
            Self::Unsupported => formatter.write_str("a message of a kind this reader does not take"),
            Self::Length => formatter.write_str("a message of the wrong length for its kind"),
            Self::Refused(finding) => write!(formatter, "refused: {finding}"),
        }
    }
}

impl std::error::Error for Error {}

/// A dump, decoded.
#[derive(Debug, Clone, PartialEq)]
pub struct Dump {
    /// The channel its header names.
    pub channel: Channel,
    /// Its voice or its bank.
    pub content: Content,
    /// What it held that was not as its format has it, each taken as the
    /// options say: its format number's first, then its checksum's, then those
    /// of its data in the order of their bytes.
    pub findings: Vec<Finding>,
}

/// What a dump holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    /// A single-voice dump's voice.
    Voice(Box<Voice>),
    /// A 32-voice dump's voices, in the order of their places.
    Bank(Box<[Voice; 32]>),
    /// The DX7II's dump of one supplement's supplement.
    Supplement(Box<Supplement>),
    /// The DX7II's dump of 32 supplements' supplements, in the order of their
    /// places.
    Supplements(Box<[Supplement; 32]>),
    /// The dump of one performance's performance.
    Performance(Box<Performance>),
    /// The dump of 64 performances' performances, in the order of their
    /// places.
    Performances(Box<[Performance; 64]>),
    /// The TX7's condition acknowledge.
    Condition(Condition),
}

/// A button of the DX7's panel, by the number that a panel switch message
/// gives it, 0 to 41 \[AJXS-ROM\]: 0 to 31 for the numbered buttons 1 to 32,
/// then STORE, MEMORY PROTECT INTERNAL and CARTRIDGE, OPERATOR SELECT,
/// EDIT/COMPARE, MEMORY SELECT INTERNAL and CARTRIDGE, FUNCTION, NO and YES.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Button(u8);

impl Button {
    /// The number of buttons: 42.
    pub const COUNT: u8 = 42;

    /// STORE.
    pub const STORE: Self = Self(32);

    /// MEMORY PROTECT INTERNAL.
    pub const PROTECT_INTERNAL: Self = Self(33);

    /// MEMORY PROTECT CARTRIDGE.
    pub const PROTECT_CARTRIDGE: Self = Self(34);

    /// OPERATOR SELECT.
    pub const OPERATOR_SELECT: Self = Self(35);

    /// EDIT/COMPARE.
    pub const EDIT: Self = Self(36);

    /// MEMORY SELECT INTERNAL.
    pub const INTERNAL: Self = Self(37);

    /// MEMORY SELECT CARTRIDGE.
    pub const CARTRIDGE: Self = Self(38);

    /// FUNCTION.
    pub const FUNCTION: Self = Self(39);

    /// NO, which steps down.
    pub const NO: Self = Self(40);

    /// YES, which steps up.
    pub const YES: Self = Self(41);

    /// The button of a number of a panel switch message, if it is below 42.
    #[must_use]
    pub fn new(number: u8) -> Option<Self> {
        (number < Self::COUNT).then_some(Self(number))
    }

    /// The numbered button of a number on the panel, if it is 1 to 32.
    #[must_use]
    pub fn numbered(button: u8) -> Option<Self> {
        (1..=32).contains(&button).then(|| Self(button - 1))
    }

    /// Its number in a panel switch message.
    #[must_use]
    pub const fn number(self) -> u8 {
        self.0
    }
}

impl fmt::Display for Button {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        const NAMED: [&str; 10] = [
            "STORE",
            "MEMORY PROTECT INTERNAL",
            "MEMORY PROTECT CARTRIDGE",
            "OPERATOR SELECT",
            "EDIT/COMPARE",
            "MEMORY SELECT INTERNAL",
            "MEMORY SELECT CARTRIDGE",
            "FUNCTION",
            "NO",
            "YES",
        ];
        match self.0.checked_sub(32) {
            Some(named) => formatter.write_str(NAMED[usize::from(named)]),
            None => write!(formatter, "{}", self.0 + 1),
        }
    }
}

/// A switch of the DX7II's panel, by the number that its switch remote gives
/// it, 0 to 45 \[DX7II-MANUAL\].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RemoteSwitch(u8);

impl RemoteSwitch {
    /// The number of switches: 46.
    pub const COUNT: u8 = 46;

    /// The switch of a number, if it is below 46.
    #[must_use]
    pub fn new(number: u8) -> Option<Self> {
        (number < Self::COUNT).then_some(Self(number))
    }

    /// Its number.
    #[must_use]
    pub const fn number(self) -> u8 {
        self.0
    }
}

/// A parameter of the TX function parameter change, one of the TX7's own
/// settings, by its number \[TX7-MANUAL\]: 0 to 4, switches of 0 and 1, whether
/// it receives the data entry and control changes, whether the data entry sets
/// the volume, whether it communicates with a computer, and whether its program
/// changes are combined (0) or individual (1); 5 and 6, the lowest and the
/// highest note it plays, 0 to 127; and 7 and 11, switches of 0 and 127, its
/// memory protect and whether it loads function data from its own memory or
/// from a cartridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TxFunctionParameter(u8);

impl TxFunctionParameter {
    /// The parameter of a number, if the number names one: 0 to 7 or 11.
    #[must_use]
    pub fn new(number: u8) -> Option<Self> {
        matches!(number, 0..=7 | 11).then_some(Self(number))
    }

    /// Its number.
    #[must_use]
    pub const fn number(self) -> u8 {
        self.0
    }

    /// Every parameter, in the order of their numbers.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..=11).filter_map(Self::new)
    }

    /// The largest step it takes: 1 for 0 to 4, and 127 for the notes and the
    /// switches of 0 and 127.
    #[must_use]
    pub const fn largest(self) -> u8 {
        if self.0 <= 4 { 1 } else { 0x7f }
    }

    /// Whether it is a switch of 0 and 127, 7 or 11, which takes no step
    /// between them.
    #[must_use]
    pub const fn is_switch(self) -> bool {
        matches!(self.0, 7 | 11)
    }
}

impl fmt::Display for TxFunctionParameter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.0 {
            0 => "data entry receive switch",
            1 => "control change receive switch",
            2 => "data entry volume switch",
            3 => "computer communication switch",
            4 => "combined or individual",
            5 => "note limit low",
            6 => "note limit high",
            7 => "memory protect",
            _ => "load function select",
        })
    }
}

/// A TX function parameter and a step within its range: what the TX function
/// parameter change sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TxStep {
    /// The parameter.
    parameter: TxFunctionParameter,
    /// The step: no more than the parameter's largest, and 0 or 127 for a
    /// switch of 0 and 127.
    step: u8,
}

impl TxStep {
    /// The parameter and step, if the step is within the parameter's range.
    #[must_use]
    pub fn new(parameter: TxFunctionParameter, step: u8) -> Option<Self> {
        let within = if parameter.is_switch() {
            step == 0 || step == PRESSED
        } else {
            step <= parameter.largest()
        };
        within.then_some(Self { parameter, step })
    }

    /// The parameter.
    #[must_use]
    pub const fn parameter(self) -> TxFunctionParameter {
        self.parameter
    }

    /// The step.
    #[must_use]
    pub const fn step(self) -> u8 {
        self.step
    }
}

/// A parameter of the DX performance parameter change and a step within its
/// range: what that change sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PerformanceStep {
    /// The parameter.
    parameter: DxPerformanceParameter,
    /// The step, from the parameter's smallest to its largest.
    step: u8,
}

impl PerformanceStep {
    /// The parameter and step, if the step is within the parameter's range.
    #[must_use]
    pub fn new(parameter: DxPerformanceParameter, step: u8) -> Option<Self> {
        (parameter.smallest()..=parameter.largest())
            .contains(&step)
            .then_some(Self { parameter, step })
    }

    /// The parameter.
    #[must_use]
    pub const fn parameter(self) -> DxPerformanceParameter {
        self.parameter
    }

    /// The step.
    #[must_use]
    pub const fn step(self) -> u8 {
        self.step
    }
}

/// The operator a panel edits, by the step of voice parameter 156, 0 to 5. The
/// TX7 numbers the operators from OP6, 0, to OP1, 5 \[TX7-MANUAL\], and the
/// DX7II the other way, from OP1 to OP6 \[DX7II-MANUAL\]: the step is kept as
/// it comes, for the instrument emulated to take as its own (docs/DESIGN.md
/// §11.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperatorSelect(u8);

impl OperatorSelect {
    /// The selection of a step, if the step is below 6.
    #[must_use]
    pub fn new(step: u8) -> Option<Self> {
        (step < 6).then_some(Self(step))
    }

    /// Its step.
    #[must_use]
    pub const fn step(self) -> u8 {
        self.0
    }
}

/// A data byte: seven bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Byte(u8);

impl Byte {
    /// The byte, if it is below 128.
    #[must_use]
    pub fn new(byte: u8) -> Option<Self> {
        (byte < 0x80).then_some(Self(byte))
    }

    /// Its value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// How a TX7 is set, as its condition acknowledge says, a bit each from bit 0
/// \[TX7-MANUAL\].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the message holds seven switches, a bit each"
)]
pub struct ConditionFlags {
    /// Bit 0: the performance echo back mode, of the combined mode.
    pub echo_back: bool,
    /// Bit 1: the computer communication mode.
    pub computer: bool,
    /// Bit 2: whether the data entry lever sets the volume.
    pub data_entry_volume: bool,
    /// Bit 3: whether it receives control changes.
    pub control_change: bool,
    /// Bit 4: the omni mode.
    pub omni: bool,
    /// Bit 5: the memory protect.
    pub memory_protect: bool,
    /// Bit 6: whether it receives the data entry.
    pub data_entry: bool,
}

impl ConditionFlags {
    /// The flags of a byte's seven bits.
    #[must_use]
    pub const fn of(byte: u8) -> Self {
        Self {
            echo_back: byte & 0x01 != 0,
            computer: byte & 0x02 != 0,
            data_entry_volume: byte & 0x04 != 0,
            control_change: byte & 0x08 != 0,
            omni: byte & 0x10 != 0,
            memory_protect: byte & 0x20 != 0,
            data_entry: byte & 0x40 != 0,
        }
    }

    /// Their byte.
    #[must_use]
    pub fn byte(self) -> u8 {
        [
            self.echo_back,
            self.computer,
            self.data_entry_volume,
            self.control_change,
            self.omni,
            self.memory_protect,
            self.data_entry,
        ]
        .iter()
        .rev()
        .fold(0, |byte, &bit| byte << 1 | u8::from(bit))
    }
}

/// The TX7's condition acknowledge, decoded: what it is and how it is set, the
/// answer to a dump request of format 125 \[TX7-MANUAL\].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Condition {
    /// The classification name, `LM  ` on the TX7.
    pub classification: UniversalName<4>,
    /// The model name, `8950  ` on the TX7.
    pub model: UniversalName<6>,
    /// The software's version.
    pub version: Byte,
    /// The software's revision.
    pub revision: Byte,
    /// How it is set.
    pub flags: ConditionFlags,
    /// The channel it receives on.
    pub receive_channel: Channel,
    /// Its battery's voltage, in tenths of a volt.
    pub battery: Byte,
    /// The fourth condition data byte, 0 on the TX7.
    pub data: Byte,
}

/// A voice parameter and a step within its range: what a parameter change of
/// the voice group sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VoiceStep {
    /// The parameter.
    parameter: Parameter,
    /// The step, no more than the parameter's largest.
    step: u8,
}

impl VoiceStep {
    /// The parameter and step, if the step is within the parameter's range.
    #[must_use]
    pub fn new(parameter: Parameter, step: u8) -> Option<Self> {
        (step <= parameter.largest()).then_some(Self { parameter, step })
    }

    /// The parameter of a voice and its step, its value rounded by `rounding`.
    #[must_use]
    pub fn of(voice: &Voice, parameter: Parameter, rounding: Rounding) -> Self {
        Self {
            parameter,
            step: voice.step(parameter, rounding).min(parameter.largest()),
        }
    }

    /// The parameter.
    #[must_use]
    pub const fn parameter(self) -> Parameter {
        self.parameter
    }

    /// The step.
    #[must_use]
    pub const fn step(self) -> u8 {
        self.step
    }
}

/// A parameter of the DX7II's supplement and a step within its range: what a
/// parameter change of the supplement sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SupplementStep {
    /// The parameter.
    parameter: SupplementParameter,
    /// The step, no more than the parameter's largest.
    step: u8,
}

impl SupplementStep {
    /// The parameter and step, if the step is within the parameter's range.
    #[must_use]
    pub fn new(parameter: SupplementParameter, step: u8) -> Option<Self> {
        (step <= parameter.largest()).then_some(Self { parameter, step })
    }

    /// The parameter of a supplement and its step, its value rounded by
    /// `rounding`.
    #[must_use]
    pub fn of(supplement: &Supplement, parameter: SupplementParameter, rounding: Rounding) -> Self {
        Self {
            parameter,
            step: supplement.step(parameter, rounding).min(parameter.largest()),
        }
    }

    /// The parameter.
    #[must_use]
    pub const fn parameter(self) -> SupplementParameter {
        self.parameter
    }

    /// The step.
    #[must_use]
    pub const fn step(self) -> u8 {
        self.step
    }
}

/// A function parameter and a step within its range: what a parameter change
/// of the function group sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FunctionStep {
    /// The parameter.
    parameter: FunctionParameter,
    /// The step, no more than the parameter's largest.
    step: u8,
}

impl FunctionStep {
    /// The parameter and step, if the step is within the parameter's range.
    #[must_use]
    pub fn new(parameter: FunctionParameter, step: u8) -> Option<Self> {
        (step <= parameter.largest()).then_some(Self { parameter, step })
    }

    /// The parameter of function data and its step, its value rounded by
    /// `rounding`.
    #[must_use]
    pub fn of(function: &Function, parameter: FunctionParameter, rounding: Rounding) -> Self {
        Self {
            parameter,
            step: function.step(parameter, rounding).min(parameter.largest()),
        }
    }

    /// The parameter.
    #[must_use]
    pub const fn parameter(self) -> FunctionParameter {
        self.parameter
    }

    /// The step.
    #[must_use]
    pub const fn step(self) -> u8 {
        self.step
    }
}

/// What a parameter change changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Change {
    /// A voice parameter, of group 0, its number's high bits in the group byte
    /// \[DEXED-SYSEX\].
    Voice(VoiceStep),
    /// A function parameter, 64 to 77, of group 2.
    Function(FunctionStep),
    /// A button of the DX7's panel, pressed (true) or released, of group 2 and
    /// numbers 0 to 41 \[AJXS-ROM\].
    Panel(Button, bool),
    /// A switch of the DX7II's panel, pressed (true) or released, by its switch
    /// remote \[DX7II-MANUAL\].
    Remote(RemoteSwitch, bool),
    /// A parameter of a performance, by the DX performance parameter change, of
    /// group 1 \[TX7-MANUAL\].
    Performance(PerformanceStep),
    /// One of the TX7's own settings, by the TX function parameter change, of
    /// group 4 with the group byte's low bits 1 \[TX7-MANUAL\].
    Tx(TxStep),
    /// The operator a panel edits, by voice parameter 156 \[TX7-MANUAL\].
    OperatorSelect(OperatorSelect),
    /// A parameter of the DX7II's supplement, of group 6 with the group byte's
    /// low bits 0 \[DX7II-MANUAL\].
    Supplement(SupplementStep),
}

/// A parameter change, decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParameterChange {
    /// The channel its header names.
    pub channel: Channel,
    /// What it changes.
    pub change: Change,
    /// What it held that was not as its format has it, taken as the options
    /// say: a step outside its parameter's range, or a switch's data neither 0
    /// nor 127.
    pub finding: Option<Finding>,
}

/// A name of the DX7II's universal bulk dump or of the TX7's condition
/// acknowledge: `N` bytes of ASCII, each below 128 \[DX7II-MANUAL\]
/// \[TX7-MANUAL\].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UniversalName<const N: usize>([u8; N]);

impl<const N: usize> UniversalName<N> {
    /// The name of bytes, if each is below 128.
    #[must_use]
    pub fn new(bytes: [u8; N]) -> Option<Self> {
        bytes.iter().all(|&byte| byte < 0x80).then_some(Self(bytes))
    }

    /// Its bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; N] {
        self.0
    }
}

/// What a dump request asks for \[DX7II-MANUAL\] \[TX7-MANUAL\].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Requested {
    /// The voice being played and edited, as a single-voice dump: format 0.
    Voice,
    /// The TX7's function data of that voice, as the dump of one performance:
    /// format 1.
    Performance,
    /// The TX7's function data of its voices, as the dump of 64 performances:
    /// format 2.
    Performances,
    /// The DX7II's supplement of that voice: format 5.
    Supplement,
    /// The DX7II's 32 supplements, packed: format 6.
    Supplements,
    /// 32 voices, as a 32-voice dump: format 9.
    Bank,
    /// The TX7's condition acknowledge: format 125.
    Condition,
    /// A universal bulk dump of what a classification name and a data format
    /// name name: format 0x7E.
    Universal {
        /// The classification name, four letters.
        classification: UniversalName<4>,
        /// The data format name, six letters.
        format: UniversalName<6>,
    },
}

/// A dump request, decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Request {
    /// The channel its header names: the device number of the DX7II's
    /// manual.
    pub channel: Channel,
    /// What it asks for.
    pub requested: Requested,
}

/// A system-exclusive message, decoded.
#[derive(Debug, Clone, PartialEq)]
pub enum Sysex {
    /// A dump of a voice or of a bank.
    Dump(Dump),
    /// A parameter change.
    Change(ParameterChange),
    /// A dump request.
    Request(Request),
}

/// A system-exclusive message of a file: where it lies, and what it says or
/// the reason it is not read.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// The offset of its F0 in the file.
    pub offset: usize,
    /// Its length in bytes: from its F0 to its F7, or, where it does not end,
    /// to the last byte before what cut it short.
    pub length: usize,
    /// What it says, or the reason it is not read.
    pub result: Result<Sysex, Error>,
}

/// The system-exclusive messages of a file, in order, as [`read`] gives them.
#[derive(Debug, Clone)]
pub struct Messages<'a> {
    /// The bytes not yet read.
    rest: &'a [u8],
    /// The offset of `rest` in the file.
    offset: usize,
    /// How messages are read.
    options: Options,
}

impl Iterator for Messages<'_> {
    type Item = Message;

    fn next(&mut self) -> Option<Message> {
        let start = self.rest.iter().position(|&byte| byte == START)?;
        let (_, from) = self.rest.split_at(start);
        // The first status byte after the F0: the F7 that ends the message, or
        // what cuts it short. A message cut short ends before it, and the next
        // is looked for from it, since it may be the F0 of another.
        let status = from.iter().skip(1).position(|&byte| byte >= 0x80).map(|i| i + 1);
        let (length, result) = match status {
            Some(end) if from.get(end) == Some(&END) => {
                let (message, _) = from.split_at(end + 1);
                (end + 1, decode(message, self.options))
            }
            Some(end) => (end, Err(Error::Unterminated)),
            None => (from.len(), Err(Error::Unterminated)),
        };
        let (_, rest) = from.split_at(length);
        let message = Message {
            offset: self.offset + start,
            length,
            result,
        };
        self.rest = rest;
        self.offset += start + length;
        Some(message)
    }
}

/// Every system-exclusive message in a file's bytes, in order, each decoded or
/// with the reason it is not.
///
/// A message runs from an F0 to its F7. One that another status byte cuts
/// short, or the end of the bytes, is given as [`Error::Unterminated`], and the
/// next is looked for from that byte on. Bytes outside messages are passed
/// over.
#[must_use]
pub fn read(bytes: &[u8], options: Options) -> Messages<'_> {
    Messages {
        rest: bytes,
        offset: 0,
        options,
    }
}

/// One system-exclusive message, decoded: a dump, a parameter change or a dump
/// request, what it holds taken as `options` say.
///
/// # Errors
///
/// [`Error::NotSysEx`] where the bytes are not one system-exclusive message,
/// [`Error::Unsupported`] where the message is of a kind this reader does not
/// take, [`Error::Length`] where its byte count or length is not its kind's,
/// and [`Error::Refused`] where the options refuse what it holds.
pub fn decode(message: &[u8], options: Options) -> Result<Sysex, Error> {
    let [START, inner @ .., END] = message else {
        return Err(Error::NotSysEx);
    };
    if inner.iter().any(|&byte| byte >= 0x80) {
        return Err(Error::NotSysEx);
    }
    // The maker, then the sub-status with the channel.
    let &[YAMAHA, status, ref rest @ ..] = inner else {
        return Err(Error::Unsupported);
    };
    let channel = Channel(status & 0x0f);
    match status >> 4 {
        DUMP => dump_of(channel, rest, options).map(Sysex::Dump),
        CHANGE => change_of(channel, rest, options).map(Sysex::Change),
        REQUEST => request_of(channel, rest).map(Sysex::Request),
        _ => Err(Error::Unsupported),
    }
}

/// What a dump holds, by its format number and its byte count.
#[derive(Debug, Clone, Copy)]
enum DumpKind {
    Voice,
    Bank,
    Performance,
    Performances,
    Condition,
    Supplement,
    Supplements,
}

/// The dump of a message's bytes after its sub-status: the format, the byte
/// count, the data and the checksum.
fn dump_of(channel: Channel, rest: &[u8], options: Options) -> Result<Dump, Error> {
    let Some((&format, rest)) = rest.split_first() else {
        return Err(Error::Unsupported);
    };
    if !matches!(
        format,
        VOICE_FORMAT
            | PERFORMANCE_FORMAT
            | PERFORMANCES_FORMAT
            | SUPPLEMENT_FORMAT
            | SUPPLEMENTS_FORMAT
            | BANK_FORMAT
            | CONDITION_FORMAT
    ) {
        return Err(Error::Unsupported);
    }
    let &[high, low, ref rest @ ..] = rest else {
        return Err(Error::Length);
    };
    let Some((&checksum, data)) = rest.split_last() else {
        return Err(Error::Length);
    };
    let mut findings = Vec::new();
    let kind = match (format, [high, low]) {
        (VOICE_FORMAT, VOICE_COUNT) => DumpKind::Voice,
        (BANK_FORMAT, BANK_COUNT) => DumpKind::Bank,
        (PERFORMANCE_FORMAT, PERFORMANCE_COUNT) => DumpKind::Performance,
        // The dump of one performance under the number of the dump of 64, as
        // [DX7-MANUAL] prints it: read, and reported.
        (PERFORMANCES_FORMAT, PERFORMANCE_COUNT) => {
            findings.push(Finding::PerformanceFormat);
            DumpKind::Performance
        }
        (PERFORMANCES_FORMAT, PERFORMANCES_COUNT) => DumpKind::Performances,
        (CONDITION_FORMAT, CONDITION_COUNT) => DumpKind::Condition,
        (SUPPLEMENT_FORMAT, SUPPLEMENT_COUNT) => DumpKind::Supplement,
        (SUPPLEMENTS_FORMAT, SUPPLEMENTS_COUNT) => DumpKind::Supplements,
        _ => return Err(Error::Length),
    };
    let expected = checksum_of(data);
    if checksum != expected {
        let finding = Finding::Checksum {
            expected,
            found: checksum,
        };
        if !options.accept_bad_checksum {
            return Err(Error::Refused(finding));
        }
        findings.push(finding);
    }
    let content = content_of(kind, data, options, &mut findings)?;
    Ok(Dump {
        channel,
        content,
        findings,
    })
}

/// What a dump of a kind holds, of its data, each byte checked as the options
/// say.
fn content_of(kind: DumpKind, data: &[u8], options: Options, findings: &mut Vec<Finding>) -> Result<Content, Error> {
    Ok(match kind {
        DumpKind::Voice => {
            let Ok(steps) = <&[u8; VOICE_BYTES]>::try_from(data) else {
                return Err(Error::Length);
            };
            Content::Voice(Box::new(voice_of(steps, 0, options, findings)?))
        }
        DumpKind::Bank => {
            let Ok(data) = <&[u8; BANK_BYTES]>::try_from(data) else {
                return Err(Error::Length);
            };
            let (packed, _) = data.as_chunks::<PACKED_BYTES>();
            let mut voices = Vec::with_capacity(32);
            for (place, packed) in (0..).zip(packed) {
                let steps = unpack(packed, place, options, findings)?;
                voices.push(voice_of(&steps, place, options, findings)?);
            }
            let Ok(voices) = <Box<[Voice; 32]>>::try_from(voices.into_boxed_slice()) else {
                return Err(Error::Length);
            };
            Content::Bank(voices)
        }
        DumpKind::Performance => {
            let Ok(steps) = <&[u8; PERFORMANCE_BYTES]>::try_from(data) else {
                return Err(Error::Length);
            };
            Content::Performance(Box::new(performance_of(steps, options, findings)?))
        }
        DumpKind::Performances => {
            let Ok(data) = <&[u8; PERFORMANCES_BYTES]>::try_from(data) else {
                return Err(Error::Length);
            };
            let (packed, _) = data.as_chunks::<PACKED_PERFORMANCE_BYTES>();
            let mut performances = Vec::with_capacity(64);
            for (place, packed) in (0..).zip(packed) {
                performances.push(unpack_performance(packed, place, options, findings)?);
            }
            let Ok(performances) = <Box<[Performance; 64]>>::try_from(performances.into_boxed_slice()) else {
                return Err(Error::Length);
            };
            Content::Performances(performances)
        }
        DumpKind::Condition => {
            let Ok(data) = <&[u8; CONDITION_BYTES]>::try_from(data) else {
                return Err(Error::Length);
            };
            Content::Condition(condition_of(data, options, findings)?)
        }
        DumpKind::Supplement => {
            let Ok(steps) = <&[u8; SUPPLEMENT_BYTES]>::try_from(data) else {
                return Err(Error::Length);
            };
            Content::Supplement(Box::new(supplement_of(steps, 0, options, findings)?))
        }
        DumpKind::Supplements => {
            let Ok(data) = <&[u8; SUPPLEMENTS_BYTES]>::try_from(data) else {
                return Err(Error::Length);
            };
            let (packed, _) = data.as_chunks::<PACKED_SUPPLEMENT_BYTES>();
            let mut supplements = Vec::with_capacity(32);
            for (place, packed) in (0..).zip(packed) {
                let steps = unpack_supplement(packed, place, options, findings)?;
                supplements.push(supplement_of(&steps, place, options, findings)?);
            }
            let Ok(supplements) = <Box<[Supplement; 32]>>::try_from(supplements.into_boxed_slice()) else {
                return Err(Error::Length);
            };
            Content::Supplements(supplements)
        }
    })
}

/// The parameter change of a message's bytes after its sub-status: the group
/// byte, the parameter's or switch's number and the data.
fn change_of(channel: Channel, rest: &[u8], options: Options) -> Result<ParameterChange, Error> {
    let &[group, number, ref data @ ..] = rest else {
        return Err(Error::Length);
    };
    let target = target_of(group, number).ok_or(Error::Unsupported)?;
    let &[data] = data else {
        return Err(Error::Length);
    };
    let (change, finding) = target.change(data);
    if let Some(finding) = finding
        && options.out_of_range == OutOfRange::Reject
    {
        return Err(Error::Refused(finding));
    }
    Ok(ParameterChange {
        channel,
        change,
        finding,
    })
}

/// What a parameter change's group byte and number name, if they name anything
/// this reader takes: the number's high bits are the group byte's low two in
/// the voice group, and 0 in the others but the TX function group's, 1.
fn target_of(group: u8, number: u8) -> Option<Target> {
    match (group >> 2, group & 0x03) {
        (VOICE_GROUP, high) if [high, number] == OPERATOR_SELECT => Some(Target::OperatorSelect),
        (VOICE_GROUP, high) => u8::try_from(u16::from(high) << 7 | u16::from(number))
            .ok()
            .and_then(Parameter::new)
            .map(Target::Voice),
        (PERFORMANCE_GROUP, 0) => DxPerformanceParameter::new(number).map(Target::Performance),
        (FUNCTION_GROUP, 0) => FunctionParameter::new(number)
            .map(Target::Function)
            .or_else(|| Button::new(number).map(Target::Panel)),
        (TX_GROUP, 1) => TxFunctionParameter::new(number).map(Target::Tx),
        (DX7II_GROUP, 0) => SupplementParameter::new(number).map(Target::Supplement),
        _ if group == SWITCH_REMOTE => RemoteSwitch::new(number).map(Target::Remote),
        _ => None,
    }
}

/// What a parameter change's group byte and number name.
#[derive(Debug, Clone, Copy)]
enum Target {
    Voice(Parameter),
    Function(FunctionParameter),
    Panel(Button),
    Remote(RemoteSwitch),
    Performance(DxPerformanceParameter),
    Tx(TxFunctionParameter),
    OperatorSelect,
    Supplement(SupplementParameter),
}

impl Target {
    /// The change of a datum, and what the datum held that was not as the
    /// format has it: a step outside its parameter's range, taken as the
    /// range's nearer end, or a switch's data neither 0 nor 127, taken as
    /// pressed unless it is 0, as the DX7's firmware takes it \[AJXS-ROM\].
    fn change(self, data: u8) -> (Change, Option<Finding>) {
        match self {
            Self::Voice(parameter) => {
                let largest = parameter.largest();
                let finding = (data > largest).then_some(Finding::OutOfRange {
                    voice: 0,
                    parameter,
                    step: data,
                });
                let step = VoiceStep {
                    parameter,
                    step: data.min(largest),
                };
                (Change::Voice(step), finding)
            }
            Self::Function(parameter) => {
                let largest = parameter.largest();
                let finding = (data > largest).then_some(Finding::FunctionOutOfRange { parameter, step: data });
                let step = FunctionStep {
                    parameter,
                    step: data.min(largest),
                };
                (Change::Function(step), finding)
            }
            Self::Panel(button) => (Change::Panel(button, data != 0), switch_finding(data)),
            Self::Remote(switch) => (Change::Remote(switch, data != 0), switch_finding(data)),
            Self::Performance(parameter) => {
                let (smallest, largest) = (parameter.smallest(), parameter.largest());
                let finding = (!(smallest..=largest).contains(&data))
                    .then_some(Finding::DxPerformanceOutOfRange { parameter, step: data });
                let step = PerformanceStep {
                    parameter,
                    step: data.max(smallest).min(largest),
                };
                (Change::Performance(step), finding)
            }
            Self::Tx(parameter) if parameter.is_switch() => {
                let step = TxStep {
                    parameter,
                    step: switch_data(data != 0),
                };
                (Change::Tx(step), switch_finding(data))
            }
            Self::Tx(parameter) => {
                let largest = parameter.largest();
                let finding = (data > largest).then_some(Finding::TxFunctionOutOfRange { parameter, step: data });
                let step = TxStep {
                    parameter,
                    step: data.min(largest),
                };
                (Change::Tx(step), finding)
            }
            Self::OperatorSelect => {
                let finding = (data > 5).then_some(Finding::OperatorSelectOutOfRange { step: data });
                (Change::OperatorSelect(OperatorSelect(data.min(5))), finding)
            }
            Self::Supplement(parameter) => {
                let largest = parameter.largest();
                let finding = (data > largest).then_some(Finding::SupplementOutOfRange {
                    supplement: 0,
                    parameter,
                    step: data,
                });
                let step = SupplementStep {
                    parameter,
                    step: data.min(largest),
                };
                (Change::Supplement(step), finding)
            }
        }
    }
}

/// The finding of a switch's data, if it is neither 0 nor 127.
fn switch_finding(data: u8) -> Option<Finding> {
    (data != 0 && data != PRESSED).then_some(Finding::SwitchData { data })
}

/// The dump request of a message's bytes after its sub-status: the format,
/// and the names of a universal bulk dump.
fn request_of(channel: Channel, rest: &[u8]) -> Result<Request, Error> {
    let requested = match *rest {
        [VOICE_FORMAT] => Requested::Voice,
        [PERFORMANCE_FORMAT] => Requested::Performance,
        [PERFORMANCES_FORMAT] => Requested::Performances,
        [SUPPLEMENT_FORMAT] => Requested::Supplement,
        [SUPPLEMENTS_FORMAT] => Requested::Supplements,
        [BANK_FORMAT] => Requested::Bank,
        [CONDITION_FORMAT] => Requested::Condition,
        [UNIVERSAL_FORMAT, ref names @ ..] => {
            let Ok(&[c0, c1, c2, c3, f0, f1, f2, f3, f4, f5]) = <&[u8; 10]>::try_from(names) else {
                return Err(Error::Length);
            };
            // The bytes are below 128, as those of every message are.
            let (Some(classification), Some(format)) = (
                UniversalName::new([c0, c1, c2, c3]),
                UniversalName::new([f0, f1, f2, f3, f4, f5]),
            ) else {
                return Err(Error::NotSysEx);
            };
            Requested::Universal { classification, format }
        }
        [
            VOICE_FORMAT | PERFORMANCE_FORMAT | PERFORMANCES_FORMAT | SUPPLEMENT_FORMAT | SUPPLEMENTS_FORMAT
            | BANK_FORMAT | CONDITION_FORMAT,
            ..,
        ]
        | [] => return Err(Error::Length),
        _ => return Err(Error::Unsupported),
    };
    Ok(Request { channel, requested })
}

/// A single-voice dump of a voice, on a channel: a message of 163 bytes. Each
/// value is rounded to a step by `rounding`, and what the dump could not hold
/// of the voice is given beside it.
#[must_use]
pub fn write_voice(voice: &Voice, channel: Channel, rounding: Rounding) -> (Vec<u8>, Loss) {
    let mut loss = Loss::default();
    let steps = steps_of(voice, 0, rounding, &mut loss);
    (message(channel, VOICE_FORMAT, VOICE_COUNT, &steps), loss)
}

/// A 32-voice dump of a bank, on a channel: a message of 4104 bytes. Each value
/// is rounded to a step by `rounding`, and what the dump could not hold of the
/// voices is given beside it.
#[must_use]
pub fn write_bank(voices: &[Voice; 32], channel: Channel, rounding: Rounding) -> (Vec<u8>, Loss) {
    let mut loss = Loss::default();
    let mut data = Vec::with_capacity(BANK_BYTES);
    for (place, voice) in (0..).zip(voices) {
        data.extend_from_slice(&pack(&steps_of(voice, place, rounding, &mut loss)));
    }
    (message(channel, BANK_FORMAT, BANK_COUNT, &data), loss)
}

/// The DX7II's dump of one supplement, on a channel: a message of 57 bytes.
/// Each value is rounded to a step by `rounding`, and the parameters whose
/// values lay between steps are given beside it.
#[must_use]
pub fn write_supplement(
    supplement: &Supplement,
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, Vec<(u8, SupplementParameter)>) {
    let mut rounded = Vec::new();
    let steps = steps_of_supplement(supplement, 0, rounding, &mut rounded);
    (message(channel, SUPPLEMENT_FORMAT, SUPPLEMENT_COUNT, &steps), rounded)
}

/// The DX7II's dump of 32 supplements, on a channel: a message of 1128 bytes.
/// Each value is rounded to a step by `rounding`, and the parameters whose
/// values lay between steps are given beside it, by the place of their
/// supplement.
#[must_use]
pub fn write_supplements(
    supplements: &[Supplement; 32],
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, Vec<(u8, SupplementParameter)>) {
    let mut rounded = Vec::new();
    let mut data = Vec::with_capacity(SUPPLEMENTS_BYTES);
    for (place, supplement) in (0..).zip(supplements) {
        let steps = steps_of_supplement(supplement, place, rounding, &mut rounded);
        data.extend_from_slice(&pack_supplement(&steps));
    }
    (message(channel, SUPPLEMENTS_FORMAT, SUPPLEMENTS_COUNT, &data), rounded)
}

/// The dump of one performance, on a channel: a message of 102 bytes, under
/// format 1. Each value is rounded to a step by `rounding`, and what the dump
/// could not hold of the performance is given beside it.
#[must_use]
pub fn write_performance(
    performance: &Performance,
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, PerformanceLoss) {
    let mut loss = PerformanceLoss::default();
    let steps = steps_of_performance(performance, 0, rounding, &mut loss);
    if performance.key_assign != KeyAssign::Single {
        loss.key_assign.push(0);
    }
    (message(channel, PERFORMANCE_FORMAT, PERFORMANCE_COUNT, &steps), loss)
}

/// The dump of 64 performances, on a channel: a message of 4104 bytes. Each
/// value is rounded to a step by `rounding`, and what the dump could not hold
/// of the performances is given beside it.
#[must_use]
pub fn write_performances(
    performances: &[Performance; 64],
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, PerformanceLoss) {
    let mut loss = PerformanceLoss::default();
    let mut data = Vec::with_capacity(PERFORMANCES_BYTES);
    for (place, performance) in (0..).zip(performances) {
        let steps = steps_of_performance(performance, place, rounding, &mut loss);
        if performance.unnamed != Unnamed::ZERO {
            loss.unnamed.push(place);
        }
        data.extend_from_slice(&pack_performance(&steps, performance.key_assign));
    }
    (message(channel, PERFORMANCES_FORMAT, PERFORMANCES_COUNT, &data), loss)
}

/// The condition acknowledge, on a channel: a message of 24 bytes. The TX7
/// sends it on channel 0 \[TX7-MANUAL\].
#[must_use]
pub fn write_condition(condition: &Condition, channel: Channel) -> Vec<u8> {
    let mut data = Vec::with_capacity(CONDITION_BYTES);
    data.extend_from_slice(&condition.classification.0);
    data.extend_from_slice(&condition.model.0);
    data.extend_from_slice(&[
        condition.version.0,
        condition.revision.0,
        condition.flags.byte(),
        condition.receive_channel.0,
        condition.battery.0,
        condition.data.0,
    ]);
    message(channel, CONDITION_FORMAT, CONDITION_COUNT, &data)
}

/// A parameter change on a channel: a message of 7 bytes.
#[must_use]
pub fn write_change(channel: Channel, change: Change) -> Vec<u8> {
    let (group, number, data) = match change {
        Change::Voice(step) => {
            let number = step.parameter.number();
            (VOICE_GROUP << 2 | number >> 7, number & 0x7f, step.step)
        }
        Change::Function(step) => (FUNCTION_GROUP << 2, step.parameter.number(), step.step),
        Change::Panel(button, pressed) => (FUNCTION_GROUP << 2, button.number(), switch_data(pressed)),
        Change::Remote(switch, pressed) => (SWITCH_REMOTE, switch.number(), switch_data(pressed)),
        Change::Performance(step) => (PERFORMANCE_GROUP << 2, step.parameter.number(), step.step),
        Change::Tx(step) => (TX_GROUP << 2 | 1, step.parameter.number(), step.step),
        Change::OperatorSelect(select) => {
            let [high, number] = OPERATOR_SELECT;
            (VOICE_GROUP << 2 | high, number, select.0)
        }
        Change::Supplement(step) => (DX7II_GROUP << 2, step.parameter.number(), step.step),
    };
    vec![START, YAMAHA, CHANGE << 4 | channel.0, group, number, data, END]
}

/// A switch's data: 127 pressed, 0 released.
const fn switch_data(pressed: bool) -> u8 {
    if pressed { PRESSED } else { 0 }
}

/// A dump request on a channel: a message of 5 bytes, or 15 for a universal
/// bulk dump.
#[must_use]
pub fn write_request(channel: Channel, requested: Requested) -> Vec<u8> {
    let mut message = vec![START, YAMAHA, REQUEST << 4 | channel.0];
    match requested {
        Requested::Voice => message.push(VOICE_FORMAT),
        Requested::Performance => message.push(PERFORMANCE_FORMAT),
        Requested::Performances => message.push(PERFORMANCES_FORMAT),
        Requested::Supplement => message.push(SUPPLEMENT_FORMAT),
        Requested::Supplements => message.push(SUPPLEMENTS_FORMAT),
        Requested::Bank => message.push(BANK_FORMAT),
        Requested::Condition => message.push(CONDITION_FORMAT),
        Requested::Universal { classification, format } => {
            message.push(UNIVERSAL_FORMAT);
            message.extend_from_slice(&classification.0);
            message.extend_from_slice(&format.0);
        }
    }
    message.push(END);
    message
}

/// What a dump could not hold of the voices it was written from: what comes
/// back changed when it is read.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Loss {
    /// The parameters whose values lay between steps, and were rounded: by the
    /// place of the voice, 0 in a single-voice dump and 0 to 31 in a bank, and
    /// the parameter.
    pub rounded: Vec<(u8, Parameter)>,
    /// The places of the voices with an operator switched off: a dump holds no
    /// switches, and every operator of a voice read from one is on.
    pub switched_off: Vec<u8>,
}

impl Loss {
    /// Whether the dump holds its voices exactly.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rounded.is_empty() && self.switched_off.is_empty()
    }
}

/// What a dump of performances could not hold of those it was written from:
/// what comes back changed when it is read.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PerformanceLoss {
    /// The parameters whose values lay between steps, and were rounded: by the
    /// place of the performance, 0 in the dump of one and 0 to 63 in the dump
    /// of 64, and the parameter.
    pub rounded: Vec<(u8, PerformanceParameter)>,
    /// The places of the performances whose key assign mode is not single,
    /// which the dump of one does not hold.
    pub key_assign: Vec<u8>,
    /// The places of the performances with an unnamed byte other than 0, which
    /// the dump of 64 does not hold.
    pub unnamed: Vec<u8>,
}

impl PerformanceLoss {
    /// Whether the dump holds its performances exactly.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rounded.is_empty() && self.key_assign.is_empty() && self.unnamed.is_empty()
    }
}

/// The checksum of a dump's data: the two's complement of their sum in seven
/// bits, so that the data and the checksum sum to a multiple of 128
/// \[DEXED-SYSEX\].
fn checksum_of(data: &[u8]) -> u8 {
    data.iter()
        .fold(0_u8, |sum, &byte| sum.wrapping_add(byte))
        .wrapping_neg()
        & 0x7f
}

/// A dump's message: the header with its channel, format and byte count, the
/// data, their checksum and the end.
fn message(channel: Channel, format: u8, count: [u8; 2], data: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(data.len() + 8);
    message.extend_from_slice(&[START, YAMAHA, DUMP << 4 | channel.0, format]);
    message.extend_from_slice(&count);
    message.extend_from_slice(data);
    message.push(checksum_of(data));
    message.push(END);
    message
}

/// A finding taken as the options say: reported, or the message refused if
/// `syx.out_of_range` is `reject`.
fn note(finding: Finding, options: Options, findings: &mut Vec<Finding>) -> Result<(), Error> {
    if options.out_of_range == OutOfRange::Reject {
        return Err(Error::Refused(finding));
    }
    findings.push(finding);
    Ok(())
}

/// The supplement of the 49 bytes of the dump of one, in the order of its
/// parameters' numbers, each checked against its parameter's range: a step
/// beyond it is a finding, and either the largest step or the dump's refusal.
fn supplement_of(
    steps: &[u8; SUPPLEMENT_BYTES],
    place: u8,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<Supplement, Error> {
    let mut supplement = Supplement::zero();
    for (parameter, &step) in SupplementParameter::all().zip(steps) {
        if !supplement.set_step(parameter, step) {
            let finding = Finding::SupplementOutOfRange {
                supplement: place,
                parameter,
                step,
            };
            note(finding, options, findings)?;
        }
    }
    Ok(supplement)
}

/// The steps of a supplement's parameters, as the dump of one holds them, and
/// the parameters whose values they round.
fn steps_of_supplement(
    supplement: &Supplement,
    place: u8,
    rounding: Rounding,
    rounded: &mut Vec<(u8, SupplementParameter)>,
) -> [u8; SUPPLEMENT_BYTES] {
    let mut steps = [0; SUPPLEMENT_BYTES];
    for (parameter, step) in SupplementParameter::all().zip(&mut steps) {
        *step = supplement.step(parameter, rounding);
        if !supplement.is_step(parameter) {
            rounded.push((place, parameter));
        }
    }
    steps
}

/// The 35 bytes of a packed supplement of the 49 steps of the dump of one,
/// every step within its range, laid out as [`SUPPLEMENT_USED`] says. The
/// steps lie in the order of the parameters' numbers: 0 to 38 at their
/// numbers, 64 to 73 at 39 to 48.
fn pack_supplement(steps: &[u8; SUPPLEMENT_BYTES]) -> [u8; PACKED_SUPPLEMENT_BYTES] {
    let mut packed = [0; PACKED_SUPPLEMENT_BYTES];
    // The scaling modes of OP6 to OP1, parameters 0 to 5, in bits 0 to 5.
    packed[0] = steps[..6].iter().rev().fold(0, |byte, &step| byte << 1 | step);
    // The amplitude modulation sensitivities of OP6 to OP1, parameters 6 to
    // 11, two to a byte, the lower-numbered in bits 2–0.
    packed[1] = steps[7] << 3 | steps[6];
    packed[2] = steps[9] << 3 | steps[8];
    packed[3] = steps[11] << 3 | steps[10];
    packed[4] = steps[19] << 4 | steps[14] << 3 | steps[13] << 2 | steps[12];
    packed[5] = steps[16] << 2 | steps[15];
    packed[6] = steps[18] << 4 | steps[17];
    packed[7] = steps[21] << 1 | steps[20];
    packed[8..24].copy_from_slice(&steps[22..38]);
    packed[24] = steps[38];
    // Parameters 64 to 71, and 72 and 73.
    packed[26..34].copy_from_slice(&steps[39..47]);
    packed[34] = steps[48] << 3 | steps[47];
    packed
}

/// The 49 steps of the dump of one supplement of a packed one's 35, which
/// [`pack_supplement`] lays out. A byte with unused bits set is a finding, and
/// the bits are ignored or the dump refused; a field wider than its range, as
/// the pitch bend's range's four bits are for 0 to 12, may give a step beyond
/// it, which [`supplement_of`] finds.
fn unpack_supplement(
    packed: &[u8; PACKED_SUPPLEMENT_BYTES],
    place: u8,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<[u8; SUPPLEMENT_BYTES], Error> {
    for (byte, (&value, &used)) in (0..).zip(packed.iter().zip(&SUPPLEMENT_USED)) {
        let bits = value & !used;
        if bits != 0 {
            let finding = Finding::SupplementUnusedBits {
                supplement: place,
                byte,
                bits,
            };
            note(finding, options, findings)?;
        }
    }
    let mut steps = [0; SUPPLEMENT_BYTES];
    for (bit, step) in (0..6).zip(&mut steps[..6]) {
        *step = packed[0] >> bit & 0x01;
    }
    steps[6] = packed[1] & 0x07;
    steps[7] = packed[1] >> 3 & 0x07;
    steps[8] = packed[2] & 0x07;
    steps[9] = packed[2] >> 3 & 0x07;
    steps[10] = packed[3] & 0x07;
    steps[11] = packed[3] >> 3 & 0x07;
    steps[12] = packed[4] & 0x03;
    steps[13] = packed[4] >> 2 & 0x01;
    steps[14] = packed[4] >> 3 & 0x01;
    steps[19] = packed[4] >> 4 & 0x07;
    steps[15] = packed[5] & 0x03;
    steps[16] = packed[5] >> 2 & 0x0f;
    steps[17] = packed[6] & 0x0f;
    steps[18] = packed[6] >> 4 & 0x03;
    steps[20] = packed[7] & 0x01;
    steps[21] = packed[7] >> 1 & 0x0f;
    steps[22..38].copy_from_slice(&packed[8..24]);
    steps[38] = packed[24] & 0x07;
    steps[39..47].copy_from_slice(&packed[26..34]);
    steps[47] = packed[34] & 0x07;
    steps[48] = packed[34] >> 3 & 0x01;
    Ok(steps)
}

/// Sets a performance's parameter to a step: a step beyond its range is a
/// finding, and either the largest step or the dump's refusal.
fn put(
    performance: &mut Performance,
    parameter: PerformanceParameter,
    step: u8,
    place: u8,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<(), Error> {
    if performance.set_step(parameter, step) {
        return Ok(());
    }
    note(
        Finding::PerformanceOutOfRange {
            performance: place,
            parameter,
            step,
        },
        options,
        findings,
    )
}

/// The performance of the 94 bytes of the dump of one, each checked against
/// its parameter's range.
fn performance_of(
    steps: &[u8; PERFORMANCE_BYTES],
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<Performance, Error> {
    let mut performance = Performance::zero();
    for (parameter, &step) in PerformanceParameter::all().zip(steps) {
        put(&mut performance, parameter, step, 0, options, findings)?;
    }
    Ok(performance)
}

/// The steps of a performance's parameters, as the dump of one holds them, and
/// what they lose of it.
fn steps_of_performance(
    performance: &Performance,
    place: u8,
    rounding: Rounding,
    loss: &mut PerformanceLoss,
) -> [u8; PERFORMANCE_BYTES] {
    let mut steps = [0; PERFORMANCE_BYTES];
    for (parameter, step) in PerformanceParameter::all().zip(&mut steps) {
        *step = performance.step(parameter, rounding);
        if !performance.is_step(parameter) {
            loss.rounded.push((place, parameter));
        }
    }
    steps
}

/// The 64 bytes of a packed performance of the 94 steps of the dump of one,
/// every step within its range, and a key assign mode, laid out as
/// [`PERFORMANCE_USED`] says. The steps' unnamed bytes are not held.
fn pack_performance(steps: &[u8; PERFORMANCE_BYTES], key_assign: KeyAssign) -> [u8; PACKED_PERFORMANCE_BYTES] {
    let mut packed = [0; PACKED_PERFORMANCE_BYTES];
    let (sides, common) = packed.split_at_mut(32);
    let (sides, _) = sides.as_chunks_mut::<16>();
    let (steps_of_sides, rest) = steps.split_at(60);
    let (steps_of_sides, _) = steps_of_sides.as_chunks::<30>();
    for (to, from) in sides.iter_mut().zip(steps_of_sides) {
        to[0] = from[2] << 6;
        to[1] = (from[4] & 0x07) << 4 | from[3];
        to[2] = from[5];
        to[3] = from[7] << 1 | from[6];
        // The assignment and the sensitivity of the modulation wheel, the foot
        // controller, the aftertouch and the breath controller.
        for (byte, offset) in [(4, 9), (5, 11), (6, 13), (7, 15)] {
            to[byte] = from[offset + 1] << 4 | from[offset];
        }
        to[14] = from[26];
        to[15] = (from[4] >> 3) << 6;
    }
    // The voice memory select is byte 61, 1 of `rest`, and the name 64 to 93.
    common[0] = rest[1] << 2 | key_assign.step();
    common[2..].copy_from_slice(&rest[4..]);
    packed
}

/// The performance of the 64 bytes of a packed one. A byte with unused bits
/// set is a finding, and the bits are ignored or the dump refused; a field
/// wider than its range, as the pitch bend's range's four bits are for 0 to 12,
/// may give a step beyond it, which is a finding as well.
fn unpack_performance(
    packed: &[u8; PACKED_PERFORMANCE_BYTES],
    place: u8,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<Performance, Error> {
    for (byte, (&value, &used)) in (0..).zip(packed.iter().zip(&PERFORMANCE_USED)) {
        let bits = value & !used;
        if bits != 0 {
            let finding = Finding::PerformanceUnusedBits {
                performance: place,
                byte,
                bits,
            };
            note(finding, options, findings)?;
        }
    }
    let mut performance = Performance::zero();
    let (sides, common) = packed.split_at(32);
    let (sides, _) = sides.as_chunks::<16>();
    for (base, side) in [0_u8, 30].into_iter().zip(sides) {
        let steps = [
            (2, side[0] >> 6),
            (3, side[1] & 0x0f),
            (4, (side[15] >> 6 & 0x01) << 3 | (side[1] >> 4 & 0x07)),
            (5, side[2]),
            (6, side[3] & 0x01),
            (7, side[3] >> 1 & 0x01),
            (9, side[4] & 0x0f),
            (10, side[4] >> 4 & 0x07),
            (11, side[5] & 0x0f),
            (12, side[5] >> 4 & 0x07),
            (13, side[6] & 0x0f),
            (14, side[6] >> 4 & 0x07),
            (15, side[7] & 0x0f),
            (16, side[7] >> 4 & 0x07),
            (26, side[14] & 0x07),
        ];
        for (offset, step) in steps {
            let parameter = PerformanceParameter(base + offset);
            put(&mut performance, parameter, step, place, options, findings)?;
        }
    }
    let select = PerformanceParameter(61);
    put(
        &mut performance,
        select,
        common[0] >> 2 & 0x01,
        place,
        options,
        findings,
    )?;
    let mode = common[0] & 0x03;
    if KeyAssign::from_step(mode).is_none() {
        note(
            Finding::KeyAssignOutOfRange {
                performance: place,
                step: mode,
            },
            options,
            findings,
        )?;
    }
    performance.key_assign = KeyAssign::saturating(mode);
    for (number, &byte) in (64..).zip(&common[2..]) {
        put(
            &mut performance,
            PerformanceParameter(number),
            byte,
            place,
            options,
            findings,
        )?;
    }
    Ok(performance)
}

/// The condition acknowledge of its 16 bytes: the classification and the
/// model names, the version and the revision, the flags, the receive channel,
/// the battery's voltage and the fourth condition data byte \[TX7-MANUAL\].
fn condition_of(
    data: &[u8; CONDITION_BYTES],
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<Condition, Error> {
    let [
        c0,
        c1,
        c2,
        c3,
        m0,
        m1,
        m2,
        m3,
        m4,
        m5,
        version,
        revision,
        flags,
        channel,
        battery,
        rest,
    ] = *data;
    // The bytes are below 128, as those of every message are.
    let (Some(classification), Some(model)) = (
        UniversalName::new([c0, c1, c2, c3]),
        UniversalName::new([m0, m1, m2, m3, m4, m5]),
    ) else {
        return Err(Error::NotSysEx);
    };
    if channel > 15 {
        note(Finding::ConditionChannel { channel }, options, findings)?;
    }
    Ok(Condition {
        classification,
        model,
        version: Byte(version),
        revision: Byte(revision),
        flags: ConditionFlags::of(flags),
        receive_channel: Channel(channel.min(15)),
        battery: Byte(battery),
        data: Byte(rest),
    })
}

/// The voice of the 155 bytes of a single-voice dump, each checked against
/// its parameter's range: a step beyond it is a finding, and either the
/// largest step or the dump's refusal.
fn voice_of(
    steps: &[u8; VOICE_BYTES],
    place: u8,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<Voice, Error> {
    let mut voice = Voice::zero();
    for (parameter, &step) in Parameter::in_dump().zip(steps) {
        if !voice.set_step(parameter, step) {
            let finding = Finding::OutOfRange {
                voice: place,
                parameter,
                step,
            };
            if options.out_of_range == OutOfRange::Reject {
                return Err(Error::Refused(finding));
            }
            findings.push(finding);
        }
    }
    Ok(voice)
}

/// The steps of a voice's parameters, as a single-voice dump holds them, and
/// what they lose of it.
fn steps_of(voice: &Voice, place: u8, rounding: Rounding, loss: &mut Loss) -> [u8; VOICE_BYTES] {
    let mut steps = [0; VOICE_BYTES];
    for (parameter, step) in Parameter::in_dump().zip(&mut steps) {
        *step = voice.step(parameter, rounding);
        if !voice.is_step(parameter) {
            loss.rounded.push((place, parameter));
        }
    }
    if voice.operators.iter().any(|operator| !operator.on) {
        loss.switched_off.push(place);
    }
    steps
}

/// The 128 bytes of a packed voice of the 155 of a single-voice dump, every
/// step within its range \[DEXED-SYSEX\]: of each operator's 21, R1 to R4, L1
/// to L4, the break point and the depths stay a byte each; the left and right
/// curves share byte 11, in bits 1–0 and 3–2; the rate scaling and the detune
/// byte 12, in bits 2–0 and 6–3; the amplitude and velocity sensitivities byte
/// 13, in bits 1–0 and 4–2; the output level is byte 14; the mode and the
/// coarse frequency share byte 15, in bit 0 and bits 5–1; and the fine
/// frequency is byte 16. Of the voice's own, the pitch envelope is bytes 102
/// to 109 and the algorithm 110; the feedback and the oscillators' key sync
/// share byte 111, in bits 2–0 and bit 3; the LFO's speed, delay and depths are
/// 112 to 115; its key sync, wave and the pitch sensitivity share byte 116, in
/// bit 0, bits 3–1 and bits 6–4; the transposition is 117 and the name 118 to
/// 127.
fn pack(steps: &[u8; VOICE_BYTES]) -> [u8; PACKED_BYTES] {
    let mut packed = [0; PACKED_BYTES];
    let (operators, voice) = steps.split_at(OPERATOR_STEPS);
    let (operators, _) = operators.as_chunks::<21>();
    let (packed_operators, _) = packed.as_chunks_mut::<17>();
    for (from, to) in operators.iter().zip(packed_operators) {
        to[..11].copy_from_slice(&from[..11]);
        to[11] = from[12] << 2 | from[11];
        to[12] = from[20] << 3 | from[13];
        to[13] = from[15] << 2 | from[14];
        to[14] = from[16];
        to[15] = from[18] << 1 | from[17];
        to[16] = from[19];
    }
    // The voice's own parameters, 126 to 154, are 0 to 28 of `voice`.
    packed[102..111].copy_from_slice(&voice[..9]);
    packed[111] = voice[10] << 3 | voice[9];
    packed[112..116].copy_from_slice(&voice[11..15]);
    packed[116] = voice[17] << 4 | voice[16] << 1 | voice[15];
    packed[117..].copy_from_slice(&voice[18..]);
    packed
}

/// The 155 bytes of a single-voice dump of a packed voice's 128, which
/// [`pack`] lays out. A byte with unused bits set is a finding, and the bits
/// are ignored or the dump refused; a field wider than its range, as the
/// detune's four bits are for 0 to 14, may give a step beyond it, which
/// [`voice_of`] finds.
fn unpack(
    packed: &[u8; PACKED_BYTES],
    place: u8,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<[u8; VOICE_BYTES], Error> {
    for (byte, (&value, &used)) in (0..).zip(packed.iter().zip(&USED)) {
        let bits = value & !used;
        if bits != 0 {
            let finding = Finding::UnusedBits {
                voice: place,
                byte,
                bits,
            };
            if options.out_of_range == OutOfRange::Reject {
                return Err(Error::Refused(finding));
            }
            findings.push(finding);
        }
    }
    let mut steps = [0; VOICE_BYTES];
    let (operators, voice) = steps.split_at_mut(OPERATOR_STEPS);
    let (operators, _) = operators.as_chunks_mut::<21>();
    let (packed_operators, _) = packed.as_chunks::<17>();
    for (from, to) in packed_operators.iter().zip(operators) {
        to[..11].copy_from_slice(&from[..11]);
        to[11] = from[11] & 0x03;
        to[12] = from[11] >> 2 & 0x03;
        to[13] = from[12] & 0x07;
        to[20] = from[12] >> 3;
        to[14] = from[13] & 0x03;
        to[15] = from[13] >> 2 & 0x07;
        to[16] = from[14];
        to[17] = from[15] & 0x01;
        to[18] = from[15] >> 1 & 0x1f;
        to[19] = from[16];
    }
    voice[..9].copy_from_slice(&packed[102..111]);
    voice[8] = packed[110] & 0x1f;
    voice[9] = packed[111] & 0x07;
    voice[10] = packed[111] >> 3 & 0x01;
    voice[11..15].copy_from_slice(&packed[112..116]);
    voice[15] = packed[116] & 0x01;
    voice[16] = packed[116] >> 1 & 0x07;
    voice[17] = packed[116] >> 4;
    voice[18..].copy_from_slice(&packed[117..]);
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::{BANK_BYTES, OPERATOR_STEPS, PACKED_BYTES, USED, VOICE_BYTES, checksum_of};
    use crate::voice::Parameter;

    #[test]
    fn sizes() {
        assert_eq!(VOICE_BYTES, usize::from(Parameter::IN_DUMP));
        assert_eq!(OPERATOR_STEPS, 6 * 21);
        assert_eq!(BANK_BYTES, 4096);
        assert_eq!(PACKED_BYTES * 32, BANK_BYTES);
    }

    #[test]
    fn unused_bits() {
        let unused: Vec<(usize, u8)> = USED
            .iter()
            .enumerate()
            .filter(|&(_, &used)| used != 0x7f)
            .map(|(byte, &used)| (byte, used))
            .collect();
        let mut expected = Vec::new();
        for operator in 0..6 {
            expected.extend([
                (17 * operator + 11, 0x0f),
                (17 * operator + 13, 0x1f),
                (17 * operator + 15, 0x3f),
            ]);
        }
        expected.extend([(110, 0x1f), (111, 0x0f)]);
        assert_eq!(unused, expected);
    }

    #[test]
    fn checksums() {
        assert_eq!(checksum_of(&[]), 0);
        assert_eq!(checksum_of(&[1]), 0x7f);
        assert_eq!(checksum_of(&[0x7f, 0x7f]), 0x02);
        assert_eq!(checksum_of(&[0x40; 4]), 0);
    }
}
