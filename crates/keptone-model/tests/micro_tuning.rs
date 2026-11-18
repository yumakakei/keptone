// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The DX7II's micro tunings, their universal bulk dumps and their parameter
//! change, against \[DX7II-MANUAL\], whose layout `common` states again by
//! itself: a micro tuning is two bytes for each of the 128 keys, the step's
//! high seven bits and its low seven, the step from 0 to 10794; the edit
//! buffer's dump is named `MCRYE `, a memory's `MCRYM` and the memory's number
//! as a byte, and a cartridge's `MCRYC `, a group for each micro tuning. Each
//! key lands in its place, random valid dumps come back byte for byte, a step
//! beyond 10794 is reported and taken as `syx.out_of_range` says, what is of no
//! kind or of the wrong length is refused, and the parameter change carries
//! every key.

mod common;

use common::{
    MICRO_TUNING_LARGEST, SplitMix64, micro_tuning_bytes, random_micro_tuning_steps, universal, universal_groups,
};
use keptone_model::dx7ii::{Index, KeyPitch, MicroTuning};
use keptone_model::syx::{
    self, Change, Channel, Content, Error, Finding, MicroTuningStep, Options, OutOfRange, ParameterChange, Sysex,
};
use keptone_model::voice::Rounding;

/// The options that refuse whatever a valid dump does not hold.
const STRICT: Options = Options {
    accept_bad_checksum: false,
    out_of_range: OutOfRange::Reject,
};

/// The names of the edit buffer.
const EDIT: &[u8; 10] = b"LM  MCRYE ";

/// The names of a cartridge's micro tunings.
const CARTRIDGE: &[u8; 10] = b"LM  MCRYC ";

/// A channel.
fn channel(number: u8) -> Channel {
    Channel::new(number).expect("a channel")
}

/// A key.
fn key(number: u8) -> Index<127> {
    Index::new(number).expect("a key")
}

/// The pitch of a step.
fn pitch(step: u16) -> KeyPitch {
    KeyPitch::from_step(step).expect("a step")
}

/// The content of the dump a message is, and what it found.
fn content_of(message: &[u8], options: Options) -> (Content, Vec<Finding>) {
    match syx::decode(message, options) {
        Ok(Sysex::Dump(dump)) => (dump.content, dump.findings),
        other => panic!("not a dump: {other:?}"),
    }
}

/// The micro tuning of an edit buffer's dump.
fn tuning_of(message: &[u8], options: Options) -> (MicroTuning, Vec<Finding>) {
    match content_of(message, options) {
        (Content::MicroTuning(tuning), findings) => (*tuning, findings),
        other => panic!("not a micro tuning: {other:?}"),
    }
}

/// The micro tunings of a cartridge's dump.
fn tunings_of(message: &[u8], options: Options) -> (Vec<MicroTuning>, Vec<Finding>) {
    match content_of(message, options) {
        (Content::MicroTunings(tunings), findings) => (tunings, findings),
        other => panic!("not a cartridge's micro tunings: {other:?}"),
    }
}

#[test]
fn each_key_lands_in_its_place() {
    // Each key a step of its own; the largest, 10794, at key 0, two bytes 84
    // and 42; 128 at key 64, its high byte 1; 1 at key 127.
    let mut steps: [u16; 128] = std::array::from_fn(|key| u16::try_from(key * 83 + key % 7).expect("a step"));
    steps[0] = MICRO_TUNING_LARGEST;
    steps[64] = 128;
    steps[127] = 1;
    let data = micro_tuning_bytes(&steps);
    assert_eq!((data[0], data[1], data[128], data[129], data[255]), (84, 42, 1, 0, 1));
    let (tuning, findings) = tuning_of(&universal(3, EDIT, &data), STRICT);
    assert_eq!(findings, []);
    assert_eq!(tuning.keys, steps.map(pitch));
}

#[test]
fn dumps_come_back_byte_for_byte() {
    let mut random = SplitMix64(61);
    for round in 0..200_u8 {
        let on = round % 16;
        let steps = random_micro_tuning_steps(&mut random);
        let data = micro_tuning_bytes(&steps);
        let message = universal(on, EDIT, &data);
        let (tuning, findings) = tuning_of(&message, STRICT);
        assert_eq!(findings, []);
        assert_eq!(tuning.keys, steps.map(pitch));
        let written = syx::write_micro_tuning(&tuning, channel(on), Rounding::Nearest);
        assert_eq!(written, (message, Vec::new()));
        // The same in a memory, 0 or 1, its number a byte after `MCRYM`.
        let memory = round % 2;
        let names = [b'L', b'M', b' ', b' ', b'M', b'C', b'R', b'Y', b'M', memory];
        let message = universal(on, &names, &data);
        let (
            Content::MicroTuningMemory {
                memory: read,
                tuning: other,
            },
            findings,
        ) = content_of(&message, STRICT)
        else {
            panic!("not a memory's micro tuning");
        };
        assert_eq!((read.get(), *other == tuning, findings), (memory, true, Vec::new()));
        let written = syx::write_micro_tuning_memory(read, &other, channel(on), Rounding::Nearest);
        assert_eq!(written, (message, Vec::new()));
    }
    // A cartridge's of 64, the DX7II's, of 63, the DX7s's, and of one.
    for count in [64, 63, 1] {
        let steps: Vec<[u16; 128]> = (0..count).map(|_| random_micro_tuning_steps(&mut random)).collect();
        let groups: Vec<Vec<u8>> = steps.iter().map(micro_tuning_bytes).collect();
        let message = universal_groups(count % 16, CARTRIDGE, &groups);
        let (tunings, findings) = tunings_of(&message, STRICT);
        assert_eq!(findings, []);
        assert_eq!(tunings.len(), usize::from(count));
        for (tuning, steps) in tunings.iter().zip(&steps) {
            assert_eq!(tuning.keys, steps.map(pitch));
        }
        let written = syx::write_micro_tunings(&tunings, channel(count % 16), Rounding::Nearest);
        assert_eq!(written, (message, Vec::new()));
    }
}

#[test]
fn steps_beyond_the_largest() {
    // Key 5 at 85 · 128, key 6 at 10795, key 7 at 10794, the largest, and key
    // 8 at 0x3FFF.
    let mut data = micro_tuning_bytes(&[0; 128]);
    data[10..18].copy_from_slice(&[85, 0, 84, 43, 84, 42, 127, 127]);
    let beyond = |tuning, number, step| Finding::MicroTuningOutOfRange {
        tuning,
        key: key(number),
        step,
    };
    let message = universal(0, EDIT, &data);
    let (tuning, findings) = tuning_of(&message, Options::default());
    assert_eq!(
        findings,
        [beyond(0, 5, 10880), beyond(0, 6, 10795), beyond(0, 8, 16383)]
    );
    assert_eq!(tuning.keys[5..9], [pitch(MICRO_TUNING_LARGEST); 4]);
    assert_eq!(syx::decode(&message, STRICT), Err(Error::Refused(findings[0])));
    // In a cartridge, by the place of its micro tuning.
    let zero = micro_tuning_bytes(&[0; 128]);
    let message = universal_groups(0, CARTRIDGE, &[zero.clone(), zero, data]);
    let (_, findings) = tunings_of(&message, Options::default());
    assert_eq!(
        findings,
        [beyond(2, 5, 10880), beyond(2, 6, 10795), beyond(2, 8, 16383)]
    );
}

#[test]
fn names_groups_and_lengths() {
    let mut random = SplitMix64(62);
    let data = micro_tuning_bytes(&random_micro_tuning_steps(&mut random));
    // A memory of the number 2, or of the letter 0; two groups of the edit
    // buffer; a cartridge of 65 groups, and one whose groups' names differ.
    let one = universal_groups(0, CARTRIDGE, std::slice::from_ref(&data));
    let unsupported = [
        universal(0, b"LM  MCRYM\x02", &data),
        universal(0, b"LM  MCRYM0", &data),
        universal_groups(0, EDIT, &[data.clone(), data.clone()]),
        universal_groups(0, CARTRIDGE, &vec![data.clone(); 65]),
        [&one[..one.len() - 1], &universal(0, EDIT, &data)[4..]].concat(),
    ];
    for message in unsupported {
        assert_eq!(syx::decode(&message, Options::default()), Err(Error::Unsupported));
    }
    // A wrong checksum in a cartridge's third group.
    let mut message = universal_groups(0, CARTRIDGE, &vec![data.clone(); 4]);
    let at = 4 + 3 * 269 - 1;
    let right = message[at];
    message[at] = (right + 1) & 0x7f;
    let (_, findings) = tunings_of(&message, Options::default());
    let finding = Finding::UniversalChecksum {
        group: 2,
        expected: right,
        found: (right + 1) & 0x7f,
    };
    assert_eq!(findings, [finding]);
    let refusing = Options {
        accept_bad_checksum: false,
        ..Options::default()
    };
    assert_eq!(syx::decode(&message, refusing), Err(Error::Refused(finding)));
    // A byte of data too few and too many, in the edit buffer and in a
    // cartridge's second group.
    let lengths = [
        universal(0, EDIT, &data[..255]),
        universal(0, EDIT, &[&data[..], &[0]].concat()),
        universal_groups(0, CARTRIDGE, &[data.clone(), data[..255].to_vec()]),
    ];
    for message in lengths {
        assert_eq!(syx::decode(&message, Options::default()), Err(Error::Length));
    }
    // Of no micro tuning, a cartridge's dump has no group, which is no dump;
    // of more than 64, it holds the first 64.
    let (written, _) = syx::write_micro_tunings(&[], channel(0), Rounding::Nearest);
    assert_eq!(written, [0xf0, 0x43, 0x00, 0x7e, 0xf7]);
    assert_eq!(syx::decode(&written, Options::default()), Err(Error::Length));
    let (tuning, _) = tuning_of(&universal(0, EDIT, &data), STRICT);
    let (written, _) = syx::write_micro_tunings(&vec![tuning.clone(); 65], channel(0), Rounding::Nearest);
    assert_eq!(written, universal_groups(0, CARTRIDGE, &vec![data; 64]));
}

#[test]
fn changes_of_every_key() {
    for number in 0..128 {
        for step in [0, 1, 127, 128, 5000, MICRO_TUNING_LARGEST] {
            let change = Change::MicroTuning(MicroTuningStep::new(key(number), step).expect("a step"));
            let message = syx::write_change(channel(2), change);
            let [high, low] = [step >> 7, step & 0x7f].map(|half| u8::try_from(half).expect("seven bits"));
            assert_eq!(message, [0xf0, 0x43, 0x12, 0x18, 0x7e, number, high, low, 0xf7]);
            assert_eq!(
                syx::decode(&message, STRICT),
                Ok(Sysex::Change(ParameterChange {
                    channel: channel(2),
                    change,
                    finding: None,
                }))
            );
        }
    }
    assert!(MicroTuningStep::new(key(0), MICRO_TUNING_LARGEST + 1).is_none());
    // A step beyond the largest: reported, and taken as the largest.
    let message = [0xf0, 0x43, 0x10, 0x18, 0x7e, 0x05, 0x55, 0x00, 0xf7];
    let finding = Finding::MicroTuningOutOfRange {
        tuning: 0,
        key: key(5),
        step: 10880,
    };
    let largest = MicroTuningStep::new(key(5), MICRO_TUNING_LARGEST).expect("a step");
    let expected = ParameterChange {
        channel: channel(0),
        change: Change::MicroTuning(largest),
        finding: Some(finding),
    };
    assert_eq!(syx::decode(&message, Options::default()), Ok(Sysex::Change(expected)));
    assert_eq!(syx::decode(&message, STRICT), Err(Error::Refused(finding)));
}

#[test]
fn pitches_between_steps() {
    let mut random = SplitMix64(63);
    let data = micro_tuning_bytes(&random_micro_tuning_steps(&mut random));
    let (mut tuning, _) = tuning_of(&universal(0, EDIT, &data), STRICT);
    tuning.keys[3] = KeyPitch::new(100.5).expect("a pitch");
    tuning.keys[90] = KeyPitch::new(10793.75).expect("a pitch");
    // Beyond the scale, its nearer end (docs/MATH.md §0.5).
    tuning.keys[5] = KeyPitch::new(-3.0).expect("a pitch");
    tuning.keys[6] = KeyPitch::new(1e300).expect("a pitch");
    for (rounding, steps) in [(Rounding::Nearest, [100, 10794]), (Rounding::Floor, [100, 10793])] {
        let (written, rounded) = syx::write_micro_tuning(&tuning, channel(0), rounding);
        assert_eq!(rounded, [key(3), key(5), key(6), key(90)]);
        assert!(written[1..written.len() - 1].iter().all(|&byte| byte < 0x80));
        let (read, _) = tuning_of(&written, STRICT);
        assert_eq!([read.keys[3], read.keys[90]], steps.map(pitch));
        assert_eq!([read.keys[5], read.keys[6]], [0, 10794].map(pitch));
        let step = MicroTuningStep::of(&tuning, key(90), rounding);
        assert_eq!((step.key(), step.step()), (key(90), steps[1]));
        assert_eq!(MicroTuningStep::of(&tuning, key(6), rounding).step(), 10794);
    }
    // A pitch is any finite number, never −0.
    assert!(KeyPitch::new(-0.5).is_some() && KeyPitch::new(10794.5).is_some() && KeyPitch::new(f64::NAN).is_none());
    assert!(KeyPitch::new(f64::INFINITY).is_none());
    assert!(KeyPitch::new(-0.0).expect("zero").get().is_sign_positive());
    assert!(KeyPitch::from_step(10795).is_none());
}
