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
//!   performances packed into 64 bytes each; the TX7's condition acknowledge,
//!   which says what it is and how it is set; the DX7II's universal bulk dumps
//!   of its performances, its system set-up, its micro tunings and its
//!   fractional scalings, a cartridge's in as many groups as it holds, and the
//!   DX7s's of its system set-up ([`crate::dx7ii`]); and the TX802's of its
//!   performance edit buffer, of the 64 performances of its memory, a group
//!   each, its performances in hexadecimal digits, and of its system set-up
//!   ([`crate::tx802`]);
//! - the parameter changes: of a voice parameter, of a function parameter, of a
//!   button of the DX7's panel, which its firmware takes as pressed or released
//!   \[AJXS-ROM\], of a switch of the DX7II's panel by its switch remote, of a
//!   parameter of its supplement, its performance and its system set-up, of a
//!   key of its micro tuning and of a parameter of its fractional scaling,
//!   with three bytes of data and four, and the DX7s's system reset; the
//!   TX802's, of a parameter of its performance and of a switch of its panel;
//!   and the TX7's, of a performance's parameter, of one of its own settings and
//!   of the operator its panel edits;
//! - the dump requests of the DX7II and the TX7, which the DX7 itself does not
//!   answer, and Keptone does (docs/DESIGN.md §11.2).
//!
//! [`read`] finds every message in a file, not only the first: a file may hold
//! several, one after another, with bytes that belong to no message before,
//! between and after them, and a message may be cut short (docs/DESIGN.md
//! §11.2). It gives each message decoded by [`decode`], or the reason it is
//! not. A dump's checksum is checked: a wrong one is reported, and the dump
//! decoded all the same only if `syx.accept_bad_checksum` allows it. Every byte
//! is checked against the range of its parameter, a packed byte for bits its
//! format leaves unused, a byte of a fractional scaling's dump for one of the
//! sixteen that send four bits, a byte of the TX802's performances for a
//! hexadecimal digit, and a switch's data for 0 or 127: what is not as it
//! should be is reported, and either taken as the nearer end of the range, the
//! bits ignored, the digit read as it most likely stands and the switch
//! pressed, or the message refused, as `syx.out_of_range` says (docs/MATH.md
//! §17). A message is decoded whole or refused whole.
//!
//! [`write_voice`], [`write_bank`], [`write_supplement`], [`write_supplements`],
//! [`write_performance`], [`write_performances`], [`write_condition`],
//! [`write_dx7ii_performance`], [`write_dx7ii_performances`],
//! [`write_system_setup`], [`write_dx7s_system_setup`],
//! [`write_micro_tuning`], [`write_micro_tuning_memory`],
//! [`write_micro_tunings`], [`write_fractional_scaling`],
//! [`write_fractional_scalings`], [`write_tx802_performance`],
//! [`write_tx802_performances`], [`write_tx802_system_setup`], [`write_change`]
//! and [`write_request`] write the messages, each value rounded to a step as
//! `syx.export_rounding` says; the dumps say what they could not hold of what
//! they were written from. A message that is valid -- its checksum right, every
//! byte in range, no unused bit set, every digit a hexadecimal one and a
//! switch's data 0 or 127 -- comes back from them byte for byte as it was read
//! (MATH §16), but for a performance under format 2, which they write under 1.
//!
//! Every function here takes any bytes and gives a value or an error, never a
//! panic, and allocates in proportion to the bytes it is given: [`read`] goes
//! through a file a message at a time, and a dump decoded takes no more than
//! ten times the bytes of its message, 1056 for each voice, 280 for each
//! performance, 344 for each supplement, 1024 for each micro tuning, 1968
//! for each fractional scaling and 344 for each of the TX802's performances on
//! a 64-bit system.

use std::fmt;

use crate::dx7ii::{
    self, FractionalScaling, Index, KeyPitch, MicroTuning, ScalingParameter, SystemParameter, SystemSetup,
};
use crate::function::{Function, FunctionParameter};
use crate::performance::{DxPerformanceParameter, KeyAssign, Performance, PerformanceParameter, Unnamed};
use crate::supplement::{Supplement, SupplementParameter};
use crate::tx802;
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

/// The number of the switch remote's message that resets the DX7s, as if it
/// were just switched on \[DX7S-MANUAL\].
const SYSTEM_RESET: u8 = 46;

/// The classification name of the DX7II's universal bulk dumps \[DX7II-MANUAL\].
const LM: [u8; 4] = *b"LM  ";

/// The data format name of the DX7II's performance edit buffer.
const PERFORMANCE_EDIT: [u8; 6] = *b"8973PE";

/// The data format name of the DX7II's 32 performances.
const PERFORMANCE_MEMORY: [u8; 6] = *b"8973PM";

/// The data format name of the DX7II's system set-up.
const SYSTEM_SETUP: [u8; 6] = *b"8973S ";

/// The bytes of one DX7II performance.
const DX7II_PERFORMANCE_BYTES: usize = 51;

/// The bytes of 32 DX7II performances, one after another.
const DX7II_PERFORMANCES_BYTES: usize = 32 * DX7II_PERFORMANCE_BYTES;

/// The bytes of the DX7II's system set-up.
const SYSTEM_SETUP_BYTES: usize = 102;

/// The bytes of the DX7s's system set-up, which has no place for the 17 of the
/// DX7II's that have no name \[DX7S-MANUAL\] p. Add-4.
const DX7S_SYSTEM_SETUP_BYTES: usize = 85;

/// The data format name of the DX7II's micro tuning edit buffer.
const MICRO_TUNING_EDIT: [u8; 6] = *b"MCRYE ";

/// The first five bytes of the data format name of one of the two micro
/// tunings in the DX7II's memory, whose sixth is the memory's number, 0 or 1,
/// as a byte rather than a letter \[DX7II-MANUAL\] p. Add-4.
const MICRO_TUNING_MEMORY: [u8; 5] = *b"MCRYM";

/// The data format name of a cartridge's micro tunings, one a group.
const MICRO_TUNING_CARTRIDGE: [u8; 6] = *b"MCRYC ";

/// The data format name of the DX7II's fractional scaling edit buffer.
const SCALING_EDIT: [u8; 6] = *b"FKSYE ";

/// The data format name of a cartridge's fractional scalings, one a group.
const SCALING_CARTRIDGE: [u8; 6] = *b"FKSYC ";

/// The bytes of a micro tuning: two for each of the 128 keys, the step's high
/// seven bits and its low seven \[DX7II-MANUAL\] p. Add-14.
const MICRO_TUNING_BYTES: usize = 256;

/// The bytes of a fractional scaling: two for each of its 246 parameters, the
/// step's high four bits and then its low four, each sent as the byte 0x30
/// above them \[DX7S-MANUAL\] p. Add-4, as dumps of DX7II banks bear out
/// \[DX7II-DUMPS\].
const SCALING_BYTES: usize = 492;

/// The byte that sends four bits of 0 in a fractional scaling's dump; the
/// fifteen after it send 1 to 15.
const NIBBLE_ZERO: u8 = 0x30;

/// The most groups that `decode` takes of a dump of several: 64, as many micro
/// tunings as the DX7II's cartridge holds, the DX7s's holding 63, as many
/// fractional scalings as the DX7s's, whose dump sends 32, and as many
/// performances as the TX802's memory \[DX7II-MANUAL\] \[DX7S-MANUAL\]
/// \[TX802-SYSEX\].
const MOST_GROUPS: usize = 64;

/// The group byte and the number of the parameter change of a key of the micro
/// tuning, the supplement's group with the number 126 \[DX7II-MANUAL\] p. Add-2.
const MICRO_TUNING_CHANGE: [u8; 2] = [DX7II_GROUP << 2, 126];

/// The group byte and the number of the parameter change of a fractional
/// scaling, the supplement's group with the number 127.
const SCALING_CHANGE: [u8; 2] = [DX7II_GROUP << 2, 127];

/// The data format name of the TX802's performance edit buffer
/// \[TX802-SYSEX\].
const TX802_PERFORMANCE_EDIT: [u8; 6] = *b"8952PE";

/// The data format name of the 64 performances of the TX802's memory, one a
/// group.
const TX802_PERFORMANCE_MEMORY: [u8; 6] = *b"8952PM";

/// The data format name of the TX802's system set-up.
const TX802_SYSTEM_SETUP: [u8; 6] = *b"8952S ";

/// The bytes of the TX802's performance edit buffer: two hexadecimal digits
/// for each of its 116 parameters.
const TX802_PERFORMANCE_BYTES: usize = 232;

/// The bytes of a performance of the TX802's memory: two hexadecimal digits for
/// each of its 84 packed bytes.
const TX802_PACKED_BYTES: usize = 168;

/// The bytes of the TX802's system set-up.
const TX802_SYSTEM_BYTES: usize = 263;

/// The group byte of the parameter changes of the TX802's performance, group 6
/// with its low bits 2 \[TX802-SYSEX\].
const TX802_PERFORMANCE_GROUP: u8 = DX7II_GROUP << 2 | 2;

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
    /// A universal bulk dump's group whose checksum is not the one its names
    /// and data have.
    UniversalChecksum {
        /// The place of the group in the message, from 0.
        group: u8,
        /// The checksum its names and data have.
        expected: u8,
        /// The checksum it ends with.
        found: u8,
    },
    /// A step of a DX7II performance's parameter beyond its range
    /// ([`dx7ii::PerformanceParameter::largest`]).
    Dx7iiPerformanceOutOfRange {
        /// The place of the performance: 0 in the edit buffer or a parameter
        /// change, 0 to 31 in the dump of 32.
        performance: u8,
        /// The parameter.
        parameter: dx7ii::PerformanceParameter,
        /// The step it held.
        step: u8,
    },
    /// A step of a parameter of the DX7II's system set-up outside its range
    /// ([`SystemParameter::smallest`] to its largest).
    SystemOutOfRange {
        /// The parameter.
        parameter: SystemParameter,
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
    /// A step of a key of a micro tuning beyond 10794 ([`KeyPitch::LARGEST`]).
    MicroTuningOutOfRange {
        /// The place of the micro tuning: 0 in the dump of the edit buffer or
        /// of a memory or in a parameter change, 0 to 63 in a cartridge's.
        tuning: u8,
        /// The key.
        key: Index<127>,
        /// The step it held.
        step: u16,
    },
    /// A byte of the dump of a fractional scaling that is not one of the
    /// sixteen that send four bits, 0x30 to 0x3F \[DX7S-MANUAL\] p. Add-4.
    ScalingByte {
        /// The place of the fractional scaling: 0 in the dump of the edit
        /// buffer, 0 to 63 in a cartridge's.
        scaling: u8,
        /// The parameter that the byte sends half of.
        parameter: ScalingParameter,
        /// The byte.
        byte: u8,
    },
    /// A step of a fractional scaling's parameter beyond 255 in a parameter
    /// change, its high byte more than 1.
    ScalingOutOfRange {
        /// The parameter.
        parameter: ScalingParameter,
        /// The step it held.
        step: u16,
    },
    /// A step of a parameter of the TX802's performance beyond its range
    /// ([`tx802::PerformanceParameter::largest`]).
    Tx802PerformanceOutOfRange {
        /// The place of the performance: 0 in the edit buffer or a parameter
        /// change, 0 to 63 in the dump of the memory.
        performance: u8,
        /// The parameter.
        parameter: tx802::PerformanceParameter,
        /// The step it held, of two bytes in a parameter change of the voice or
        /// the micro tuning table.
        step: u16,
    },
    /// A byte of the TX802's performances that is not one of the sixteen
    /// hexadecimal digits, '0' to '9' and 'A' to 'F' \[TX802-SYSEX\].
    Tx802Digit {
        /// The place of the performance.
        performance: u8,
        /// The place of the byte whose half it sends, among the edit buffer's
        /// 116 or a packed performance's 84.
        value: u8,
        /// The byte.
        byte: u8,
    },
    /// A byte of a packed performance of the TX802's memory that sets its high
    /// bit, which the format leaves unused.
    Tx802UnusedBits {
        /// The place of the performance, 0 to 63.
        performance: u8,
        /// The place of the byte among the performance's 84.
        byte: u8,
        /// The bits it set that are unused.
        bits: u8,
    },
    /// A step of a parameter of the TX802's system set-up that the parameter
    /// does not take.
    Tx802SystemOutOfRange {
        /// The parameter.
        parameter: tx802::SystemParameter,
        /// The step it held.
        step: u8,
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
            _ => self.fmt_family(formatter),
        }
    }
}

impl Finding {
    /// The description of a finding of a message of the DX7's family that the
    /// DX7 does not read: the TX7's, the DX1's and the TX802's, the DX7II's
    /// apart.
    fn fmt_family(self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
            }
            | Self::Tx802UnusedBits {
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
            Self::Tx802PerformanceOutOfRange {
                performance,
                parameter,
                step,
            } => write!(
                formatter,
                "performance {}: {parameter} is {step}, beyond its largest, {}",
                u16::from(performance) + 1,
                parameter.largest()
            ),
            Self::Tx802Digit {
                performance,
                value,
                byte,
            } => write!(
                formatter,
                "performance {}: byte {value} is sent with {byte:#04x}, not a hexadecimal digit",
                u16::from(performance) + 1
            ),
            Self::Tx802SystemOutOfRange { parameter, step } => {
                write!(formatter, "{parameter} is {step}, which it does not take")
            }
            Self::Checksum { .. }
            | Self::OutOfRange { .. }
            | Self::UnusedBits { .. }
            | Self::FunctionOutOfRange { .. }
            | Self::SwitchData { .. } => fmt::Display::fmt(&self, formatter),
            _ => self.fmt_dx7ii(formatter),
        }
    }

    /// The description of a finding of a message of the DX7II and the DX7s.
    fn fmt_dx7ii(self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
            Self::UniversalChecksum { group, expected, found } => write!(
                formatter,
                "group {}: the checksum is {found:#04x}, where the names and the data have {expected:#04x}",
                u16::from(group) + 1
            ),
            Self::Dx7iiPerformanceOutOfRange {
                performance,
                parameter,
                step,
            } => write!(
                formatter,
                "performance {}: {parameter} is {step}, beyond its largest, {}",
                u16::from(performance) + 1,
                parameter.largest()
            ),
            Self::SystemOutOfRange { parameter, step } => write!(
                formatter,
                "{parameter} is {step}, outside its range, {} to {}",
                parameter.smallest(),
                parameter.largest()
            ),
            Self::SupplementUnusedBits { supplement, byte, bits } => write!(
                formatter,
                "supplement {}: byte {byte} sets unused bits {bits:#04x}",
                u16::from(supplement) + 1
            ),
            Self::MicroTuningOutOfRange { tuning, key, step } => write!(
                formatter,
                "micro tuning {}: key {} is {step}, beyond its largest, {}",
                u16::from(tuning) + 1,
                key.get(),
                KeyPitch::LARGEST
            ),
            Self::ScalingByte {
                scaling,
                parameter,
                byte,
            } => write!(
                formatter,
                "fractional scaling {}: {parameter} is sent as the byte {byte:#04x}, not one of 0x30 to 0x3f",
                u16::from(scaling) + 1
            ),
            Self::ScalingOutOfRange { parameter, step } => {
                write!(formatter, "{parameter} is {step}, beyond its largest, 255")
            }
            Self::Checksum { .. }
            | Self::OutOfRange { .. }
            | Self::UnusedBits { .. }
            | Self::FunctionOutOfRange { .. }
            | Self::SwitchData { .. }
            | Self::PerformanceOutOfRange { .. }
            | Self::KeyAssignOutOfRange { .. }
            | Self::PerformanceUnusedBits { .. }
            | Self::PerformanceFormat
            | Self::DxPerformanceOutOfRange { .. }
            | Self::TxFunctionOutOfRange { .. }
            | Self::OperatorSelectOutOfRange { .. }
            | Self::ConditionChannel { .. }
            | Self::Tx802PerformanceOutOfRange { .. }
            | Self::Tx802Digit { .. }
            | Self::Tx802UnusedBits { .. }
            | Self::Tx802SystemOutOfRange { .. } => fmt::Display::fmt(&self, formatter),
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
    /// that the DX7, the DX7II and the TX7 number; or a universal bulk dump of
    /// groups of different names, or of more groups than its kind holds, one
    /// but for a cartridge's 64.
    Unsupported,
    /// A message whose byte count or length is not its kind's: a single-voice
    /// dump holds 155 bytes of data in a message of 163, a 32-voice dump 4096
    /// in 4104, the dump of one supplement 49 in 57, the dump of 32 1120 in
    /// 1128, the dump of one performance 94 in 102, the dump of 64 4096 in
    /// 4104, the condition acknowledge 16 in 24, a universal bulk dump's group
    /// the 10 bytes of its names and the data its names give, a DX7II
    /// performance 51 bytes, 32 of them 1632, its system set-up 102, the
    /// DX7s's 85, a micro tuning 256, a fractional scaling 492, the TX802's
    /// performance 232, a packed one 168 and its system set-up 263, a
    /// parameter change one byte of data in 7, two in 8 for the TX802's voice
    /// and micro tuning table, three in 9 for a key of a micro tuning and four
    /// in 10 for a fractional scaling, and a dump request is of 5 bytes, or 15
    /// for a universal bulk dump's.
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
    /// The DX7II's universal bulk dump of its performance edit buffer.
    Dx7iiPerformance(Box<dx7ii::Performance>),
    /// The DX7II's universal bulk dump of its 32 performances, in the order of
    /// their places.
    Dx7iiPerformances(Box<[dx7ii::Performance; 32]>),
    /// The DX7II's universal bulk dump of its system set-up.
    SystemSetup(Box<SystemSetup>),
    /// The DX7s's universal bulk dump of its system set-up, of 85 bytes, whose
    /// bytes that have no name, which the dump does not hold, are 0.
    Dx7sSystemSetup(Box<SystemSetup>),
    /// The DX7II's universal bulk dump of its micro tuning edit buffer.
    MicroTuning(Box<MicroTuning>),
    /// The DX7II's universal bulk dump of one of the two micro tunings in its
    /// memory.
    MicroTuningMemory {
        /// The memory's number, 0 or 1.
        memory: Index<1>,
        /// The micro tuning.
        tuning: Box<MicroTuning>,
    },
    /// The universal bulk dump of the micro tunings of a cartridge, a group
    /// each, 64 from the DX7II and 63 from the DX7s, in the order of their
    /// places.
    MicroTunings(Vec<MicroTuning>),
    /// The DX7II's universal bulk dump of its fractional scaling edit buffer.
    FractionalScaling(Box<FractionalScaling>),
    /// The universal bulk dump of the fractional scalings of a cartridge, a
    /// group each, 32 from the DX7II, in the order of their places.
    FractionalScalings(Vec<FractionalScaling>),
    /// The TX802's universal bulk dump of its performance edit buffer.
    Tx802Performance(Box<tx802::Performance>),
    /// The TX802's universal bulk dump of the 64 performances of its memory, a
    /// group each, in the order of their places.
    Tx802Performances(Vec<tx802::Performance>),
    /// The TX802's universal bulk dump of its system set-up.
    Tx802SystemSetup(Box<tx802::SystemSetup>),
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
/// it comes, to be read as the instrument that sent it numbers them, where
/// that is known (docs/DESIGN.md §6.4).
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

/// A parameter of the DX7II's performance and a step within its range: what a
/// parameter change of the performance sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Dx7iiPerformanceStep {
    /// The parameter.
    parameter: dx7ii::PerformanceParameter,
    /// The step, no more than the parameter's largest.
    step: u8,
}

impl Dx7iiPerformanceStep {
    /// The parameter and step, if the step is within the parameter's range.
    #[must_use]
    pub fn new(parameter: dx7ii::PerformanceParameter, step: u8) -> Option<Self> {
        (step <= parameter.largest()).then_some(Self { parameter, step })
    }

    /// The parameter.
    #[must_use]
    pub const fn parameter(self) -> dx7ii::PerformanceParameter {
        self.parameter
    }

    /// The step.
    #[must_use]
    pub const fn step(self) -> u8 {
        self.step
    }
}

/// A parameter of the DX7II's system set-up that a parameter change sets, its
/// bytes 0 to 19, and a step within its range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SystemStep {
    /// The parameter, of a byte below 20.
    parameter: SystemParameter,
    /// The step, from the parameter's smallest to its largest.
    step: u8,
}

impl SystemStep {
    /// The parameter and step, if a parameter change sets the parameter and the
    /// step is within its range.
    #[must_use]
    pub fn new(parameter: SystemParameter, step: u8) -> Option<Self> {
        (parameter.change_number().is_some() && (parameter.smallest()..=parameter.largest()).contains(&step))
            .then_some(Self { parameter, step })
    }

    /// The parameter.
    #[must_use]
    pub const fn parameter(self) -> SystemParameter {
        self.parameter
    }

    /// The step.
    #[must_use]
    pub const fn step(self) -> u8 {
        self.step
    }
}

/// A key of a micro tuning and a step of its pitch, no more than 10794: what a
/// parameter change of the micro tuning sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MicroTuningStep {
    /// The key.
    key: Index<127>,
    /// The step, no more than 10794.
    step: u16,
}

impl MicroTuningStep {
    /// The key and step, if the step is no more than 10794.
    #[must_use]
    pub fn new(key: Index<127>, step: u16) -> Option<Self> {
        (step <= KeyPitch::LARGEST).then_some(Self { key, step })
    }

    /// A key of a micro tuning and its step, its pitch rounded by `rounding`.
    #[must_use]
    pub fn of(tuning: &MicroTuning, key: Index<127>, rounding: Rounding) -> Self {
        Self {
            key,
            step: tuning.keys[usize::from(key.get())]
                .step(rounding)
                .min(KeyPitch::LARGEST),
        }
    }

    /// The key.
    #[must_use]
    pub const fn key(self) -> Index<127> {
        self.key
    }

    /// The step.
    #[must_use]
    pub const fn step(self) -> u16 {
        self.step
    }
}

/// A parameter of a fractional scaling and a step, which every byte is: what a
/// parameter change of the fractional scaling sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScalingStep {
    /// The parameter.
    parameter: ScalingParameter,
    /// The step.
    step: u8,
}

impl ScalingStep {
    /// The parameter and step.
    #[must_use]
    pub const fn new(parameter: ScalingParameter, step: u8) -> Self {
        Self { parameter, step }
    }

    /// A parameter of a fractional scaling and its step, its value rounded by
    /// `rounding`.
    #[must_use]
    pub fn of(scaling: &FractionalScaling, parameter: ScalingParameter, rounding: Rounding) -> Self {
        Self {
            parameter,
            step: scaling.step(parameter, rounding),
        }
    }

    /// The parameter.
    #[must_use]
    pub const fn parameter(self) -> ScalingParameter {
        self.parameter
    }

    /// The step.
    #[must_use]
    pub const fn step(self) -> u8 {
        self.step
    }
}

/// A parameter of the TX802's performance and a step within its range: what a
/// parameter change of the performance sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tx802PerformanceStep {
    /// The parameter.
    parameter: tx802::PerformanceParameter,
    /// The step, no more than the parameter's largest, and below 128 but for
    /// the voice and the micro tuning table.
    step: u8,
}

impl Tx802PerformanceStep {
    /// The parameter and step, if the step is within the parameter's range and
    /// a parameter change carries it: below 128, but for the voice and the
    /// micro tuning table, whose changes carry two bytes of data.
    #[must_use]
    pub fn new(parameter: tx802::PerformanceParameter, step: u8) -> Option<Self> {
        (step <= parameter.largest() && (step < 0x80 || parameter.is_wide())).then_some(Self { parameter, step })
    }

    /// A parameter of a performance and its step, its value rounded by
    /// `rounding`, and no more than a parameter change carries.
    #[must_use]
    pub fn of(performance: &tx802::Performance, parameter: tx802::PerformanceParameter, rounding: Rounding) -> Self {
        let most = if parameter.is_wide() { parameter.largest() } else { 0x7f };
        Self {
            parameter,
            step: performance.step(parameter, rounding).min(most),
        }
    }

    /// The parameter.
    #[must_use]
    pub const fn parameter(self) -> tx802::PerformanceParameter {
        self.parameter
    }

    /// The step.
    #[must_use]
    pub const fn step(self) -> u8 {
        self.step
    }
}

/// A switch of the TX802's panel, by the number its switch remote gives it,
/// 64 to 96 \[TX802-SYSEX\] Table 4: from 64, power on, to 96, the eighth
/// tone generator on and off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tx802Switch(u8);

impl Tx802Switch {
    /// The switch of a number, if the number is 64 to 96.
    #[must_use]
    pub fn new(number: u8) -> Option<Self> {
        (64..=96).contains(&number).then_some(Self(number))
    }

    /// Its number.
    #[must_use]
    pub const fn number(self) -> u8 {
        self.0
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
    /// A parameter of the DX7II's performance, of group 6 with the group byte's
    /// low bits 1 and numbers 0 to 50.
    Dx7iiPerformance(Dx7iiPerformanceStep),
    /// A parameter of the DX7II's system set-up, of group 6 with the group
    /// byte's low bits 1 and numbers 64 to 83.
    System(SystemStep),
    /// The DX7s's system reset, which resets it as if it were just switched on,
    /// by the switch remote's number 46 \[DX7S-MANUAL\]: the data, which the
    /// DX7s ignores, kept as it comes.
    SystemReset(Byte),
    /// A key of the DX7II's micro tuning, of group 6 with the group byte's low
    /// bits 0 and the number 126, whose three bytes of data are the key and
    /// the step's high seven bits and its low seven \[DX7II-MANUAL\] p. Add-2.
    MicroTuning(MicroTuningStep),
    /// A parameter of the DX7II's fractional scaling, of the number 127, whose
    /// four bytes of data are the operator's number, 0 for OP6 to 5 for OP1,
    /// the group's, 0 for the offset, and the step's high bit and its low seven
    /// \[DX7S-MANUAL\] p. Add-3.
    FractionalScaling(ScalingStep),
    /// A parameter of the TX802's performance, of group 6 with the group byte's
    /// low bits 2 and numbers 0 to 115, the voice's and the micro tuning
    /// table's changes with two bytes of data, the step's high seven bits and
    /// its low seven \[TX802-SYSEX\] Table 3.
    Tx802Performance(Tx802PerformanceStep),
    /// A switch of the TX802's panel by its switch remote, the group byte 0x1B
    /// with numbers 64 to 96: the data, which the TX802 does not look at, kept
    /// as it comes \[TX802-SYSEX\] Table 4.
    Tx802Switch(Tx802Switch, Byte),
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
    if format == UNIVERSAL_FORMAT {
        return universal_of(channel, rest, options);
    }
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

/// A group of a universal bulk dump: its byte count; the 10 bytes of its
/// classification and data format names; its data, which the count counts with
/// the names, 61 for the 51 bytes of the DX7II's performance \[DX7II-MANUAL\]
/// p. Add-4, but alone for the TX802's performances \[TX802-SYSEX\] Fig. 1;
/// and the checksum of the names and the data \[DX7II-DUMPS\]
/// \[TX802-DUMPS\].
struct Group<'a> {
    /// The names.
    names: [u8; 10],
    /// The names and the data, which the checksum is of.
    counted: &'a [u8],
    /// The data.
    data: &'a [u8],
    /// The checksum it ends with.
    checksum: u8,
}

/// The groups of a universal bulk dump's bytes after its format, as many as
/// the message holds.
fn groups_of(mut rest: &[u8]) -> Result<Vec<Group<'_>>, Error> {
    let mut groups = Vec::new();
    while !rest.is_empty() {
        let &[high, low, ref after @ ..] = rest else {
            return Err(Error::Length);
        };
        let count = usize::from(high) << 7 | usize::from(low);
        let counted_length = match after.first_chunk::<10>() {
            Some(&names) if counts_data_alone(names) => count + 10,
            _ => count,
        };
        let (Some(counted), Some((&checksum, after))) = (
            after.get(..counted_length),
            after.get(counted_length..).and_then(<[u8]>::split_first),
        ) else {
            return Err(Error::Length);
        };
        let Some((&names, data)) = counted.split_first_chunk::<10>() else {
            return Err(Error::Length);
        };
        groups.push(Group {
            names,
            counted,
            data,
            checksum,
        });
        rest = after;
    }
    Ok(groups)
}

/// Whether a universal bulk dump's group of these names counts its data alone:
/// the TX802's performances' do, where every other's counts the names as well
/// \[TX802-SYSEX\] Fig. 1.
fn counts_data_alone(names: [u8; 10]) -> bool {
    let [c0, c1, c2, c3, f0, f1, f2, f3, f4, f5] = names;
    [c0, c1, c2, c3] == LM
        && matches!(
            [f0, f1, f2, f3, f4, f5],
            TX802_PERFORMANCE_EDIT | TX802_PERFORMANCE_MEMORY
        )
}

/// The dump of a universal bulk dump's bytes after its format: one group of a
/// performance edit buffer, 32 performances, a system set-up, a micro tuning,
/// the edit buffer's or one of the memory's, or a fractional scaling of the
/// DX7II, of a system set-up of the DX7s, or of the TX802's performance edit
/// buffer or system set-up; or a cartridge's micro tunings or fractional
/// scalings, or the TX802's performances, a group each, as many as 64.
fn universal_of(channel: Channel, rest: &[u8], options: Options) -> Result<Dump, Error> {
    let groups = groups_of(rest)?;
    let Some(first) = groups.first() else {
        return Err(Error::Length);
    };
    let kind = universal_kind(first.names)?;
    let most = match kind {
        UniversalKind::MicroTunings | UniversalKind::Scalings | UniversalKind::Tx802Performances => MOST_GROUPS,
        _ => 1,
    };
    if groups.len() > most || groups.iter().any(|group| group.names != first.names) {
        return Err(Error::Unsupported);
    }
    let mut findings = Vec::new();
    for (place, group) in (0..).zip(&groups) {
        let expected = checksum_of(group.counted);
        if group.checksum != expected {
            let finding = Finding::UniversalChecksum {
                group: place,
                expected,
                found: group.checksum,
            };
            if !options.accept_bad_checksum {
                return Err(Error::Refused(finding));
            }
            findings.push(finding);
        }
    }
    let content = universal_content(kind, first.data, &groups, options, &mut findings)?;
    Ok(Dump {
        channel,
        content,
        findings,
    })
}

/// What a universal bulk dump holds, by its names.
#[derive(Debug, Clone, Copy)]
enum UniversalKind {
    Performance,
    Performances,
    SystemSetup,
    MicroTuning,
    MicroTuningMemory(Index<1>),
    MicroTunings,
    Scaling,
    Scalings,
    Tx802Performance,
    Tx802Performances,
    Tx802SystemSetup,
}

/// What a universal bulk dump's group holds, by its names, if it is of a kind
/// this reader takes.
fn universal_kind(names: [u8; 10]) -> Result<UniversalKind, Error> {
    let [c0, c1, c2, c3, f0, f1, f2, f3, f4, f5] = names;
    if [c0, c1, c2, c3] != LM {
        return Err(Error::Unsupported);
    }
    Ok(match [f0, f1, f2, f3, f4, f5] {
        PERFORMANCE_EDIT => UniversalKind::Performance,
        PERFORMANCE_MEMORY => UniversalKind::Performances,
        SYSTEM_SETUP => UniversalKind::SystemSetup,
        MICRO_TUNING_EDIT => UniversalKind::MicroTuning,
        MICRO_TUNING_CARTRIDGE => UniversalKind::MicroTunings,
        SCALING_EDIT => UniversalKind::Scaling,
        SCALING_CARTRIDGE => UniversalKind::Scalings,
        TX802_PERFORMANCE_EDIT => UniversalKind::Tx802Performance,
        TX802_PERFORMANCE_MEMORY => UniversalKind::Tx802Performances,
        TX802_SYSTEM_SETUP => UniversalKind::Tx802SystemSetup,
        [m0, m1, m2, m3, m4, memory] if [m0, m1, m2, m3, m4] == MICRO_TUNING_MEMORY => {
            UniversalKind::MicroTuningMemory(Index::new(memory).ok_or(Error::Unsupported)?)
        }
        _ => return Err(Error::Unsupported),
    })
}

/// What a universal bulk dump of a kind holds, of the data of its first group
/// and of its groups, each byte checked as the options say.
fn universal_content(
    kind: UniversalKind,
    data: &[u8],
    groups: &[Group<'_>],
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<Content, Error> {
    Ok(match kind {
        UniversalKind::Performance => {
            let Ok(steps) = <&[u8; DX7II_PERFORMANCE_BYTES]>::try_from(data) else {
                return Err(Error::Length);
            };
            Content::Dx7iiPerformance(Box::new(dx7ii_performance_of(steps, 0, options, findings)?))
        }
        UniversalKind::Performances => {
            let Ok(data) = <&[u8; DX7II_PERFORMANCES_BYTES]>::try_from(data) else {
                return Err(Error::Length);
            };
            let (steps, _) = data.as_chunks::<DX7II_PERFORMANCE_BYTES>();
            let mut performances = Vec::with_capacity(32);
            for (place, steps) in (0..).zip(steps) {
                performances.push(dx7ii_performance_of(steps, place, options, findings)?);
            }
            let Ok(performances) = <Box<[dx7ii::Performance; 32]>>::try_from(performances.into_boxed_slice()) else {
                return Err(Error::Length);
            };
            Content::Dx7iiPerformances(performances)
        }
        UniversalKind::SystemSetup => match data.len() {
            SYSTEM_SETUP_BYTES => {
                let setup = system_setup_of(data, SystemParameter::all(), options, findings)?;
                Content::SystemSetup(Box::new(setup))
            }
            DX7S_SYSTEM_SETUP_BYTES => {
                let setup = system_setup_of(data, SystemParameter::in_dx7s_dump(), options, findings)?;
                Content::Dx7sSystemSetup(Box::new(setup))
            }
            _ => return Err(Error::Length),
        },
        UniversalKind::MicroTuning => Content::MicroTuning(Box::new(micro_tuning_of(data, 0, options, findings)?)),
        UniversalKind::MicroTuningMemory(memory) => Content::MicroTuningMemory {
            memory,
            tuning: Box::new(micro_tuning_of(data, 0, options, findings)?),
        },
        UniversalKind::MicroTunings => {
            let mut tunings = Vec::with_capacity(groups.len());
            for (place, group) in (0..).zip(groups) {
                tunings.push(micro_tuning_of(group.data, place, options, findings)?);
            }
            Content::MicroTunings(tunings)
        }
        UniversalKind::Scaling => Content::FractionalScaling(Box::new(scaling_of(data, 0, options, findings)?)),
        UniversalKind::Scalings => {
            let mut scalings = Vec::with_capacity(groups.len());
            for (place, group) in (0..).zip(groups) {
                scalings.push(scaling_of(group.data, place, options, findings)?);
            }
            Content::FractionalScalings(scalings)
        }
        UniversalKind::Tx802Performance => Content::Tx802Performance(Box::new(tx802_edit_of(data, options, findings)?)),
        UniversalKind::Tx802Performances => {
            let mut performances = Vec::with_capacity(groups.len());
            for (place, group) in (0..).zip(groups) {
                performances.push(tx802_packed_of(group.data, place, options, findings)?);
            }
            Content::Tx802Performances(performances)
        }
        UniversalKind::Tx802SystemSetup => {
            Content::Tx802SystemSetup(Box::new(tx802_system_of(data, options, findings)?))
        }
    })
}

/// The TX802's performance of its edit buffer's data, two hexadecimal digits
/// for each of its 116 parameters, each step checked against its range.
fn tx802_edit_of(data: &[u8], options: Options, findings: &mut Vec<Finding>) -> Result<tx802::Performance, Error> {
    if data.len() != TX802_PERFORMANCE_BYTES {
        return Err(Error::Length);
    }
    let steps = bytes_of_digits::<116>(data, 0, options, findings)?;
    tx802_performance_of(&steps, 0, options, findings)
}

/// The TX802's performance of a group of its memory's dump, two hexadecimal
/// digits for each of its 84 packed bytes, each checked.
fn tx802_packed_of(
    data: &[u8],
    place: u8,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<tx802::Performance, Error> {
    if data.len() != TX802_PACKED_BYTES {
        return Err(Error::Length);
    }
    let packed = bytes_of_digits::<84>(data, place, options, findings)?;
    let steps = unpack_tx802(&packed, place, options, findings)?;
    tx802_performance_of(&steps, place, options, findings)
}

/// The TX802's performance of its 116 steps, in the order of its edit buffer,
/// each checked against its parameter's range.
fn tx802_performance_of(
    steps: &[u8; 116],
    place: u8,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<tx802::Performance, Error> {
    let mut performance = tx802::Performance::zero();
    for (parameter, &step) in tx802::PerformanceParameter::all().zip(steps) {
        if !performance.set_step(parameter, step) {
            let finding = Finding::Tx802PerformanceOutOfRange {
                performance: place,
                parameter,
                step: u16::from(step),
            };
            note(finding, options, findings)?;
        }
    }
    Ok(performance)
}

/// The TX802's system set-up of its 263 bytes, each checked against the
/// steps its parameter takes.
fn tx802_system_of(data: &[u8], options: Options, findings: &mut Vec<Finding>) -> Result<tx802::SystemSetup, Error> {
    if data.len() != TX802_SYSTEM_BYTES {
        return Err(Error::Length);
    }
    let mut setup = tx802::SystemSetup::zero();
    for (parameter, &step) in tx802::SystemParameter::all().zip(data) {
        if !setup.set_step(parameter, step) {
            note(Finding::Tx802SystemOutOfRange { parameter, step }, options, findings)?;
        }
    }
    Ok(setup)
}

/// `N` bytes of their `2N` hexadecimal digits, the high first \[TX802-SYSEX\],
/// each digit checked.
fn bytes_of_digits<const N: usize>(
    digits: &[u8],
    place: u8,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<[u8; N], Error> {
    let mut bytes = [0; N];
    let (pairs, _) = digits.as_chunks::<2>();
    for ((value, &[high, low]), byte) in (0..).zip(pairs).zip(&mut bytes) {
        let high = digit_of(high, place, value, options, findings)?;
        let low = digit_of(low, place, value, options, findings)?;
        *byte = high << 4 | low;
    }
    Ok(bytes)
}

/// The four bits that a hexadecimal digit stands for, '0' to '9' and 'A' to
/// 'F': any other byte is reported, and taken as the digit it most likely
/// stands for, ':' to '?' and 'a' to 'f' as 10 to 15, as the fractional
/// scaling's dump and small letters would write them, a byte below '0' as 0
/// and any other as 15.
fn digit_of(byte: u8, place: u8, value: u8, options: Options, findings: &mut Vec<Finding>) -> Result<u8, Error> {
    let digit = match byte {
        b'0'..=b'9' => return Ok(byte - b'0'),
        b'A'..=b'F' => return Ok(byte - b'A' + 10),
        b':'..=b'?' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        0..=0x2f => 0,
        _ => 15,
    };
    let finding = Finding::Tx802Digit {
        performance: place,
        value,
        byte,
    };
    note(finding, options, findings)?;
    Ok(digit)
}

/// The 116 steps of a packed performance of the TX802's memory, in the order
/// of its edit buffer \[TX802-SYSEX\] Table 5: the voice channel offset in
/// bits 7-5 of the first eight bytes and the receive channel in bits 4-0; the
/// voices; the micro tuning tables; the volumes in bits 6-0; the detune in bits
/// 6-3, the key assign group in bit 2 and the output in bits 1-0, the detune
/// taking four bits as dumps of the TX802 hold it \[TX802-DUMPS\], where the
/// table draws three; the lowest and the highest keys in bits 6-0; the forced
/// damp in bit 6 and the note shift in bits 5-0; and the name. A high bit set
/// where the table leaves it unused is reported.
fn unpack_tx802(
    packed: &[u8; 84],
    place: u8,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<[u8; 116], Error> {
    for (byte, &value) in (24..64).zip(&packed[24..64]) {
        if value & 0x80 != 0 {
            let finding = Finding::Tx802UnusedBits {
                performance: place,
                byte,
                bits: 0x80,
            };
            note(finding, options, findings)?;
        }
    }
    let mut steps = [0; 116];
    for generator in 0..8 {
        let channels = packed[generator];
        let tuning = packed[32 + generator];
        let damp = packed[56 + generator];
        steps[generator] = channels >> 5;
        steps[8 + generator] = channels & 0x1f;
        steps[16 + generator] = packed[8 + generator];
        steps[24 + generator] = tuning >> 3 & 0x0f;
        steps[32 + generator] = packed[24 + generator] & 0x7f;
        steps[40 + generator] = tuning & 0x03;
        steps[48 + generator] = packed[40 + generator] & 0x7f;
        steps[56 + generator] = packed[48 + generator] & 0x7f;
        steps[64 + generator] = damp & 0x3f;
        steps[72 + generator] = damp >> 6 & 1;
        steps[80 + generator] = tuning >> 2 & 1;
        steps[88 + generator] = packed[16 + generator];
    }
    steps[96..].copy_from_slice(&packed[64..]);
    Ok(steps)
}

/// The 84 packed bytes of a performance of the TX802's memory, of its 116
/// steps, each within its parameter's range ([`unpack_tx802`]).
fn pack_tx802(steps: &[u8; 116]) -> [u8; 84] {
    let mut packed = [0; 84];
    for generator in 0..8 {
        packed[generator] = steps[generator] << 5 | steps[8 + generator] & 0x1f;
        packed[8 + generator] = steps[16 + generator];
        packed[16 + generator] = steps[88 + generator];
        packed[24 + generator] = steps[32 + generator] & 0x7f;
        packed[32 + generator] =
            (steps[24 + generator] & 0x0f) << 3 | (steps[80 + generator] & 1) << 2 | steps[40 + generator] & 0x03;
        packed[40 + generator] = steps[48 + generator] & 0x7f;
        packed[48 + generator] = steps[56 + generator] & 0x7f;
        packed[56 + generator] = (steps[72 + generator] & 1) << 6 | steps[64 + generator] & 0x3f;
    }
    packed[64..].copy_from_slice(&steps[96..]);
    packed
}

/// The micro tuning of a group's data, two bytes for each key, the step's high
/// seven bits and its low seven, each step checked against its range.
fn micro_tuning_of(
    data: &[u8],
    place: u8,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<MicroTuning, Error> {
    let Ok(data) = <&[u8; MICRO_TUNING_BYTES]>::try_from(data) else {
        return Err(Error::Length);
    };
    let (pairs, _) = data.as_chunks::<2>();
    let mut tuning = MicroTuning::zero();
    for (key, &[high, low]) in Index::all().zip(pairs) {
        let step = u16::from(high) << 7 | u16::from(low);
        if !tuning.set_step(key, step) {
            let finding = Finding::MicroTuningOutOfRange {
                tuning: place,
                key,
                step,
            };
            note(finding, options, findings)?;
        }
    }
    Ok(tuning)
}

/// The fractional scaling of a group's data, two bytes for each parameter,
/// the step's high four bits and then its low four, each sent as the byte 0x30
/// above them, which is checked.
fn scaling_of(
    data: &[u8],
    place: u8,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<FractionalScaling, Error> {
    let Ok(data) = <&[u8; SCALING_BYTES]>::try_from(data) else {
        return Err(Error::Length);
    };
    let (pairs, _) = data.as_chunks::<2>();
    let mut scaling = FractionalScaling::zero();
    for (parameter, &[high, low]) in ScalingParameter::all().zip(pairs) {
        let high = nibble_of(high, place, parameter, options, findings)?;
        let low = nibble_of(low, place, parameter, options, findings)?;
        scaling.set_step(parameter, high << 4 | low);
    }
    Ok(scaling)
}

/// The four bits that a byte of a fractional scaling's dump sends, 0x30 above
/// them: a byte below 0x30 or above 0x3F is reported, and taken as the nearer
/// end, 0 or 15.
fn nibble_of(
    byte: u8,
    place: u8,
    parameter: ScalingParameter,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<u8, Error> {
    if !(NIBBLE_ZERO..=NIBBLE_ZERO + 0x0f).contains(&byte) {
        let finding = Finding::ScalingByte {
            scaling: place,
            parameter,
            byte,
        };
        note(finding, options, findings)?;
    }
    Ok(byte.saturating_sub(NIBBLE_ZERO).min(0x0f))
}

/// The DX7II performance of its 51 bytes, each checked against its
/// parameter's range.
fn dx7ii_performance_of(
    steps: &[u8; DX7II_PERFORMANCE_BYTES],
    place: u8,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<dx7ii::Performance, Error> {
    let mut performance = dx7ii::Performance::zero();
    for (parameter, &step) in dx7ii::PerformanceParameter::all().zip(steps) {
        if !performance.set_step(parameter, step) {
            let finding = Finding::Dx7iiPerformanceOutOfRange {
                performance: place,
                parameter,
                step,
            };
            note(finding, options, findings)?;
        }
    }
    Ok(performance)
}

/// The system set-up of a dump's bytes, which are those of `parameters`, each
/// checked against its parameter's range; the parameters the dump does not
/// hold are 0.
fn system_setup_of(
    steps: &[u8],
    parameters: impl Iterator<Item = SystemParameter>,
    options: Options,
    findings: &mut Vec<Finding>,
) -> Result<SystemSetup, Error> {
    let mut setup = SystemSetup::zero();
    for (parameter, &step) in parameters.zip(steps) {
        if !setup.set_step(parameter, step) {
            note(Finding::SystemOutOfRange { parameter, step }, options, findings)?;
        }
    }
    Ok(setup)
}

/// A universal bulk dump on a channel of a group for each of `groups`' data:
/// its count, its names, its data, and their checksum.
fn universal_message<'a>(channel: Channel, format: [u8; 6], groups: impl IntoIterator<Item = &'a [u8]>) -> Vec<u8> {
    let mut message = vec![START, YAMAHA, DUMP << 4 | channel.0, UNIVERSAL_FORMAT];
    let [f0, f1, f2, f3, f4, f5] = format;
    let [c0, c1, c2, c3] = LM;
    let names = [c0, c1, c2, c3, f0, f1, f2, f3, f4, f5];
    for data in groups {
        let mut counted = Vec::with_capacity(10 + data.len());
        counted.extend_from_slice(&names);
        counted.extend_from_slice(data);
        // A count below 2^14, as every group's is, of the data alone for the
        // TX802's performances.
        let length = if counts_data_alone(names) {
            data.len()
        } else {
            counted.len()
        };
        let count = u16::try_from(length).unwrap_or(u16::MAX);
        message.extend_from_slice(&seven_bits(count));
        message.extend_from_slice(&counted);
        message.push(checksum_of(&counted));
    }
    message.push(END);
    message
}

/// A number below 2^14 as two bytes, its high seven bits and its low seven.
fn seven_bits(number: u16) -> [u8; 2] {
    [
        u8::try_from(number >> 7 & 0x7f).unwrap_or(0x7f),
        u8::try_from(number & 0x7f).unwrap_or(0x7f),
    ]
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
    // The TX802's voice and micro tuning table, whose changes carry two bytes.
    let wide = tx802::PerformanceParameter::new(number)
        .filter(|parameter| group == TX802_PERFORMANCE_GROUP && parameter.is_wide());
    let (change, finding) = match ([group, number], data, wide) {
        (MICRO_TUNING_CHANGE, &[key, high, low], _) => micro_tuning_change(key, high, low),
        (SCALING_CHANGE, &[operator, key_group, high, low], _) => scaling_change(operator, key_group, high, low)?,
        (_, &[high, low], Some(parameter)) => tx802_wide_change(parameter, high, low),
        (MICRO_TUNING_CHANGE | SCALING_CHANGE, _, _) | (_, _, Some(_)) => return Err(Error::Length),
        _ => {
            let target = target_of(group, number).ok_or(Error::Unsupported)?;
            let &[data] = data else {
                return Err(Error::Length);
            };
            target.change(data)
        }
    };
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
        (DX7II_GROUP, 1) => dx7ii::PerformanceParameter::new(number)
            .map(Target::Dx7iiPerformance)
            .or_else(|| SystemParameter::of_change(number).map(Target::System)),
        (DX7II_GROUP, 2) => tx802::PerformanceParameter::new(number).map(Target::Tx802Performance),
        _ if group == SWITCH_REMOTE && number == SYSTEM_RESET => Some(Target::SystemReset),
        _ if group == SWITCH_REMOTE => RemoteSwitch::new(number)
            .map(Target::Remote)
            .or_else(|| Tx802Switch::new(number).map(Target::Tx802Switch)),
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
    Dx7iiPerformance(dx7ii::PerformanceParameter),
    System(SystemParameter),
    SystemReset,
    Tx802Performance(tx802::PerformanceParameter),
    Tx802Switch(Tx802Switch),
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
            Self::Dx7iiPerformance(parameter) => {
                let largest = parameter.largest();
                let finding = (data > largest).then_some(Finding::Dx7iiPerformanceOutOfRange {
                    performance: 0,
                    parameter,
                    step: data,
                });
                let step = Dx7iiPerformanceStep {
                    parameter,
                    step: data.min(largest),
                };
                (Change::Dx7iiPerformance(step), finding)
            }
            Self::System(parameter) => {
                let (smallest, largest) = (parameter.smallest(), parameter.largest());
                let finding = (!(smallest..=largest).contains(&data))
                    .then_some(Finding::SystemOutOfRange { parameter, step: data });
                let step = SystemStep {
                    parameter,
                    step: data.max(smallest).min(largest),
                };
                (Change::System(step), finding)
            }
            Self::SystemReset => (Change::SystemReset(Byte(data)), None),
            Self::Tx802Performance(parameter) => tx802_change(parameter, u16::from(data)),
            Self::Tx802Switch(switch) => (Change::Tx802Switch(switch, Byte(data)), None),
        }
    }
}

/// The change of a parameter of the TX802's performance to a step: a step
/// beyond the parameter's largest is reported, and taken as its largest.
fn tx802_change(parameter: tx802::PerformanceParameter, step: u16) -> (Change, Option<Finding>) {
    let largest = parameter.largest();
    let finding = (step > u16::from(largest)).then_some(Finding::Tx802PerformanceOutOfRange {
        performance: 0,
        parameter,
        step,
    });
    let step = Tx802PerformanceStep {
        parameter,
        step: u8::try_from(step).unwrap_or(largest).min(largest),
    };
    (Change::Tx802Performance(step), finding)
}

/// The change of the TX802's voice or micro tuning table, of its two bytes of
/// data, the step's high seven bits and its low seven.
fn tx802_wide_change(parameter: tx802::PerformanceParameter, high: u8, low: u8) -> (Change, Option<Finding>) {
    tx802_change(parameter, u16::from(high) << 7 | u16::from(low))
}

/// The change of a key of the micro tuning, of its three bytes of data: the
/// key, and the step's high seven bits and its low seven. A step beyond 10794
/// is reported, and taken as 10794.
fn micro_tuning_change(key: u8, high: u8, low: u8) -> (Change, Option<Finding>) {
    // The key is a byte of a message, below 128.
    let key = Index::saturating(key);
    let step = u16::from(high) << 7 | u16::from(low);
    let finding = (step > KeyPitch::LARGEST).then_some(Finding::MicroTuningOutOfRange { tuning: 0, key, step });
    let step = MicroTuningStep {
        key,
        step: step.min(KeyPitch::LARGEST),
    };
    (Change::MicroTuning(step), finding)
}

/// The change of a parameter of the fractional scaling, of its four bytes of
/// data: the operator's number, the group's, and the step's high bit and its
/// low seven. A step beyond 255 is reported, and taken as 255; numbers that
/// name no parameter make a message this reader does not take.
fn scaling_change(operator: u8, group: u8, high: u8, low: u8) -> Result<(Change, Option<Finding>), Error> {
    let parameter = ScalingParameter::of(operator, group).ok_or(Error::Unsupported)?;
    let step = u16::from(high) << 7 | u16::from(low);
    let finding = (step > 0xff).then_some(Finding::ScalingOutOfRange { parameter, step });
    let step = ScalingStep {
        parameter,
        step: u8::try_from(step).unwrap_or(u8::MAX),
    };
    Ok((Change::FractionalScaling(step), finding))
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

/// The DX7II's universal bulk dump of its performance edit buffer, on a
/// channel: a message of 69 bytes. Each value is rounded to a step by
/// `rounding`, and the parameters whose values lay between steps are given
/// beside it.
#[must_use]
pub fn write_dx7ii_performance(
    performance: &dx7ii::Performance,
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, Vec<(u8, dx7ii::PerformanceParameter)>) {
    let mut rounded = Vec::new();
    let steps = steps_of_dx7ii_performance(performance, 0, rounding, &mut rounded);
    (universal_message(channel, PERFORMANCE_EDIT, [&steps[..]]), rounded)
}

/// The DX7II's universal bulk dump of 32 performances, on a channel: a
/// message of 1650 bytes. Each value is rounded to a step by `rounding`, and
/// the parameters whose values lay between steps are given beside it, by the
/// place of their performance.
#[must_use]
pub fn write_dx7ii_performances(
    performances: &[dx7ii::Performance; 32],
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, Vec<(u8, dx7ii::PerformanceParameter)>) {
    let mut rounded = Vec::new();
    let mut data = Vec::with_capacity(DX7II_PERFORMANCES_BYTES);
    for (place, performance) in (0..).zip(performances) {
        data.extend_from_slice(&steps_of_dx7ii_performance(performance, place, rounding, &mut rounded));
    }
    (universal_message(channel, PERFORMANCE_MEMORY, [&data[..]]), rounded)
}

/// The DX7II's universal bulk dump of its system set-up, on a channel: a
/// message of 120 bytes. Each value is rounded to a step by `rounding`, and the
/// parameters whose values lay between steps are given beside it.
#[must_use]
pub fn write_system_setup(
    setup: &SystemSetup,
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, Vec<SystemParameter>) {
    let steps: Vec<u8> = SystemParameter::all()
        .map(|parameter| setup.step(parameter, rounding))
        .collect();
    let rounded = SystemParameter::all()
        .filter(|&parameter| !setup.is_step(parameter))
        .collect();
    (universal_message(channel, SYSTEM_SETUP, [&steps[..]]), rounded)
}

/// The DX7s's universal bulk dump of its system set-up, on a channel: a
/// message of 103 bytes, which has no place for the bytes that have no name.
/// Each value is rounded to a step by `rounding`, and the parameters whose
/// values lay between steps, and those of the bytes that have no name that
/// are not 0, are given beside it.
#[must_use]
pub fn write_dx7s_system_setup(
    setup: &SystemSetup,
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, Vec<SystemParameter>) {
    let steps: Vec<u8> = SystemParameter::in_dx7s_dump()
        .map(|parameter| setup.step(parameter, rounding))
        .collect();
    let lost = SystemParameter::all()
        .filter(|&parameter| match parameter.byte() {
            20..=36 => setup.value(parameter) != 0.0,
            _ => !setup.is_step(parameter),
        })
        .collect();
    (universal_message(channel, SYSTEM_SETUP, [&steps[..]]), lost)
}

/// The DX7II's universal bulk dump of its micro tuning edit buffer, on a
/// channel: a message of 274 bytes. Each pitch is rounded to a step by
/// `rounding`, and the keys whose pitches lay between steps are given beside
/// it.
#[must_use]
pub fn write_micro_tuning(tuning: &MicroTuning, channel: Channel, rounding: Rounding) -> (Vec<u8>, Vec<Index<127>>) {
    let (data, rounded) = data_of_micro_tuning(tuning, rounding);
    (universal_message(channel, MICRO_TUNING_EDIT, [&data[..]]), rounded)
}

/// The DX7II's universal bulk dump of one of the two micro tunings in its
/// memory, on a channel: a message of 274 bytes. Each pitch is rounded to a
/// step by `rounding`, and the keys whose pitches lay between steps are given
/// beside it.
#[must_use]
pub fn write_micro_tuning_memory(
    memory: Index<1>,
    tuning: &MicroTuning,
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, Vec<Index<127>>) {
    let [m0, m1, m2, m3, m4] = MICRO_TUNING_MEMORY;
    let (data, rounded) = data_of_micro_tuning(tuning, rounding);
    let format = [m0, m1, m2, m3, m4, memory.get()];
    (universal_message(channel, format, [&data[..]]), rounded)
}

/// The universal bulk dump of a cartridge's micro tunings, on a channel: a
/// group of 269 bytes for each of the first 64, as many as a cartridge holds,
/// in a message of 5 more. Each pitch is rounded to a step by `rounding`, and
/// the keys whose pitches lay between steps are given beside it, by the place
/// of their micro tuning. Of no micro tuning, the message has no group, and
/// [`decode`] does not take it.
#[must_use]
pub fn write_micro_tunings(
    tunings: &[MicroTuning],
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, Vec<(u8, Index<127>)>) {
    let mut rounded = Vec::new();
    let mut groups = Vec::with_capacity(tunings.len().min(MOST_GROUPS));
    for (place, tuning) in (0..).zip(tunings.iter().take(MOST_GROUPS)) {
        let (data, keys) = data_of_micro_tuning(tuning, rounding);
        rounded.extend(keys.into_iter().map(|key| (place, key)));
        groups.push(data);
    }
    let message = universal_message(channel, MICRO_TUNING_CARTRIDGE, groups.iter().map(Vec::as_slice));
    (message, rounded)
}

/// The 256 bytes of a micro tuning, each pitch rounded to a step by
/// `rounding`, and the keys whose pitches lay between steps.
fn data_of_micro_tuning(tuning: &MicroTuning, rounding: Rounding) -> (Vec<u8>, Vec<Index<127>>) {
    let mut data = Vec::with_capacity(MICRO_TUNING_BYTES);
    let mut rounded = Vec::new();
    for (key, pitch) in Index::all().zip(&tuning.keys) {
        data.extend_from_slice(&seven_bits(pitch.step(rounding)));
        if pitch.get().fract() != 0.0 {
            rounded.push(key);
        }
    }
    (data, rounded)
}

/// The DX7II's universal bulk dump of its fractional scaling edit buffer, on a
/// channel: a message of 510 bytes. Each level is rounded to a step by
/// `rounding`, and the parameters whose values lay between steps are given
/// beside it.
#[must_use]
pub fn write_fractional_scaling(
    scaling: &FractionalScaling,
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, Vec<ScalingParameter>) {
    let (data, rounded) = data_of_scaling(scaling, rounding);
    (universal_message(channel, SCALING_EDIT, [&data[..]]), rounded)
}

/// The universal bulk dump of a cartridge's fractional scalings, on a channel:
/// a group of 505 bytes for each of the first 64, as many as a cartridge holds,
/// in a message of 5 more. Each level is rounded to a step by `rounding`, and
/// the parameters whose values lay between steps are given beside it, by the
/// place of their fractional scaling. Of no fractional scaling, the message has
/// no group, and [`decode`] does not take it.
#[must_use]
pub fn write_fractional_scalings(
    scalings: &[FractionalScaling],
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, Vec<(u8, ScalingParameter)>) {
    let mut rounded = Vec::new();
    let mut groups = Vec::with_capacity(scalings.len().min(MOST_GROUPS));
    for (place, scaling) in (0..).zip(scalings.iter().take(MOST_GROUPS)) {
        let (data, parameters) = data_of_scaling(scaling, rounding);
        rounded.extend(parameters.into_iter().map(|parameter| (place, parameter)));
        groups.push(data);
    }
    let message = universal_message(channel, SCALING_CARTRIDGE, groups.iter().map(Vec::as_slice));
    (message, rounded)
}

/// The TX802's universal bulk dump of its performance edit buffer, on a
/// channel: a message of 250 bytes, each step as two hexadecimal digits. Each
/// value is rounded to a step by `rounding`, and the parameters whose values
/// lay between steps are given beside it.
#[must_use]
pub fn write_tx802_performance(
    performance: &tx802::Performance,
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, Vec<tx802::PerformanceParameter>) {
    let mut rounded = Vec::new();
    let steps = steps_of_tx802(performance, rounding, |parameter| rounded.push(parameter));
    (
        universal_message(channel, TX802_PERFORMANCE_EDIT, [&digits_of(&steps)[..]]),
        rounded,
    )
}

/// The TX802's universal bulk dump of the performances of its memory, on a
/// channel: a group of 181 bytes for each of the first 64, as many as its
/// memory holds, in a message of 5 more, each packed into 84 bytes
/// \[TX802-SYSEX\] Table 5 and each of those as two hexadecimal digits. Each
/// value is rounded to a step by `rounding`, and the parameters whose values
/// lay between steps are given beside it, by the place of their performance.
/// Of no performance, the message has no group, and [`decode`] does not take
/// it.
#[must_use]
pub fn write_tx802_performances(
    performances: &[tx802::Performance],
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, Vec<(u8, tx802::PerformanceParameter)>) {
    let mut rounded = Vec::new();
    let mut groups = Vec::with_capacity(performances.len().min(MOST_GROUPS));
    for (place, performance) in (0..).zip(performances.iter().take(MOST_GROUPS)) {
        let steps = steps_of_tx802(performance, rounding, |parameter| rounded.push((place, parameter)));
        groups.push(digits_of(&pack_tx802(&steps)));
    }
    let message = universal_message(channel, TX802_PERFORMANCE_MEMORY, groups.iter().map(Vec::as_slice));
    (message, rounded)
}

/// The TX802's universal bulk dump of its system set-up, on a channel: a
/// message of 281 bytes. The master tune is rounded to a step by `rounding`,
/// and given beside it if it lay between steps.
#[must_use]
pub fn write_tx802_system_setup(
    setup: &tx802::SystemSetup,
    channel: Channel,
    rounding: Rounding,
) -> (Vec<u8>, Vec<tx802::SystemParameter>) {
    let steps: Vec<u8> = tx802::SystemParameter::all()
        .map(|parameter| setup.step(parameter, rounding))
        .collect();
    let rounded = tx802::SystemParameter::all()
        .filter(|&parameter| !setup.is_step(parameter))
        .collect();
    (universal_message(channel, TX802_SYSTEM_SETUP, [&steps[..]]), rounded)
}

/// The 116 steps of a TX802 performance, each rounded by `rounding`, the
/// parameters whose values lay between steps given to `rounded`.
fn steps_of_tx802(
    performance: &tx802::Performance,
    rounding: Rounding,
    mut rounded: impl FnMut(tx802::PerformanceParameter),
) -> [u8; 116] {
    let mut steps = [0; 116];
    for (parameter, step) in tx802::PerformanceParameter::all().zip(&mut steps) {
        *step = performance.step(parameter, rounding);
        if !performance.is_step(parameter) {
            rounded(parameter);
        }
    }
    steps
}

/// The hexadecimal digits of bytes, two to a byte, the high first, '0' to '9'
/// and 'A' to 'F' \[TX802-SYSEX\].
fn digits_of(bytes: &[u8]) -> Vec<u8> {
    let digit = |nibble: u8| if nibble < 10 { b'0' + nibble } else { b'A' + nibble - 10 };
    bytes
        .iter()
        .flat_map(|&byte| [digit(byte >> 4), digit(byte & 0x0f)])
        .collect()
}

/// The 492 bytes of a fractional scaling, each step's high four bits and then
/// its low four, each as the byte 0x30 above them, each level rounded to a step
/// by `rounding`; and the parameters whose values lay between steps.
fn data_of_scaling(scaling: &FractionalScaling, rounding: Rounding) -> (Vec<u8>, Vec<ScalingParameter>) {
    let mut data = Vec::with_capacity(SCALING_BYTES);
    let mut rounded = Vec::new();
    for parameter in ScalingParameter::all() {
        let step = scaling.step(parameter, rounding);
        data.extend_from_slice(&[NIBBLE_ZERO + (step >> 4), NIBBLE_ZERO + (step & 0x0f)]);
        if !scaling.is_step(parameter) {
            rounded.push(parameter);
        }
    }
    (data, rounded)
}

/// The steps of a DX7II performance's parameters, and the parameters whose
/// values they round.
fn steps_of_dx7ii_performance(
    performance: &dx7ii::Performance,
    place: u8,
    rounding: Rounding,
    rounded: &mut Vec<(u8, dx7ii::PerformanceParameter)>,
) -> [u8; DX7II_PERFORMANCE_BYTES] {
    let mut steps = [0; DX7II_PERFORMANCE_BYTES];
    for (parameter, step) in dx7ii::PerformanceParameter::all().zip(&mut steps) {
        *step = performance.step(parameter, rounding);
        if !performance.is_step(parameter) {
            rounded.push((place, parameter));
        }
    }
    steps
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

/// A parameter change on a channel: a message of 7 bytes, 8 for the TX802's
/// voice and micro tuning table, 9 for a key of a micro tuning and 10 for a
/// parameter of a fractional scaling.
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
        Change::Dx7iiPerformance(step) => (DX7II_GROUP << 2 | 1, step.parameter.number(), step.step),
        Change::System(step) => (
            DX7II_GROUP << 2 | 1,
            SystemParameter::FIRST_CHANGE + step.parameter.byte(),
            step.step,
        ),
        Change::SystemReset(data) => (SWITCH_REMOTE, SYSTEM_RESET, data.0),
        Change::Tx802Performance(step) if step.parameter.is_wide() => {
            let [high, low] = seven_bits(u16::from(step.step));
            let number = step.parameter.number();
            return vec![
                START,
                YAMAHA,
                CHANGE << 4 | channel.0,
                TX802_PERFORMANCE_GROUP,
                number,
                high,
                low,
                END,
            ];
        }
        Change::Tx802Performance(step) => (TX802_PERFORMANCE_GROUP, step.parameter.number(), step.step),
        Change::Tx802Switch(switch, data) => (SWITCH_REMOTE, switch.number(), data.0),
        Change::MicroTuning(step) => {
            let [group, number] = MICRO_TUNING_CHANGE;
            let [high, low] = seven_bits(step.step);
            let key = step.key.get();
            return vec![
                START,
                YAMAHA,
                CHANGE << 4 | channel.0,
                group,
                number,
                key,
                high,
                low,
                END,
            ];
        }
        Change::FractionalScaling(step) => {
            let [group, number] = SCALING_CHANGE;
            let (operator, key_group) = (step.parameter.operator(), step.parameter.group());
            let (high, low) = (step.step >> 7, step.step & 0x7f);
            return vec![
                START,
                YAMAHA,
                CHANGE << 4 | channel.0,
                group,
                number,
                operator,
                key_group,
                high,
                low,
                END,
            ];
        }
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
