// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The DX7II's universal bulk dumps of its performances and its system
//! set-up, and their parameter changes, against \[DX7II-MANUAL\] and
//! \[DX7S-MANUAL\], whose layouts `common` states again by itself: a universal
//! bulk dump's group is its byte count, the names `LM  ` and `8973PE`,
//! `8973PM` or `8973S `, the data the count counts with the names, and the
//! checksum of the names and the data; the performance is 51 bytes, a
//! parameter each, 32 of them one after another, and the system set-up 102,
//! the DX7s's 85, without the 17 bytes that have no name.
//! Each byte lands in its field, random valid dumps come back byte for byte,
//! what is out of range is reported and taken as `syx.out_of_range` says, a
//! wrong checksum as `syx.accept_bad_checksum` says, and what is of no kind or
//! of the wrong length is refused; every parameter that a parameter change
//! carries comes back at every step, and the DX7s's system reset with its
//! data.

mod common;

use common::{SplitMix64, random_dx7ii_performance, random_dx7s_system_setup, random_system_setup, universal};
use keptone_model::dx7ii::{
    Channels, FootSwitchAssign, Index, KeyFilter, PanAssign, PanMode, Performance, PerformanceParameter, PlayMode,
    ProgramChangeMode, Protect, SliderSwitches, SystemParameter, SystemSetup,
};
use keptone_model::syx::{
    self, Byte, Change, Channel, Content, Dx7iiPerformanceStep, Error, Finding, Options, OutOfRange, ParameterChange,
    Sysex, SystemStep,
};
use keptone_model::voice::{Rounding, Value};

/// The options that refuse whatever a valid dump does not hold.
const STRICT: Options = Options {
    accept_bad_checksum: false,
    out_of_range: OutOfRange::Reject,
};

/// The names of the performance edit buffer.
const EDIT: &[u8; 10] = b"LM  8973PE";

/// The names of the 32 performances.
const MEMORY: &[u8; 10] = b"LM  8973PM";

/// The names of the system set-up.
const SYSTEM: &[u8; 10] = b"LM  8973S ";

/// A channel.
fn channel(number: u8) -> Channel {
    Channel::new(number).expect("a channel")
}

/// The content of the dump a message is, and what it found.
fn content_of(message: &[u8], options: Options) -> (Content, Vec<Finding>) {
    match syx::decode(message, options) {
        Ok(Sysex::Dump(dump)) => (dump.content, dump.findings),
        other => panic!("not a dump: {other:?}"),
    }
}

/// The performance of an edit buffer's dump.
fn performance_of(message: &[u8], options: Options) -> (Performance, Vec<Finding>) {
    match content_of(message, options) {
        (Content::Dx7iiPerformance(performance), findings) => (*performance, findings),
        other => panic!("not a performance: {other:?}"),
    }
}

/// The system set-up of its dump.
fn setup_of(message: &[u8], options: Options) -> (SystemSetup, Vec<Finding>) {
    match content_of(message, options) {
        (Content::SystemSetup(setup), findings) => (*setup, findings),
        other => panic!("not a system set-up: {other:?}"),
    }
}

/// The parameter change that a message decodes to, refusing what is not
/// valid.
fn change_in(message: &[u8]) -> ParameterChange {
    match syx::decode(message, STRICT) {
        Ok(Sysex::Change(change)) => change,
        other => panic!("not a parameter change: {other:?}"),
    }
}

/// The value of a step within its range.
fn value<const MAX: u8>(step: u8) -> Value<MAX> {
    Value::from_step(step).expect("a step within its range")
}

/// An index within its range.
fn index<const MAX: u8>(number: u8) -> Index<MAX> {
    Index::new(number).expect("an index within its range")
}

#[test]
fn each_byte_of_a_performance_lands_in_its_field() {
    let mut steps = vec![
        2, 100, 70, 74, 11, 2, 7, 60, 1, 1, 2, 3, 5, 30, 12, 75, 98, 105, 109, 9, 3, 44, 1, 90, 80, 70, 60, 10, 20, 30,
        40,
    ];
    steps.extend_from_slice(b"TWENTY LETTERS NAMED");
    let (performance, findings) = performance_of(&universal(0, EDIT, &steps), STRICT);
    assert_eq!(findings, []);
    assert_eq!(performance.play_mode, PlayMode::Split);
    assert_eq!((performance.voice_a, performance.voice_b), (index(100), index(70)));
    assert_eq!(
        (performance.micro_tuning_table, performance.micro_tuning_key),
        (index(74), index(11))
    );
    assert_eq!(performance.micro_tuning, Channels { a: false, b: true });
    assert_eq!(performance.dual_detune, value(7));
    assert_eq!(performance.split_point, index(60));
    assert!(performance.eg_forced_damp);
    assert_eq!(performance.sustain, Channels { a: true, b: false });
    assert_eq!(performance.foot_switch_assign, FootSwitchAssign::KeyHold);
    assert_eq!(performance.foot_switch, Channels { a: true, b: true });
    assert_eq!(performance.soft_pedal_range, value(5));
    assert_eq!(
        (performance.note_shift_a, performance.note_shift_b),
        (value(30), value(12))
    );
    assert_eq!((performance.balance, performance.total_volume), (value(75), value(98)));
    assert_eq!((performance.slider_1, performance.slider_2), (index(105), index(109)));
    let switches = SliderSwitches {
        slider_1_a: true,
        slider_1_b: false,
        slider_2_a: false,
        slider_2_b: true,
    };
    assert_eq!(performance.slider_switches, switches);
    assert_eq!(performance.pan_mode, PanMode::OffOn);
    assert_eq!(performance.pan_range, value(44));
    assert_eq!(performance.pan_assign, PanAssign::Velocity);
    assert_eq!(performance.pan_envelope.rates, [90, 80, 70, 60].map(value));
    assert_eq!(performance.pan_envelope.levels, [10, 20, 30, 40].map(value));
    assert_eq!(performance.name.bytes(), *b"TWENTY LETTERS NAMED");
}

#[test]
fn each_byte_of_a_system_setup_lands_in_its_field() {
    let mut steps = vec![9, 1, 16, 3, 1, 9, 31, 5, 20, 2, 2, 1, 0, 1, 15, 1, 4, 5, 6, 3];
    steps.extend(40..57);
    steps.push(64);
    steps.extend((0..64).map(|entry| 127 - entry));
    let (setup, findings) = setup_of(&universal(5, SYSTEM, &steps), STRICT);
    assert_eq!(findings, []);
    assert_eq!((setup.transmit_channel, setup.channel_messages), (index(9), true));
    assert_eq!(
        (setup.receive_channel_a, setup.receive_channel_b),
        (index(16), index(3))
    );
    assert!(setup.omni);
    assert_eq!((setup.controller_a, setup.controller_b), (index(9), index(31)));
    assert_eq!(
        (setup.slider_1_controller, setup.slider_2_controller),
        (index(5), index(20))
    );
    assert_eq!(
        (setup.key_filter, setup.program_change_mode),
        (KeyFilter::Even, ProgramChangeMode::Programmable)
    );
    assert_eq!(
        (setup.local, setup.transmit_block, setup.receive_block),
        (true, false, true)
    );
    assert_eq!((setup.system_channel, setup.system_messages), (index(15), true));
    assert_eq!(setup.cartridge_banks, [index(4), index(5), index(6)]);
    let protect = Protect {
        internal: true,
        cartridge: true,
    };
    assert_eq!(setup.protect, protect);
    assert_eq!(setup.unnamed.bytes().as_slice(), (40..57).collect::<Vec<u8>>());
    assert_eq!(setup.master_tune, value(64));
    assert_eq!(setup.program_changes[0], index(127));
    assert_eq!(setup.program_changes[63], index(64));
}

#[test]
fn the_dx7s_system_setup() {
    // The DX7II's bytes but for the 17 that have no name: the master tune at
    // 20 and the table from 21 \[DX7S-MANUAL\] p. Add-14.
    let mut steps = vec![9, 1, 16, 3, 1, 9, 31, 5, 20, 2, 2, 1, 0, 1, 15, 1, 4, 5, 6, 3, 64];
    steps.extend((0..64).map(|entry| 127 - entry));
    let (Content::Dx7sSystemSetup(setup), findings) = content_of(&universal(5, SYSTEM, &steps), STRICT) else {
        panic!("not the DX7s's system set-up");
    };
    assert_eq!(findings, []);
    let dx7ii = [&steps[..20], &[0; 17], &steps[20..]].concat();
    assert_eq!(*setup, setup_of(&universal(5, SYSTEM, &dx7ii), STRICT).0);
    assert_eq!(setup.master_tune, value(64));
    assert_eq!(
        (setup.program_changes[0], setup.program_changes[63]),
        (index(127), index(64))
    );
    let order: Vec<u8> = SystemParameter::in_dx7s_dump().map(SystemParameter::byte).collect();
    assert_eq!(order, (0..20).chain(37..102).collect::<Vec<u8>>());
    // Random ones come back byte for byte.
    let mut random = SplitMix64(55);
    for round in 0..200_u8 {
        let on = round % 16;
        let message = universal(on, SYSTEM, &random_dx7s_system_setup(&mut random));
        let (Content::Dx7sSystemSetup(setup), findings) = content_of(&message, STRICT) else {
            panic!("not the DX7s's system set-up");
        };
        assert_eq!(findings, []);
        let written = syx::write_dx7s_system_setup(&setup, channel(on), Rounding::Nearest);
        assert_eq!(written, (message, Vec::new()));
    }
    // Written from the DX7II's, it reports the bytes without a name that are
    // not 0, which it has no place for, and a value between steps.
    let mut steps = random_system_setup(&mut random);
    steps[20..37].fill(0);
    steps[20] = 5;
    steps[36] = 1;
    let (mut setup, _) = setup_of(&universal(0, SYSTEM, &steps), STRICT);
    setup.master_tune = Value::new(63.5).expect("a tune");
    let (written, lost) = syx::write_dx7s_system_setup(&setup, channel(0), Rounding::Nearest);
    let parameter = |byte| SystemParameter::new(byte).expect("a parameter");
    assert_eq!(lost, [parameter(20), parameter(36), parameter(37)]);
    assert_eq!(written.len(), 103);
    assert_eq!(written[16..36], steps[..20]);
    assert_eq!(written[16 + 20], 64);
    assert_eq!(written[16 + 21..16 + 85], steps[38..]);
}

#[test]
fn dumps_come_back_byte_for_byte() {
    let mut random = SplitMix64(51);
    for round in 0..200_u8 {
        let on = round % 16;
        let message = universal(on, EDIT, &random_dx7ii_performance(&mut random));
        let (performance, findings) = performance_of(&message, STRICT);
        assert_eq!(findings, []);
        let written = syx::write_dx7ii_performance(&performance, channel(on), Rounding::Nearest);
        assert_eq!(written, (message, Vec::new()));
        let message = universal(on, SYSTEM, &random_system_setup(&mut random));
        let (setup, findings) = setup_of(&message, STRICT);
        assert_eq!(findings, []);
        let written = syx::write_system_setup(&setup, channel(on), Rounding::Nearest);
        assert_eq!(written, (message, Vec::new()));
    }
    for round in 0..20_u8 {
        let on = round % 16;
        let steps: Vec<Vec<u8>> = (0..32).map(|_| random_dx7ii_performance(&mut random)).collect();
        let message = universal(on, MEMORY, &steps.concat());
        let (Content::Dx7iiPerformances(performances), findings) = content_of(&message, STRICT) else {
            panic!("not 32 performances");
        };
        assert_eq!(findings, []);
        // Each is the performance of its 51 bytes alone.
        for (steps, performance) in steps.iter().zip(performances.iter()) {
            assert_eq!(performance_of(&universal(on, EDIT, steps), STRICT).0, *performance);
        }
        let written = syx::write_dx7ii_performances(&performances, channel(on), Rounding::Nearest);
        assert_eq!(written, (message, Vec::new()));
    }
}

#[test]
fn steps_out_of_range() {
    // A performance's play mode at 3, its micro tuning table at 75, its second
    // slider at 110 and its slider switches at 16.
    let mut steps = vec![0; 51];
    steps[0] = 3;
    steps[3] = 75;
    steps[18] = 110;
    steps[19] = 16;
    let message = universal(0, EDIT, &steps);
    let (performance, findings) = performance_of(&message, Options::default());
    let beyond = |number, step| Finding::Dx7iiPerformanceOutOfRange {
        performance: 0,
        parameter: PerformanceParameter::new(number).expect("a parameter"),
        step,
    };
    assert_eq!(findings, [beyond(0, 3), beyond(3, 75), beyond(18, 110), beyond(19, 16)]);
    assert_eq!(performance.play_mode, PlayMode::Split);
    assert_eq!(performance.slider_2, index(109));
    assert_eq!(syx::decode(&message, STRICT), Err(Error::Refused(findings[0])));
    // A system set-up's first controller at 8, below its range, its first
    // slider's at 4, its receive channel A at 17 and its protect at 4.
    let mut steps = random_system_setup(&mut SplitMix64(52));
    steps[2] = 17;
    steps[5] = 8;
    steps[7] = 4;
    steps[19] = 4;
    let message = universal(0, SYSTEM, &steps);
    let (setup, findings) = setup_of(&message, Options::default());
    let outside = |byte, step| Finding::SystemOutOfRange {
        parameter: SystemParameter::new(byte).expect("a parameter"),
        step,
    };
    assert_eq!(findings, [outside(2, 17), outside(5, 8), outside(7, 4), outside(19, 4)]);
    assert_eq!(
        (setup.receive_channel_a, setup.controller_a, setup.slider_1_controller),
        (index(16), index(9), index(5))
    );
    assert_eq!(syx::decode(&message, STRICT), Err(Error::Refused(findings[0])));
}

#[test]
fn checksums_and_messages_of_no_kind() {
    let mut random = SplitMix64(53);
    let mut message = universal(0, EDIT, &random_dx7ii_performance(&mut random));
    let at = message.len() - 2;
    let right = message[at];
    message[at] = (right + 1) & 0x7f;
    let (_, findings) = performance_of(&message, Options::default());
    let finding = Finding::UniversalChecksum {
        group: 0,
        expected: right,
        found: (right + 1) & 0x7f,
    };
    assert_eq!(findings, [finding]);
    let refusing = Options {
        accept_bad_checksum: false,
        ..Options::default()
    };
    assert_eq!(syx::decode(&message, refusing), Err(Error::Refused(finding)));
    // Another classification, another data format, and two groups in one
    // message are of no kind read.
    let steps = random_dx7ii_performance(&mut random);
    for names in [b"LM  8976AE", b"LN  8973PE", b"LM  8952PE"] {
        assert_eq!(
            syx::decode(&universal(0, names, &steps), Options::default()),
            Err(Error::Unsupported)
        );
    }
    let one = universal(0, EDIT, &steps);
    let two = [&one[..one.len() - 1], &one[4..]].concat();
    assert_eq!(syx::decode(&two, Options::default()), Err(Error::Unsupported));
    // No group, a count below the names', a count beyond the bytes, a byte of
    // data too few or too many for the names, and for both system set-ups.
    let lengths: [Vec<u8>; 7] = [
        vec![0xf0, 0x43, 0x00, 0x7e, 0xf7],
        [&[0xf0, 0x43, 0x00, 0x7e, 0x00, 0x09][..], &EDIT[..9], &[0x00, 0xf7]].concat(),
        [&one[..4], &[0x7f, 0x7f], &one[6..]].concat(),
        universal(0, EDIT, &steps[..50]),
        universal(0, SYSTEM, &[0; 84]),
        universal(0, SYSTEM, &[0; 86]),
        universal(0, SYSTEM, &[0; 103]),
    ];
    for message in lengths {
        assert_eq!(
            syx::decode(&message, Options::default()),
            Err(Error::Length),
            "{message:02x?}"
        );
    }
}

#[test]
fn performance_and_system_changes_at_every_step() {
    for parameter in PerformanceParameter::all() {
        let number = parameter.number();
        let largest = common::dx7ii_performance_largest(usize::from(number));
        assert_eq!(parameter.largest(), largest, "{parameter}");
        for step in 0..=largest {
            let change = Change::Dx7iiPerformance(Dx7iiPerformanceStep::new(parameter, step).expect("a step"));
            let message = syx::write_change(channel(3), change);
            assert_eq!(message, [0xf0, 0x43, 0x13, 0x19, number, step, 0xf7]);
            assert_eq!(change_in(&message).change, change);
        }
    }
    for byte in 0..20 {
        let parameter = SystemParameter::new(byte).expect("a parameter");
        let (smallest, largest) = common::system_range(usize::from(byte));
        assert_eq!(
            (parameter.smallest(), parameter.largest()),
            (smallest, largest),
            "{parameter}"
        );
        assert_eq!(parameter.change_number(), Some(64 + byte));
        for step in smallest..=largest {
            let change = Change::System(SystemStep::new(parameter, step).expect("a step"));
            let message = syx::write_change(channel(4), change);
            assert_eq!(message, [0xf0, 0x43, 0x14, 0x19, 64 + byte, step, 0xf7]);
            assert_eq!(change_in(&message).change, change);
        }
    }
    // The bytes from 20 have no parameter change.
    let master = SystemParameter::new(37).expect("a parameter");
    assert_eq!(master.change_number(), None);
    assert!(SystemStep::new(master, 64).is_none());
    // A controller's number below its range: reported, and taken as 9.
    let message = [0xf0, 0x43, 0x10, 0x19, 69, 3, 0xf7];
    let decoded = syx::decode(&message, Options::default());
    let parameter = SystemParameter::new(5).expect("a parameter");
    let expected = ParameterChange {
        channel: Channel::default(),
        change: Change::System(SystemStep::new(parameter, 9).expect("a step")),
        finding: Some(Finding::SystemOutOfRange { parameter, step: 3 }),
    };
    assert_eq!(decoded, Ok(Sysex::Change(expected)));
}

#[test]
fn the_dx7s_system_reset() {
    // The switch remote's number 46, its data ignored and kept \[DX7S-MANUAL\]
    // p. Add-7.
    for data in 0..128 {
        let message = [0xf0, 0x43, 0x10, 0x1b, 0x2e, data, 0xf7];
        let decoded = change_in(&message);
        let reset = Change::SystemReset(Byte::new(data).expect("a byte"));
        assert_eq!((decoded.change, decoded.finding), (reset, None));
        assert_eq!(syx::write_change(Channel::default(), reset), message);
    }
}

#[test]
fn names_and_values_between_steps() {
    let names = [
        (0, "play mode"),
        (17, "continuous slider 1"),
        (30, "pan EG level 4"),
        (31, "name byte 1"),
        (50, "name byte 20"),
    ];
    for (number, name) in names {
        assert_eq!(
            PerformanceParameter::new(number).expect("a parameter").to_string(),
            name
        );
    }
    let system = [
        (0, "transmit channel"),
        (19, "memory protect"),
        (20, "system byte 20"),
        (37, "master tune"),
        (101, "program change table entry 64"),
    ];
    for (byte, name) in system {
        assert_eq!(SystemParameter::new(byte).expect("a parameter").to_string(), name);
    }
    assert!(PerformanceParameter::new(51).is_none() && SystemParameter::new(102).is_none());
    assert_eq!(SystemParameter::of_change(64), SystemParameter::new(0));
    assert!(SystemParameter::of_change(84).is_none() && SystemParameter::of_change(63).is_none());
    // Values between steps are rounded and reported.
    let mut random = SplitMix64(54);
    let (mut performance, _) = performance_of(&universal(0, EDIT, &random_dx7ii_performance(&mut random)), STRICT);
    performance.total_volume = Value::new(80.5).expect("a volume");
    let (written, rounded) = syx::write_dx7ii_performance(&performance, channel(0), Rounding::Floor);
    let volume = PerformanceParameter::new(16).expect("a parameter");
    assert_eq!(rounded, [(0, volume)]);
    assert_eq!(written[16 + 16], 80);
    let (mut setup, _) = setup_of(&universal(0, SYSTEM, &random_system_setup(&mut random)), STRICT);
    setup.master_tune = Value::new(63.5).expect("a tune");
    let (written, rounded) = syx::write_system_setup(&setup, channel(0), Rounding::Nearest);
    assert_eq!(rounded, [SystemParameter::new(37).expect("a parameter")]);
    assert_eq!(written[16 + 37], 64);
}
