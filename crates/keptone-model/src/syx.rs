// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The DX7's dumps of voices, read and written byte for byte as \[DEXED-SYSEX\]
//! lays them out: the single-voice dump, the 155 parameters of one voice a byte
//! each, and the 32-voice dump, a bank of 32 voices of 128 bytes each, two or
//! three parameters packed into some of the bytes. `.syx` files hold them as
//! the system-exclusive messages the DX7 sends.
//!
//! [`read`] finds every message in a file, not only the first: a file may hold
//! several, one after another, with bytes that belong to no message before,
//! between and after them, and a message may be cut short (docs/DESIGN.md
//! §11.2). It gives each message's dump, which [`decode`] decodes, or the
//! reason it holds none. A dump's checksum is checked: a wrong one is reported,
//! and the dump decoded all the same only if `syx.accept_bad_checksum` allows
//! it. Every byte is checked against the range of its parameter, and a packed
//! byte for bits its format leaves unused: either is reported, and either taken
//! as the end of the range and the bits ignored, or the message refused, as
//! `syx.out_of_range` says (docs/MATH.md §17). A message is decoded whole or
//! refused whole.
//!
//! [`write_voice`] and [`write_bank`] write dumps, each value rounded to a step
//! as `syx.export_rounding` says, and say what a dump could not hold of its
//! voices. A dump that is valid -- its checksum right, every byte in range and
//! no unused bit set -- comes back from them byte for byte as it was read
//! (MATH §16).
//!
//! Every function here takes any bytes and gives a value or an error, never a
//! panic, and allocates in proportion to the bytes it is given: [`read`] goes
//! through a file a message at a time, and a dump decoded takes six to eight
//! times the bytes of its message, 1056 for each voice.

use std::fmt;

use crate::voice::{Parameter, Rounding, Voice};

/// The byte that starts a system-exclusive message.
const START: u8 = 0xf0;

/// The byte that ends a system-exclusive message.
const END: u8 = 0xf7;

/// Yamaha's manufacturer ID.
const YAMAHA: u8 = 0x43;

/// The format number of a single-voice dump.
const VOICE_FORMAT: u8 = 0;

/// The format number of a 32-voice dump.
const BANK_FORMAT: u8 = 9;

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

/// How dumps are read: two of the settings of docs/MATH.md §17.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Options {
    /// `syx.accept_bad_checksum`: whether a dump whose checksum is wrong is
    /// decoded all the same, the default, or refused. A wrong checksum is
    /// reported either way.
    pub accept_bad_checksum: bool,
    /// `syx.out_of_range`: what becomes of a byte beyond its parameter's range,
    /// or a packed byte with unused bits set.
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
    /// largest step, and bits that a packed byte leaves unused are ignored: the
    /// default.
    #[default]
    Clamp,
    /// The message is refused.
    Reject,
}

/// Something a dump held that was not as its format has it.
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
        /// The place of the voice: 0 in a single-voice dump, 0 to 31 in a bank.
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
        }
    }
}

/// Why a message gives no dump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Error {
    /// The bytes are not one system-exclusive message: F0, bytes below 0x80,
    /// and F7.
    NotSysEx,
    /// A message that does not end: the bytes run out, or another status byte
    /// comes, before its F7.
    Unterminated,
    /// A message that is no DX7 dump of voices: another maker's, another kind
    /// of message, or a format other than the two.
    NotDump,
    /// A dump whose byte count or length is not its format's: 155 bytes of
    /// data in a message of 163 for a single voice, 4096 in a message of 4104
    /// for a bank.
    Length,
    /// A dump refused for what it held, as the options have it: a wrong
    /// checksum if `syx.accept_bad_checksum` is false, or a byte out of range
    /// if `syx.out_of_range` is `reject`; the first found.
    Refused(Finding),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSysEx => formatter.write_str("not a system-exclusive message"),
            Self::Unterminated => formatter.write_str("a system-exclusive message that does not end"),
            Self::NotDump => formatter.write_str("not a DX7 dump of voices"),
            Self::Length => formatter.write_str("a DX7 dump of the wrong length"),
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
    /// What it held that was not as its format has it, in the order of its
    /// bytes, each taken as the options say.
    pub findings: Vec<Finding>,
}

/// What a dump holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    /// A single-voice dump's voice.
    Voice(Box<Voice>),
    /// A 32-voice dump's voices, in the order of their places.
    Bank(Box<[Voice; 32]>),
}

/// A system-exclusive message of a file: where it lies, and its dump or the
/// reason it has none.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// The offset of its F0 in the file.
    pub offset: usize,
    /// Its length in bytes: from its F0 to its F7, or, where it does not end,
    /// to the last byte before what cut it short.
    pub length: usize,
    /// Its dump, or the reason it has none.
    pub result: Result<Dump, Error>,
}

/// The system-exclusive messages of a file, in order, as [`read`] gives them.
#[derive(Debug, Clone)]
pub struct Messages<'a> {
    /// The bytes not yet read.
    rest: &'a [u8],
    /// The offset of `rest` in the file.
    offset: usize,
    /// How dumps are read.
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

/// Every system-exclusive message in a file's bytes, in order, each with the
/// dump it holds or the reason it holds none.
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

/// The dump one system-exclusive message holds: a single-voice dump or a
/// 32-voice dump, its findings taken as `options` say.
///
/// # Errors
///
/// [`Error::NotSysEx`] where the bytes are not one system-exclusive message,
/// [`Error::NotDump`] where the message is no DX7 dump of voices,
/// [`Error::Length`] where the dump's byte count or length is not its
/// format's, and [`Error::Refused`] where the options refuse what it holds.
pub fn decode(message: &[u8], options: Options) -> Result<Dump, Error> {
    let [START, inner @ .., END] = message else {
        return Err(Error::NotSysEx);
    };
    if inner.iter().any(|&byte| byte >= 0x80) {
        return Err(Error::NotSysEx);
    }
    // The header: the maker, the sub-status 0 of a dump with the channel, the
    // format and the byte count.
    let &[YAMAHA, status, format, high, low, ref rest @ ..] = inner else {
        return Err(Error::NotDump);
    };
    if status >> 4 != 0 {
        return Err(Error::NotDump);
    }
    let count = match format {
        VOICE_FORMAT => VOICE_COUNT,
        BANK_FORMAT => BANK_COUNT,
        _ => return Err(Error::NotDump),
    };
    let Some((&checksum, data)) = rest.split_last() else {
        return Err(Error::Length);
    };
    if [high, low] != count {
        return Err(Error::Length);
    }
    let mut findings = Vec::new();
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
    let content = if format == VOICE_FORMAT {
        let Ok(steps) = <&[u8; VOICE_BYTES]>::try_from(data) else {
            return Err(Error::Length);
        };
        Content::Voice(Box::new(voice_of(steps, 0, options, &mut findings)?))
    } else {
        let Ok(data) = <&[u8; BANK_BYTES]>::try_from(data) else {
            return Err(Error::Length);
        };
        let (packed, _) = data.as_chunks::<PACKED_BYTES>();
        let mut voices = Vec::with_capacity(32);
        for (place, packed) in (0..).zip(packed) {
            let steps = unpack(packed, place, options, &mut findings)?;
            voices.push(voice_of(&steps, place, options, &mut findings)?);
        }
        let Ok(voices) = <Box<[Voice; 32]>>::try_from(voices.into_boxed_slice()) else {
            return Err(Error::Length);
        };
        Content::Bank(voices)
    };
    Ok(Dump {
        channel: Channel(status),
        content,
        findings,
    })
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
    message.extend_from_slice(&[START, YAMAHA, channel.0, format]);
    message.extend_from_slice(&count);
    message.extend_from_slice(data);
    message.push(checksum_of(data));
    message.push(END);
    message
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
