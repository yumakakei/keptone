// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The TX802's own data, its performances and its system set-up, their
//! universal bulk dumps and the parameter changes of the performance and the
//! panel, against \[TX802-SYSEX\], whose layouts `common` states again by
//! itself: the edit buffer, `LM  8952PE`, is 116 bytes, each sent as two
//! hexadecimal digits; the memory, `LM  8952PM`, is 64 groups of 84 packed
//! bytes, each sent as two digits; their counts are of the data alone; the
//! system set-up, `LM  8952S `, is 263 bytes of seven bits. Each byte lands in
//! its field, random valid dumps come back byte for byte, what is out of range,
//! a set unused bit and a byte that is no digit are reported and taken as
//! `syx.out_of_range` says, what is of no kind or of the wrong length is
//! refused, and the parameter changes carry every parameter, the voice and the
//! micro tuning table in two bytes of data.

mod common;

use common::{
    SplitMix64, TX802_CONTROLS, checksum, hex_digits, pack_tx802, random_tx802_performance, random_tx802_system,
    tx802_performance_largest, tx802_system_takes, tx802_universal, universal,
};
use keptone_model::dx7ii::Index;
use keptone_model::syx::{
    self, Byte, Change, Channel, Content, Error, Finding, Options, OutOfRange, ParameterChange, Sysex,
    Tx802PerformanceStep, Tx802Switch,
};
use keptone_model::tx802::{ControlAssign, Output, Performance, PerformanceParameter, SystemParameter, SystemSetup};
use keptone_model::voice::{Rounding, Value};

/// The options that refuse whatever a valid dump does not hold.
const STRICT: Options = Options {
    accept_bad_checksum: false,
    out_of_range: OutOfRange::Reject,
};

/// The names of the performance edit buffer.
const EDIT: &[u8; 10] = b"LM  8952PE";

/// The names of the memory's performances.
const MEMORY: &[u8; 10] = b"LM  8952PM";

/// The names of the system set-up.
const SYSTEM: &[u8; 10] = b"LM  8952S ";

/// A channel.
fn channel(number: u8) -> Channel {
    Channel::new(number).expect("a channel")
}

/// A parameter of the performance.
fn parameter(number: u8) -> PerformanceParameter {
    PerformanceParameter::new(number).expect("a parameter")
}

/// The content of the dump a message is, and what it found.
fn content_of(message: &[u8], options: Options) -> (Content, Vec<Finding>) {
    match syx::decode(message, options) {
        Ok(Sysex::Dump(dump)) => (dump.content, dump.findings),
        other => panic!("not a dump: {other:?}"),
    }
}

/// The performance of an edit buffer's dump.
fn edit_of(message: &[u8], options: Options) -> (Performance, Vec<Finding>) {
    match content_of(message, options) {
        (Content::Tx802Performance(performance), findings) => (*performance, findings),
        other => panic!("not a performance: {other:?}"),
    }
}

/// The performances of the memory's dump.
fn memory_of(message: &[u8], options: Options) -> (Vec<Performance>, Vec<Finding>) {
    match content_of(message, options) {
        (Content::Tx802Performances(performances), findings) => (performances, findings),
        other => panic!("not the memory's performances: {other:?}"),
    }
}

/// The system set-up of its dump.
fn setup_of(message: &[u8], options: Options) -> (SystemSetup, Vec<Finding>) {
    match content_of(message, options) {
        (Content::Tx802SystemSetup(setup), findings) => (*setup, findings),
        other => panic!("not a system set-up: {other:?}"),
    }
}

/// The edit buffer's dump of 116 steps.
fn edit_message(on: u8, steps: &[u8; 116]) -> Vec<u8> {
    tx802_universal(on, EDIT, &[hex_digits(steps)])
}

/// The memory's dump of the steps of its performances.
fn memory_message(on: u8, performances: &[[u8; 116]]) -> Vec<u8> {
    let groups: Vec<Vec<u8>> = performances
        .iter()
        .map(|steps| hex_digits(&pack_tx802(steps)))
        .collect();
    tx802_universal(on, MEMORY, &groups)
}

/// The steps of a performance's parameters.
fn steps_of(performance: &Performance) -> Vec<f64> {
    PerformanceParameter::all()
        .map(|parameter| performance.value(parameter))
        .collect()
}

/// A value of a step within its range.
fn value<const MAX: u8>(step: u8) -> Value<MAX> {
    Value::from_step(step).expect("a step within its range")
}

/// An index within its range.
fn index<const MAX: u8>(number: u8) -> Index<MAX> {
    Index::new(number).expect("an index within its range")
}

#[test]
fn each_byte_lands_in_its_field() {
    // TG1 and TG8 of every parameter at a step of their own, the others 0.
    let mut steps = [0; 116];
    let first = [3, 16, 200, 14, 99, 2, 21, 108, 48, 1, 1, 254];
    let last = [7, 15, 255, 0, 1, 3, 0, 127, 0, 0, 1, 11];
    for kind in 0..12 {
        steps[kind * 8] = first[kind];
        steps[kind * 8 + 7] = last[kind];
    }
    steps[96..].copy_from_slice(b"TWENTY LETTERS NAMED");
    let (performance, findings) = edit_of(&edit_message(2, &steps), STRICT);
    assert_eq!(findings, []);
    let [one, .., eight] = &performance.tone_generators;
    assert_eq!(
        (one.channel_offset, one.receive_channel, one.voice),
        (index(3), index(16), index(200))
    );
    assert_eq!(
        (one.detune, one.volume, one.output),
        (value(14), value(99), Output::Second)
    );
    assert_eq!(
        (one.note_low, one.note_high, one.note_shift),
        (index(21), index(108), value(48))
    );
    assert_eq!(
        (one.forced_damp, one.key_assign_group, one.micro_tuning),
        (true, true, index(254))
    );
    assert_eq!(
        (eight.channel_offset, eight.receive_channel, eight.voice, eight.output),
        (index(7), index(15), index(255), Output::Both)
    );
    assert_eq!((eight.key_assign_group, eight.micro_tuning), (true, index(11)));
    assert_eq!(performance.name.bytes(), *b"TWENTY LETTERS NAMED");
    // The same steps packed into the memory's 84 bytes: the voice channel
    // offset with the receive channel, the detune with the key assign group
    // and the output, the forced damp with the note shift.
    let packed = pack_tx802(&steps);
    assert_eq!(
        (packed[0], packed[32], packed[56], packed[39]),
        (0x70, 0x76, 0x70, 0x07)
    );
    let (memory, findings) = memory_of(&memory_message(2, &[steps]), STRICT);
    assert_eq!((findings, memory), (Vec::new(), vec![performance]));
}

#[test]
fn dumps_come_back_byte_for_byte() {
    let mut random = SplitMix64(81);
    for round in 0..200_u8 {
        let on = round % 16;
        let steps = random_tx802_performance(&mut random);
        let message = edit_message(on, &steps);
        let (performance, findings) = edit_of(&message, STRICT);
        assert_eq!(findings, []);
        assert_eq!(steps_of(&performance), steps.map(f64::from));
        let written = syx::write_tx802_performance(&performance, channel(on), Rounding::Nearest);
        assert_eq!(written, (message, Vec::new()));
        let message = universal(on, SYSTEM, &random_tx802_system(&mut random));
        let (setup, findings) = setup_of(&message, STRICT);
        assert_eq!(findings, []);
        let written = syx::write_tx802_system_setup(&setup, channel(on), Rounding::Nearest);
        assert_eq!(written, (message, Vec::new()));
    }
    // The memory's 64, and a dump of one.
    for count in [64, 1] {
        let steps: Vec<[u8; 116]> = (0..count).map(|_| random_tx802_performance(&mut random)).collect();
        let message = memory_message(count % 16, &steps);
        let (performances, findings) = memory_of(&message, STRICT);
        assert_eq!(findings, []);
        for (performance, steps) in performances.iter().zip(&steps) {
            assert_eq!(steps_of(performance), steps.map(f64::from));
        }
        let written = syx::write_tx802_performances(&performances, channel(count % 16), Rounding::Nearest);
        assert_eq!(written, (message, Vec::new()));
    }
}

#[test]
fn steps_out_of_range_and_unused_bits() {
    // TG1's receive channel at 17, TG2's detune at 15, TG3's volume at 100,
    // TG4's micro tuning table at 255.
    let mut steps = [0; 116];
    steps[8] = 17;
    steps[25] = 15;
    steps[34] = 100;
    steps[91] = 255;
    let beyond = |performance, number, step| Finding::Tx802PerformanceOutOfRange {
        performance,
        parameter: parameter(number),
        step,
    };
    let message = edit_message(0, &steps);
    let (performance, findings) = edit_of(&message, Options::default());
    assert_eq!(
        findings,
        [
            beyond(0, 8, 17),
            beyond(0, 25, 15),
            beyond(0, 34, 100),
            beyond(0, 91, 255)
        ]
    );
    let tones = &performance.tone_generators;
    assert_eq!(
        (
            tones[0].receive_channel,
            tones[1].detune,
            tones[2].volume,
            tones[3].micro_tuning
        ),
        (index(16), value(14), value(99), index(254))
    );
    assert_eq!(syx::decode(&message, STRICT), Err(Error::Refused(findings[0])));
    // In the memory, the second performance's: the receive channel at 31, the
    // note shift at 63, and the high bits, which are unused, of the first
    // volume's byte and of the last byte of the forced damp and note shift,
    // the first and the last that have one.
    let mut packed = pack_tx802(&[0; 116]);
    packed[0] = 31;
    packed[56] = 63;
    packed[24] = 0x80;
    packed[63] = 0x80 | 0x18;
    let zero = hex_digits(&pack_tx802(&[0; 116]));
    let message = tx802_universal(0, MEMORY, &[zero, hex_digits(&packed)]);
    let (performances, findings) = memory_of(&message, Options::default());
    let unused = |byte| Finding::Tx802UnusedBits {
        performance: 1,
        byte,
        bits: 0x80,
    };
    assert_eq!(findings, [unused(24), unused(63), beyond(1, 8, 31), beyond(1, 64, 63)]);
    assert_eq!(performances[1].tone_generators[7].note_shift, value(24));
    assert_eq!(syx::decode(&message, STRICT), Err(Error::Refused(findings[0])));
}

#[test]
fn bytes_that_are_no_digits() {
    // The first byte's digits as 'a' and ':', ten each way; the second's below
    // '0' and beyond 'F'.
    let mut digits = hex_digits(&[0; 116]);
    digits[..4].copy_from_slice(b"a:/G");
    let digit = |value, byte| Finding::Tx802Digit {
        performance: 0,
        value,
        byte,
    };
    let message = tx802_universal(0, EDIT, &[digits]);
    let (performance, findings) = edit_of(&message, Options::default());
    // So read, the bytes are 0xAA and 0x0F, each beyond the channel offset's
    // 7, and reported again as such.
    let beyond = |number, step| Finding::Tx802PerformanceOutOfRange {
        performance: 0,
        parameter: parameter(number),
        step,
    };
    assert_eq!(
        findings,
        [
            digit(0, b'a'),
            digit(0, b':'),
            digit(1, b'/'),
            digit(1, b'G'),
            beyond(0, 0xaa),
            beyond(1, 0x0f)
        ]
    );
    assert_eq!(
        (performance.value(parameter(0)), performance.value(parameter(1))),
        (7.0, 7.0)
    );
    assert_eq!(syx::decode(&message, STRICT), Err(Error::Refused(findings[0])));
}

#[test]
fn counts_names_and_lengths() {
    let mut random = SplitMix64(82);
    let steps = random_tx802_performance(&mut random);
    let digits = hex_digits(&steps);
    // Counted with the names, as the DX7II counts, the edit buffer is of no
    // length; so are a byte of data too few or too many, and the system
    // set-up's 262 and 264 bytes.
    let lengths = [
        universal(0, EDIT, &digits),
        tx802_universal(0, EDIT, &[digits[..231].to_vec()]),
        tx802_universal(0, EDIT, &[[&digits[..], b"0"].concat()]),
        universal(0, SYSTEM, &random_tx802_system(&mut random)[..262]),
        universal(0, SYSTEM, &[&random_tx802_system(&mut random)[..], &[0]].concat()),
    ];
    for message in lengths {
        assert_eq!(syx::decode(&message, Options::default()), Err(Error::Length));
    }
    // Two edit buffers in one message, and 65 performances, are of no kind
    // read.
    let packed = hex_digits(&pack_tx802(&steps));
    let unsupported = [
        tx802_universal(0, EDIT, &[digits.clone(), digits.clone()]),
        tx802_universal(0, MEMORY, &vec![packed.clone(); 65]),
    ];
    for message in unsupported {
        assert_eq!(syx::decode(&message, Options::default()), Err(Error::Unsupported));
    }
    // A wrong checksum in the memory's third group.
    let mut message = tx802_universal(0, MEMORY, &vec![packed; 4]);
    let at = 4 + 3 * 181 - 1;
    let right = message[at];
    message[at] = (right + 1) & 0x7f;
    let (_, findings) = memory_of(&message, Options::default());
    let finding = Finding::UniversalChecksum {
        group: 2,
        expected: right,
        found: (right + 1) & 0x7f,
    };
    assert_eq!(findings, [finding]);
    // The checksum is of the names and the data.
    let message = edit_message(0, &steps);
    let counted = [&EDIT[..], &digits].concat();
    assert_eq!(message[message.len() - 2], checksum(&counted));
}

#[test]
fn each_byte_of_the_system_setup() {
    let mut steps = vec![5, 17, 1, 16, 17, 0, 2, 1, 15, 2, 3, 64, 1];
    steps.extend((0..122).map(|entry| TX802_CONTROLS[entry % 8]));
    steps.extend((0..128).map(|entry| 127 - entry));
    let (setup, findings) = setup_of(&universal(9, SYSTEM, &steps), STRICT);
    assert_eq!(findings, []);
    assert_eq!(
        (setup.performance_channel, setup.device_number, setup.voice_block),
        (index(5), index(17), true)
    );
    assert_eq!(
        (setup.program_changes, setup.aftertouch, setup.pitch_bend, setup.notes),
        (index(16), index(17), index(0), index(2))
    );
    assert_eq!(
        (setup.program_table_switch, setup.banks, setup.master_tune),
        (true, [index(15), index(2), index(3)], value(64))
    );
    assert_eq!(setup.control_changes, index(1));
    assert_eq!(
        setup.control_table[..8],
        [
            ControlAssign::Off,
            ControlAssign::ModulationWheel,
            ControlAssign::Breath,
            ControlAssign::Foot,
            ControlAssign::PortamentoTime,
            ControlAssign::Volume,
            ControlAssign::Sustain,
            ControlAssign::Portamento
        ]
    );
    assert_eq!(
        (setup.program_table[0], setup.program_table[127]),
        (index(127), index(0))
    );
    // A channel of 18, a note switch of 3 and an entry of 3: reported, the
    // entry taken as none.
    steps[0] = 18;
    steps[6] = 3;
    steps[13 + 7] = 3;
    let (setup, findings) = setup_of(&universal(0, SYSTEM, &steps), Options::default());
    let outside = |byte, step| Finding::Tx802SystemOutOfRange {
        parameter: SystemParameter::new(byte).expect("a parameter"),
        step,
    };
    assert_eq!(findings, [outside(0, 18), outside(6, 3), outside(20, 3)]);
    assert_eq!(
        (setup.performance_channel, setup.notes, setup.control_table[7]),
        (index(17), index(2), ControlAssign::Off)
    );
    for byte in 0..263 {
        for step in 0..0x80 {
            assert_eq!(
                {
                    let mut setup = setup.clone();
                    setup.set_step(SystemParameter::new(byte).expect("a parameter"), step)
                },
                tx802_system_takes(usize::from(byte), step),
                "byte {byte} step {step}"
            );
        }
    }
}

#[test]
fn changes_of_every_parameter_and_switch() {
    for parameter in PerformanceParameter::all() {
        let number = parameter.number();
        let largest = tx802_performance_largest(usize::from(number));
        assert_eq!(parameter.largest(), largest, "{parameter}");
        let wide = matches!(number, 16..=23 | 88..=95);
        assert_eq!(parameter.is_wide(), wide);
        for step in [0, 1, largest.min(0x7f), largest] {
            let Some(step) = Tx802PerformanceStep::new(parameter, step) else {
                // A name byte beyond 127, which no parameter change carries.
                assert!(!wide && step >= 0x80);
                continue;
            };
            let change = Change::Tx802Performance(step);
            let message = syx::write_change(channel(6), change);
            let data: Vec<u8> = if wide {
                vec![step.step() >> 7, step.step() & 0x7f]
            } else {
                vec![step.step()]
            };
            assert_eq!(
                message,
                [&[0xf0, 0x43, 0x16, 0x1a, number][..], &data, &[0xf7]].concat()
            );
            let decoded = ParameterChange {
                channel: channel(6),
                change,
                finding: None,
            };
            assert_eq!(syx::decode(&message, STRICT), Ok(Sysex::Change(decoded)));
        }
    }
    // The voice at 256 and the micro tuning table at 255: reported, and taken
    // as their largest.
    let cases = [
        ([0xf0, 0x43, 0x10, 0x1a, 16, 0x02, 0x00, 0xf7], 16, 256, 255),
        ([0xf0, 0x43, 0x10, 0x1a, 95, 0x01, 0x7f, 0xf7], 95, 255, 254),
    ];
    for (message, number, step, largest) in cases {
        let finding = Finding::Tx802PerformanceOutOfRange {
            performance: 0,
            parameter: parameter(number),
            step,
        };
        let change = Change::Tx802Performance(Tx802PerformanceStep::new(parameter(number), largest).expect("a step"));
        let decoded = ParameterChange {
            channel: Channel::default(),
            change,
            finding: Some(finding),
        };
        assert_eq!(syx::decode(&message, Options::default()), Ok(Sysex::Change(decoded)));
        assert_eq!(syx::decode(&message, STRICT), Err(Error::Refused(finding)));
    }
    // The voice with one byte of data and the volume with two are of no
    // length.
    for message in [
        &[0xf0, 0x43, 0x10, 0x1a, 16, 0x05, 0xf7][..],
        &[0xf0, 0x43, 0x10, 0x1a, 32, 0x00, 0x05, 0xf7],
    ] {
        assert_eq!(syx::decode(message, Options::default()), Err(Error::Length));
    }
    // The panel's switches, 64 to 96, with any data, which is kept.
    for number in 64..=96 {
        for data in [0, 0x2a, 0x7f] {
            let switch = Tx802Switch::new(number).expect("a switch");
            let change = Change::Tx802Switch(switch, Byte::new(data).expect("a byte"));
            let message = syx::write_change(channel(1), change);
            assert_eq!(message, [0xf0, 0x43, 0x11, 0x1b, number, data, 0xf7]);
            let decoded = ParameterChange {
                channel: channel(1),
                change,
                finding: None,
            };
            assert_eq!(syx::decode(&message, STRICT), Ok(Sysex::Change(decoded)));
        }
    }
    assert!(Tx802Switch::new(63).is_none() && Tx802Switch::new(97).is_none());
}

#[test]
fn names_and_values_between_steps() {
    let names = [
        (0, "TG1 voice channel offset"),
        (17, "TG2 voice"),
        (95, "TG8 micro tuning table"),
        (96, "name byte 1"),
        (115, "name byte 20"),
    ];
    for (number, name) in names {
        assert_eq!(parameter(number).to_string(), name);
    }
    let system = [
        (0, "performance select channel"),
        (11, "master tune"),
        (13, "control table entry 0"),
        (262, "program table entry 127"),
    ];
    for (byte, name) in system {
        assert_eq!(SystemParameter::new(byte).expect("a parameter").to_string(), name);
    }
    assert!(PerformanceParameter::new(116).is_none() && SystemParameter::new(263).is_none());
    // Values between steps are rounded and reported.
    let mut random = SplitMix64(83);
    let (mut performance, _) = edit_of(&edit_message(0, &random_tx802_performance(&mut random)), STRICT);
    performance.tone_generators[2].volume = Value::new(80.5).expect("a volume");
    let (written, rounded) = syx::write_tx802_performance(&performance, channel(0), Rounding::Floor);
    assert_eq!(rounded, [parameter(34)]);
    assert_eq!(written[16 + 2 * 34..16 + 2 * 35], *b"50");
    let step = Tx802PerformanceStep::of(&performance, parameter(34), Rounding::Nearest);
    assert_eq!(step.step(), 80);
    let (mut setup, _) = setup_of(&universal(0, SYSTEM, &random_tx802_system(&mut random)), STRICT);
    setup.master_tune = Value::new(63.5).expect("a tune");
    let (written, rounded) = syx::write_tx802_system_setup(&setup, channel(0), Rounding::Nearest);
    assert_eq!(rounded, [SystemParameter::new(11).expect("a parameter")]);
    assert_eq!(written[16 + 11], 64);
}
