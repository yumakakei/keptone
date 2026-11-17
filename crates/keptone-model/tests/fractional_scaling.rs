// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The DX7II's fractional scalings, their universal bulk dumps and their
//! parameter change, against \[DX7II-MANUAL\] and \[DX7S-MANUAL\], whose layout
//! `common` states again by itself: a fractional scaling is 246 bytes, for
//! each operator from OP6 its offset and the levels of its 40 groups of keys,
//! each byte sent as two, its high four bits and then its low four, each 0x30
//! above them; the edit buffer's dump is named `FKSYE ` and a cartridge's
//! `FKSYC `, a group for each fractional scaling. Each byte lands in its
//! place, random valid dumps come back byte for byte, a byte that sends no four
//! bits is reported and taken as `syx.out_of_range` says, what is of no kind or
//! of the wrong length is refused, the parameter change carries every
//! parameter, and an offset's byte holds it in two's complement.

mod common;

use common::{SplitMix64, random_scaling_bytes, scaling_bytes, universal, universal_groups};
use keptone_model::dx7ii::{FractionalScaling, Offset, ScalingParameter};
use keptone_model::syx::{
    self, Change, Channel, Content, Error, Finding, Options, OutOfRange, ParameterChange, ScalingStep, Sysex,
};
use keptone_model::voice::{Rounding, Value};

/// The options that refuse whatever a valid dump does not hold.
const STRICT: Options = Options {
    accept_bad_checksum: false,
    out_of_range: OutOfRange::Reject,
};

/// The names of the edit buffer.
const EDIT: &[u8; 10] = b"LM  FKSYE ";

/// The names of a cartridge's fractional scalings.
const CARTRIDGE: &[u8; 10] = b"LM  FKSYC ";

/// A channel.
fn channel(number: u8) -> Channel {
    Channel::new(number).expect("a channel")
}

/// A parameter.
fn parameter(place: u8) -> ScalingParameter {
    ScalingParameter::new(place).expect("a parameter")
}

/// The content of the dump a message is, and what it found.
fn content_of(message: &[u8], options: Options) -> (Content, Vec<Finding>) {
    match syx::decode(message, options) {
        Ok(Sysex::Dump(dump)) => (dump.content, dump.findings),
        other => panic!("not a dump: {other:?}"),
    }
}

/// The fractional scaling of an edit buffer's dump.
fn scaling_of(message: &[u8], options: Options) -> (FractionalScaling, Vec<Finding>) {
    match content_of(message, options) {
        (Content::FractionalScaling(scaling), findings) => (*scaling, findings),
        other => panic!("not a fractional scaling: {other:?}"),
    }
}

/// The fractional scalings of a cartridge's dump.
fn scalings_of(message: &[u8], options: Options) -> (Vec<FractionalScaling>, Vec<Finding>) {
    match content_of(message, options) {
        (Content::FractionalScalings(scalings), findings) => (scalings, findings),
        other => panic!("not a cartridge's fractional scalings: {other:?}"),
    }
}

/// The 246 bytes of a fractional scaling, in the order of its dump.
fn bytes_of(scaling: &FractionalScaling) -> Vec<f64> {
    ScalingParameter::all()
        .map(|parameter| scaling.value(parameter))
        .collect()
}

#[test]
fn each_byte_lands_in_its_place() {
    let mut bytes: [u8; 246] = std::array::from_fn(|place| u8::try_from(place * 7 % 256).expect("a byte"));
    // OP6's offset, sent first, and the level of its lowest group; OP5's
    // offset; OP1's highest group's level, sent last.
    bytes[0] = 0x80;
    bytes[1] = 0xff;
    bytes[41] = 0x7f;
    bytes[245] = 0x01;
    let data = scaling_bytes(&bytes);
    assert_eq!(data[..4], [0x38, 0x30, 0x3f, 0x3f]);
    assert_eq!(data[490..], [0x30, 0x31]);
    let (scaling, findings) = scaling_of(&universal(4, EDIT, &data), STRICT);
    assert_eq!(findings, []);
    let [op1, _, _, _, op5, op6] = &scaling.operators;
    assert_eq!(
        (op6.offset.byte(), op6.levels[0]),
        (0x80, Value::from_step(0xff).expect("a level"))
    );
    assert_eq!(
        (op5.offset.byte(), op1.levels[39]),
        (0x7f, Value::from_step(1).expect("a level"))
    );
    assert_eq!(bytes_of(&scaling), bytes.map(f64::from));
    // OP4's offset, at 82, and its 18th group, at 100.
    assert_eq!(scaling.operators[3].offset.byte(), bytes[82]);
    assert_eq!(scaling.operators[3].levels[17].get(), f64::from(bytes[100]));
}

#[test]
fn dumps_come_back_byte_for_byte() {
    let mut random = SplitMix64(71);
    for round in 0..200_u8 {
        let on = round % 16;
        let bytes = random_scaling_bytes(&mut random);
        let message = universal(on, EDIT, &scaling_bytes(&bytes));
        let (scaling, findings) = scaling_of(&message, STRICT);
        assert_eq!(findings, []);
        assert_eq!(bytes_of(&scaling), bytes.map(f64::from));
        let written = syx::write_fractional_scaling(&scaling, channel(on), Rounding::Nearest);
        assert_eq!(written, (message, Vec::new()));
    }
    // A cartridge's of 32, the DX7II's, of 64, as many as the DX7s's holds,
    // and of one.
    for count in [32, 64, 1_u8] {
        let bytes: Vec<[u8; 246]> = (0..count).map(|_| random_scaling_bytes(&mut random)).collect();
        let groups: Vec<Vec<u8>> = bytes.iter().map(scaling_bytes).collect();
        let message = universal_groups(count % 16, CARTRIDGE, &groups);
        let (scalings, findings) = scalings_of(&message, STRICT);
        assert_eq!(findings, []);
        assert_eq!(scalings.len(), usize::from(count));
        for (scaling, bytes) in scalings.iter().zip(&bytes) {
            assert_eq!(bytes_of(scaling), bytes.map(f64::from));
        }
        let written = syx::write_fractional_scalings(&scalings, channel(count % 16), Rounding::Nearest);
        assert_eq!(written, (message, Vec::new()));
    }
}

#[test]
fn offsets_in_twos_complement() {
    // The offset's byte holds it in two's complement (docs/DESIGN.md §15).
    let offsets = [
        (0x00, 0.0),
        (0x01, 1.0),
        (0x74, 116.0),
        (0x7f, 127.0),
        (0x80, -128.0),
        (0xff, -1.0),
    ];
    for (byte, offset) in offsets {
        assert_eq!((Offset::new(byte).byte(), Offset::new(byte).value()), (byte, offset));
    }
    // OP6's offset at 0xFF and OP1's at 0x80, in a dump.
    let mut bytes = [0; 246];
    bytes[0] = 0xff;
    bytes[205] = 0x80;
    let (scaling, _) = scaling_of(&universal(0, EDIT, &scaling_bytes(&bytes)), STRICT);
    assert_eq!(scaling.operators[5].offset.value(), -1.0);
    assert_eq!(scaling.operators[0].offset.value(), -128.0);
}

#[test]
fn bytes_that_send_no_four_bits() {
    // OP6's offset's high half at 0x2F, below 0x30; its lowest group's low
    // half at 0x40 and the next group's at 0x7F, above 0x3F.
    let mut data = scaling_bytes(&[0x55; 246]);
    data[0] = 0x2f;
    data[3] = 0x40;
    data[5] = 0x7f;
    let sent = |scaling, place, byte| Finding::ScalingByte {
        scaling,
        parameter: parameter(place),
        byte,
    };
    let message = universal(0, EDIT, &data);
    let (scaling, findings) = scaling_of(&message, Options::default());
    assert_eq!(findings, [sent(0, 0, 0x2f), sent(0, 1, 0x40), sent(0, 2, 0x7f)]);
    assert_eq!(bytes_of(&scaling)[..4], [0x05, 0x5f, 0x5f, 0x55].map(f64::from));
    assert_eq!(syx::decode(&message, STRICT), Err(Error::Refused(findings[0])));
    // In a cartridge, by the place of its fractional scaling.
    let valid = scaling_bytes(&[0x55; 246]);
    let (_, findings) = scalings_of(&universal_groups(0, CARTRIDGE, &[valid, data]), Options::default());
    assert_eq!(findings, [sent(1, 0, 0x2f), sent(1, 1, 0x40), sent(1, 2, 0x7f)]);
}

#[test]
fn names_groups_and_lengths() {
    let mut random = SplitMix64(72);
    let data = scaling_bytes(&random_scaling_bytes(&mut random));
    // Two groups of the edit buffer; a cartridge of 65 groups, and one whose
    // groups' names differ.
    let one = universal_groups(0, CARTRIDGE, std::slice::from_ref(&data));
    let unsupported = [
        universal_groups(0, EDIT, &[data.clone(), data.clone()]),
        universal_groups(0, CARTRIDGE, &vec![data.clone(); 65]),
        [&one[..one.len() - 1], &universal(0, EDIT, &data)[4..]].concat(),
    ];
    for message in unsupported {
        assert_eq!(syx::decode(&message, Options::default()), Err(Error::Unsupported));
    }
    // A wrong checksum in a cartridge's second group.
    let mut message = universal_groups(0, CARTRIDGE, &vec![data.clone(); 3]);
    let at = 4 + 2 * 505 - 1;
    let right = message[at];
    message[at] = (right + 1) & 0x7f;
    let (_, findings) = scalings_of(&message, Options::default());
    let finding = Finding::UniversalChecksum {
        group: 1,
        expected: right,
        found: (right + 1) & 0x7f,
    };
    assert_eq!(findings, [finding]);
    // A byte of data too few and too many.
    for message in [
        universal(0, EDIT, &data[..491]),
        universal(0, EDIT, &[&data[..], &[0x30]].concat()),
    ] {
        assert_eq!(syx::decode(&message, Options::default()), Err(Error::Length));
    }
    let (written, _) = syx::write_fractional_scalings(&[], channel(0), Rounding::Nearest);
    assert_eq!(written, [0xf0, 0x43, 0x00, 0x7e, 0xf7]);
}

#[test]
fn changes_of_every_parameter() {
    for parameter in ScalingParameter::all() {
        let (operator, group) = (parameter.operator(), parameter.group());
        assert_eq!(ScalingParameter::of(operator, group), Some(parameter));
        for step in [0, 1, 127, 128, 255] {
            let change = Change::FractionalScaling(ScalingStep::new(parameter, step));
            let message = syx::write_change(channel(5), change);
            assert_eq!(
                message,
                [
                    0xf0,
                    0x43,
                    0x15,
                    0x18,
                    0x7f,
                    operator,
                    group,
                    step >> 7,
                    step & 0x7f,
                    0xf7
                ]
            );
            assert_eq!(
                syx::decode(&message, STRICT),
                Ok(Sysex::Change(ParameterChange {
                    channel: channel(5),
                    change,
                    finding: None,
                }))
            );
        }
    }
    // A step beyond 255, its high byte 2: reported, and taken as 255.
    let message = [0xf0, 0x43, 0x10, 0x18, 0x7f, 0x00, 0x05, 0x02, 0x00, 0xf7];
    let finding = Finding::ScalingOutOfRange {
        parameter: parameter(5),
        step: 256,
    };
    let expected = ParameterChange {
        channel: channel(0),
        change: Change::FractionalScaling(ScalingStep::new(parameter(5), 255)),
        finding: Some(finding),
    };
    assert_eq!(syx::decode(&message, Options::default()), Ok(Sysex::Change(expected)));
    assert_eq!(syx::decode(&message, STRICT), Err(Error::Refused(finding)));
}

#[test]
fn parameters_and_values_between_steps() {
    // The operators from OP6 at 0, and the groups from the offset at 0
    // \[DX7S-MANUAL\] p. Add-3.
    assert_eq!(ScalingParameter::of(0, 0), Some(parameter(0)));
    assert_eq!(ScalingParameter::of(5, 40), Some(parameter(245)));
    assert!(ScalingParameter::of(6, 0).is_none() && ScalingParameter::of(0, 41).is_none());
    assert!(ScalingParameter::new(246).is_none());
    assert_eq!((parameter(100).operator(), parameter(100).group()), (2, 18));
    let names = [
        (0, "OP6 offset"),
        (41, "OP5 offset"),
        (100, "OP4 key group 18"),
        (245, "OP1 key group 40"),
    ];
    for (place, name) in names {
        assert_eq!(parameter(place).to_string(), name);
    }
    // A level between steps is rounded and reported; an offset is a byte.
    let mut random = SplitMix64(73);
    let (mut scaling, _) = scaling_of(
        &universal(0, EDIT, &scaling_bytes(&random_scaling_bytes(&mut random))),
        STRICT,
    );
    scaling.operators[0].levels[39] = Value::new(100.5).expect("a level");
    for (rounding, step) in [(Rounding::Nearest, 100), (Rounding::Floor, 100)] {
        let (written, rounded) = syx::write_fractional_scaling(&scaling, channel(0), rounding);
        assert_eq!(rounded, [parameter(245)]);
        assert_eq!(written[16 + 490..16 + 492], [0x36, 0x34]);
        assert_eq!(ScalingStep::of(&scaling, parameter(245), rounding).step(), step);
    }
    assert!(scaling.is_step(parameter(0)) && !scaling.is_step(parameter(245)));
}
