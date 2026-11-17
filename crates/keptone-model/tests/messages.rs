// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The parameter changes and dump requests against \[DEXED-SYSEX\],
//! \[DX7-MANUAL\], \[DX7II-MANUAL\], \[TX7-MANUAL\] and \[AJXS-ROM\], whose
//! bytes this file states again by itself: a parameter change is F0 43 1n, a
//! group byte 0gggggpp, the parameter's number's low seven bits and one byte of
//! data, F7; the voice group is 0, with the TX7's operator select at 156; the
//! DX performance parameter change, which the TX7 receives, is group 1; the
//! function group 2, which also carries the DX7's panel buttons 0 to 41; the
//! TX function parameter change group 4 with the group byte's low bits 1; and
//! the DX7II's switch remote is the group byte 0x1B. A dump request is F0 43
//! 2n, a format and F7, or 0x7E and ten letters of names for a universal bulk
//! dump. Every parameter at every step, every button and switch, and every
//! request on every channel come back byte for byte; what is out of range is
//! reported and taken as `syx.out_of_range` says; and what is of no kind the
//! reader takes is refused.

mod common;

use common::FUNCTION_LARGEST;
use keptone_model::function::FunctionParameter;
use keptone_model::performance::DxPerformanceParameter;
use keptone_model::syx::{
    self, Button, Change, Channel, Error, Finding, FunctionStep, OperatorSelect, Options, OutOfRange, ParameterChange,
    PerformanceStep, RemoteSwitch, Request, Requested, Sysex, TxFunctionParameter, TxStep, UniversalName, VoiceStep,
};
use keptone_model::voice::{Parameter, Rounding, Value};

/// The options that refuse whatever a valid message does not hold.
const STRICT: Options = Options {
    accept_bad_checksum: false,
    out_of_range: OutOfRange::Reject,
};

/// A channel.
fn channel(number: u8) -> Channel {
    Channel::new(number).expect("a channel")
}

/// The parameter change that a message decodes to, refusing what is not
/// valid.
fn change_in(message: &[u8]) -> ParameterChange {
    match syx::decode(message, STRICT) {
        Ok(Sysex::Change(change)) => change,
        other => panic!("not a parameter change: {other:?}"),
    }
}

#[test]
fn voice_parameters_at_every_step() {
    for parameter in Parameter::all() {
        let number = parameter.number();
        let largest = common::largest_in_change(usize::from(number));
        for step in 0..=largest {
            let on = channel(number % 16);
            let change = Change::Voice(VoiceStep::new(parameter, step).expect("a step within its range"));
            let message = syx::write_change(on, change);
            assert_eq!(
                message,
                [0xf0, 0x43, 0x10 | on.get(), number >> 7, number & 0x7f, step, 0xf7]
            );
            let decoded = change_in(&message);
            assert_eq!((decoded.channel, decoded.change, decoded.finding), (on, change, None));
        }
        assert!(VoiceStep::new(parameter, largest + 1).is_none());
    }
    assert_eq!(Parameter::all().count(), 156);
}

#[test]
fn function_parameters_at_every_step() {
    assert_eq!(
        FunctionParameter::all()
            .map(FunctionParameter::largest)
            .collect::<Vec<_>>(),
        FUNCTION_LARGEST
    );
    for parameter in FunctionParameter::all() {
        for step in 0..=parameter.largest() {
            let change = Change::Function(FunctionStep::new(parameter, step).expect("a step within its range"));
            let message = syx::write_change(channel(15), change);
            assert_eq!(message, [0xf0, 0x43, 0x1f, 0x08, parameter.number(), step, 0xf7]);
            assert_eq!(change_in(&message).change, change);
        }
    }
}

#[test]
fn buttons_and_switches() {
    let names: Vec<String> = (0..42)
        .map(|number| Button::new(number).expect("a button").to_string())
        .collect();
    assert_eq!(names[0], "1");
    assert_eq!(names[31], "32");
    assert_eq!(
        names[32..],
        [
            "STORE",
            "MEMORY PROTECT INTERNAL",
            "MEMORY PROTECT CARTRIDGE",
            "OPERATOR SELECT",
            "EDIT/COMPARE",
            "MEMORY SELECT INTERNAL",
            "MEMORY SELECT CARTRIDGE",
            "FUNCTION",
            "NO",
            "YES"
        ]
    );
    assert!(Button::new(42).is_none());
    assert_eq!(Button::numbered(1), Button::new(0));
    assert_eq!(Button::numbered(32), Button::new(31));
    assert!(Button::numbered(0).is_none() && Button::numbered(33).is_none());
    assert_eq!((Button::STORE.number(), Button::YES.number()), (32, 41));
    for pressed in [false, true] {
        let data = if pressed { 0x7f } else { 0 };
        for number in 0..42 {
            let change = Change::Panel(Button::new(number).expect("a button"), pressed);
            let message = syx::write_change(channel(0), change);
            assert_eq!(message, [0xf0, 0x43, 0x10, 0x08, number, data, 0xf7]);
            assert_eq!(change_in(&message).change, change);
        }
        for number in 0..46 {
            let change = Change::Remote(RemoteSwitch::new(number).expect("a switch"), pressed);
            let message = syx::write_change(channel(3), change);
            assert_eq!(message, [0xf0, 0x43, 0x13, 0x1b, number, data, 0xf7]);
            assert_eq!(change_in(&message).change, change);
        }
    }
    assert!(RemoteSwitch::new(46).is_none());
}

#[test]
fn requests() {
    // The DX7II's performance edit buffer \[DX7II-MANUAL\] p. Add-4.
    let universal = Requested::Universal {
        classification: UniversalName::new(*b"LM  ").expect("a name"),
        format: UniversalName::new(*b"8973PE").expect("a name"),
    };
    let expected: [(Requested, &[u8]); 8] = [
        (Requested::Voice, &[0x00]),
        (Requested::Performance, &[0x01]),
        (Requested::Performances, &[0x02]),
        (Requested::Supplement, &[0x05]),
        (Requested::Supplements, &[0x06]),
        (Requested::Bank, &[0x09]),
        (Requested::Condition, &[0x7d]),
        (universal, b"\x7eLM  8973PE"),
    ];
    for number in 0..16 {
        for (requested, body) in expected {
            let message = syx::write_request(channel(number), requested);
            assert_eq!(message, [&[0xf0, 0x43, 0x20 | number][..], body, &[0xf7]].concat());
            let request = Request {
                channel: channel(number),
                requested,
            };
            assert_eq!(syx::decode(&message, STRICT), Ok(Sysex::Request(request)));
        }
    }
    assert!(UniversalName::new([0x41, 0x80, 0x41, 0x41]).is_none());
}

#[test]
fn steps_and_data_out_of_range() {
    let source_select = DxPerformanceParameter::new(1).expect("a parameter");
    let receive = TxFunctionParameter::new(0).expect("a parameter");
    let protect = TxFunctionParameter::new(7).expect("a parameter");
    // OP6's R1 at 100; the pitch bend's range at 13; a button's data 0x40; the
    // source select at 0 and at 17; the data entry receive switch at 2; the
    // memory protect's data 0x40; the operator select at 6.
    let cases: [(&[u8], Change, Finding); 8] = [
        (
            &[0xf0, 0x43, 0x10, 0x00, 0x00, 100, 0xf7],
            Change::Voice(VoiceStep::new(Parameter::new(0).expect("a parameter"), 99).expect("a step")),
            Finding::OutOfRange {
                voice: 0,
                parameter: Parameter::new(0).expect("a parameter"),
                step: 100,
            },
        ),
        (
            &[0xf0, 0x43, 0x10, 0x08, 65, 13, 0xf7],
            Change::Function(FunctionStep::new(FunctionParameter::new(65).expect("a parameter"), 12).expect("a step")),
            Finding::FunctionOutOfRange {
                parameter: FunctionParameter::new(65).expect("a parameter"),
                step: 13,
            },
        ),
        (
            &[0xf0, 0x43, 0x10, 0x08, 0x20, 0x40, 0xf7],
            Change::Panel(Button::STORE, true),
            Finding::SwitchData { data: 0x40 },
        ),
        (
            &[0xf0, 0x43, 0x10, 0x04, 0x01, 0x00, 0xf7],
            Change::Performance(PerformanceStep::new(source_select, 1).expect("a step")),
            Finding::DxPerformanceOutOfRange {
                parameter: source_select,
                step: 0,
            },
        ),
        (
            &[0xf0, 0x43, 0x10, 0x04, 0x01, 17, 0xf7],
            Change::Performance(PerformanceStep::new(source_select, 16).expect("a step")),
            Finding::DxPerformanceOutOfRange {
                parameter: source_select,
                step: 17,
            },
        ),
        (
            &[0xf0, 0x43, 0x10, 0x11, 0x00, 0x02, 0xf7],
            Change::Tx(TxStep::new(receive, 1).expect("a step")),
            Finding::TxFunctionOutOfRange {
                parameter: receive,
                step: 2,
            },
        ),
        (
            &[0xf0, 0x43, 0x10, 0x11, 0x07, 0x40, 0xf7],
            Change::Tx(TxStep::new(protect, 0x7f).expect("a step")),
            Finding::SwitchData { data: 0x40 },
        ),
        (
            &[0xf0, 0x43, 0x10, 0x01, 0x1c, 0x06, 0xf7],
            Change::OperatorSelect(OperatorSelect::new(5).expect("a selection")),
            Finding::OperatorSelectOutOfRange { step: 6 },
        ),
    ];
    for (message, change, finding) in cases {
        let decoded = syx::decode(message, Options::default());
        let expected = ParameterChange {
            channel: Channel::default(),
            change,
            finding: Some(finding),
        };
        assert_eq!(decoded, Ok(Sysex::Change(expected)));
        assert_eq!(syx::decode(message, STRICT), Err(Error::Refused(finding)));
    }
}

#[test]
fn what_is_of_no_kind_read_is_refused() {
    let unsupported: [&[u8]; 23] = [
        // Voice parameter 157, and the voice group's numbers from 256 on.
        &[0xf0, 0x43, 0x10, 0x01, 0x1d, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x02, 0x00, 0x00, 0xf7],
        // The DX performance parameter change's numbers that name nothing, 0,
        // 8, 17, 27, 63 and 65, and with the group byte's low bits set.
        &[0xf0, 0x43, 0x10, 0x04, 0, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x04, 8, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x04, 17, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x04, 27, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x04, 63, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x04, 65, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x05, 2, 0x00, 0xf7],
        // The function group's numbers 42 to 63 and from 78, and with its low
        // bits set.
        &[0xf0, 0x43, 0x10, 0x08, 42, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x08, 63, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x08, 78, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x09, 64, 0x00, 0xf7],
        // The TX function parameter change's numbers 8 and 12, and the group
        // byte's low bits 0.
        &[0xf0, 0x43, 0x10, 0x11, 8, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x11, 12, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x10, 0, 0x00, 0xf7],
        // Group 3, the DX7II's supplement (group 6), switch 46.
        &[0xf0, 0x43, 0x10, 0x0c, 0x00, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x18, 0x00, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x1b, 46, 0x7f, 0xf7],
        // Requests of formats 3, 0x7C and 0x7F, and a sub-status of no kind.
        &[0xf0, 0x43, 0x20, 0x03, 0xf7],
        &[0xf0, 0x43, 0x20, 0x7c, 0xf7],
        &[0xf0, 0x43, 0x20, 0x7f, 0xf7],
        &[0xf0, 0x43, 0x30, 0x00, 0xf7],
    ];
    for message in unsupported {
        assert_eq!(
            syx::decode(message, Options::default()),
            Err(Error::Unsupported),
            "{message:02x?}"
        );
    }
    let lengths: [&[u8]; 9] = [
        // A change without its data, with two bytes of it, and with no number;
        // a request without its format, with a byte too many, of format 1 and
        // 125 among them, and a universal one with nine letters and with
        // eleven.
        &[0xf0, 0x43, 0x10, 0x00, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x00, 0x00, 0x00, 0x00, 0xf7],
        &[0xf0, 0x43, 0x10, 0x00, 0xf7],
        &[0xf0, 0x43, 0x20, 0xf7],
        &[0xf0, 0x43, 0x20, 0x09, 0x00, 0xf7],
        &[0xf0, 0x43, 0x20, 0x01, 0x00, 0xf7],
        &[0xf0, 0x43, 0x20, 0x7d, 0x00, 0xf7],
        b"\xf0\x43\x20\x7eLM  8973P\xf7",
        b"\xf0\x43\x20\x7eLM  8973PE!\xf7",
    ];
    for message in lengths {
        assert_eq!(
            syx::decode(message, Options::default()),
            Err(Error::Length),
            "{message:02x?}"
        );
    }
}

#[test]
fn performance_changes_at_every_step() {
    // The parameters of the DX performance parameter change and their ranges,
    // \[TX7-MANUAL\] p. 40: the source select 1–16; mono/poly 0–1; the pitch
    // bend's range and step 0–12; the portamento's time 0–99, glissando and
    // mode 0–1; the sensitivity 0–15 and the assignment 0–7 of the modulation
    // wheel, the foot controller, the aftertouch and the breath controller;
    // the attenuator 0–7; the master tuning 0–127.
    let ranges: [(u8, u8, u8); 17] = [
        (1, 1, 16),
        (2, 0, 1),
        (3, 0, 12),
        (4, 0, 12),
        (5, 0, 99),
        (6, 0, 1),
        (7, 0, 1),
        (9, 0, 15),
        (10, 0, 7),
        (11, 0, 15),
        (12, 0, 7),
        (13, 0, 15),
        (14, 0, 7),
        (15, 0, 15),
        (16, 0, 7),
        (26, 0, 7),
        (64, 0, 127),
    ];
    let found: Vec<(u8, u8, u8)> = DxPerformanceParameter::all()
        .map(|parameter| (parameter.number(), parameter.smallest(), parameter.largest()))
        .collect();
    assert_eq!(found, ranges);
    for (number, smallest, largest) in ranges {
        let parameter = DxPerformanceParameter::new(number).expect("a parameter");
        for step in smallest..=largest {
            let change = Change::Performance(PerformanceStep::new(parameter, step).expect("a step within its range"));
            let message = syx::write_change(channel(9), change);
            assert_eq!(message, [0xf0, 0x43, 0x19, 0x04, number, step, 0xf7]);
            assert_eq!(change_in(&message).change, change);
        }
        assert!(PerformanceStep::new(parameter, largest + 1).is_none());
        assert!(smallest == 0 || PerformanceStep::new(parameter, smallest - 1).is_none());
    }
}

#[test]
fn tx_function_changes() {
    // The TX7's own settings, \[TX7-MANUAL\] p. 41: switches of 0 and 1 at 0
    // to 4, notes 0–127 at 5 and 6, and switches of 0 and 127 at 7 and 11.
    let numbers: Vec<u8> = TxFunctionParameter::all().map(TxFunctionParameter::number).collect();
    assert_eq!(numbers, [0, 1, 2, 3, 4, 5, 6, 7, 11]);
    for number in numbers {
        let parameter = TxFunctionParameter::new(number).expect("a parameter");
        let steps: Vec<u8> = match number {
            0..=4 => vec![0, 1],
            5 | 6 => (0..=127).collect(),
            _ => vec![0, 127],
        };
        for step in steps {
            let change = Change::Tx(TxStep::new(parameter, step).expect("a step within its range"));
            let message = syx::write_change(channel(0), change);
            assert_eq!(message, [0xf0, 0x43, 0x10, 0x11, number, step, 0xf7]);
            assert_eq!(change_in(&message).change, change);
        }
    }
    let protect = TxFunctionParameter::new(7).expect("a parameter");
    assert!(protect.is_switch() && TxStep::new(protect, 1).is_none() && TxStep::new(protect, 126).is_none());
    let names: Vec<String> = TxFunctionParameter::all()
        .map(|parameter| parameter.to_string())
        .collect();
    assert_eq!(names[2], "data entry volume switch");
    assert_eq!(names[8], "load function select");
}

#[test]
fn operator_select() {
    // Voice parameter 156, 0 to 5, whose operators the TX7 numbers from OP6
    // \[TX7-MANUAL\] p. 39 and the DX7II from OP1 \[DX7II-MANUAL\] p. Add-8:
    // kept as its step.
    for step in 0..6 {
        let select = OperatorSelect::new(step).expect("a selection");
        assert_eq!(select.step(), step);
        let message = syx::write_change(channel(2), Change::OperatorSelect(select));
        assert_eq!(message, [0xf0, 0x43, 0x12, 0x01, 0x1c, step, 0xf7]);
        assert_eq!(change_in(&message).change, Change::OperatorSelect(select));
    }
    assert!(OperatorSelect::new(6).is_none());
}

#[test]
fn the_step_of_a_voice_parameter() {
    let mut random = common::SplitMix64(21);
    let Ok(Sysex::Dump(dump)) = syx::decode(&common::random_voice(&mut random, 0), STRICT) else {
        panic!("a dump");
    };
    let syx::Content::Voice(mut voice) = dump.content else {
        panic!("a voice");
    };
    // OP1's output level, 105 + 16, between two steps, and OP3 switched off.
    voice.operators[0].output_level = Value::new(41.5).expect("a level");
    voice.operators[2].on = false;
    let level = Parameter::new(121).expect("a parameter");
    assert_eq!(VoiceStep::of(&voice, level, Rounding::Nearest).step(), 42);
    assert_eq!(VoiceStep::of(&voice, level, Rounding::Floor).step(), 41);
    assert_eq!(VoiceStep::of(&voice, level, Rounding::Floor).parameter(), level);
    // The switches, D5 for OP1 to D0 for OP6 \[DX7-MANUAL\] p. 31.
    let switches = Parameter::new(155).expect("a parameter");
    assert_eq!(VoiceStep::of(&voice, switches, Rounding::Nearest).step(), 0b11_0111);
}
