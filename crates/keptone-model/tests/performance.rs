// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The performances of the DX1 and the TX7, and the TX7's condition
//! acknowledge, against \[TX7-MANUAL\], whose layouts `common` states again by
//! itself: the dump of one performance, 94 bytes, a parameter each, under
//! format number 1 and under 2, which \[DX7-MANUAL\] prints; the dump of 64,
//! packed into 64 bytes each; and the condition acknowledge's 16 bytes. Each
//! byte lands in its field, random valid dumps come back byte for byte, a
//! performance is the same in both dumps, what is out of range or unused is
//! reported and taken as `syx.out_of_range` says, what a dump cannot hold is
//! reported, and a side's sensitivities become the DX7's ranges as
//! `syx.performance_sensitivity` says (docs/MATH.md §7.4).

mod common;

use common::{SplitMix64, dump, random_performance_steps, random_performances};
use keptone_model::function::{Assign, Polyphony, PortamentoMode};
use keptone_model::performance::{
    DxPerformanceParameter, Effect, KeyAssign, Performance, PerformanceParameter, SensitivityScale, Side, Unnamed,
    VoiceSelect,
};
use keptone_model::syx::{
    self, Byte, Channel, Condition, ConditionFlags, Content, Error, Finding, Options, OutOfRange, PerformanceLoss,
    Sysex, UniversalName,
};
use keptone_model::voice::{Rounding, Value};

/// The options that refuse whatever a valid dump does not hold.
const STRICT: Options = Options {
    accept_bad_checksum: false,
    out_of_range: OutOfRange::Reject,
};

/// The value of a step within its range.
fn value<const MAX: u8>(step: u8) -> Value<MAX> {
    Value::from_step(step).expect("a step within its range")
}

/// The parameter of a number.
fn parameter(number: u8) -> PerformanceParameter {
    PerformanceParameter::new(number).expect("a parameter")
}

/// A channel.
fn channel(number: u8) -> Channel {
    Channel::new(number).expect("a channel")
}

/// A controller of a side.
fn effect(sensitivity: u8, pitch: bool, amplitude: bool, eg_bias: bool) -> Effect {
    Effect {
        sensitivity: value(sensitivity),
        assign: Assign {
            pitch,
            amplitude,
            eg_bias,
        },
    }
}

/// The content of the dump a message is, and what it found.
fn content_of(message: &[u8], options: Options) -> (Content, Vec<Finding>) {
    match syx::decode(message, options) {
        Ok(Sysex::Dump(dump)) => (dump.content, dump.findings),
        other => panic!("not a dump: {other:?}"),
    }
}

/// The performance of the dump of one.
fn performance_of(message: &[u8], options: Options) -> (Performance, Vec<Finding>) {
    match content_of(message, options) {
        (Content::Performance(performance), findings) => (*performance, findings),
        other => panic!("not one performance: {other:?}"),
    }
}

/// The performances of the dump of 64.
fn performances_of(message: &[u8], options: Options) -> (Box<[Performance; 64]>, Vec<Finding>) {
    match content_of(message, options) {
        (Content::Performances(performances), findings) => (performances, findings),
        other => panic!("not 64 performances: {other:?}"),
    }
}

/// The numbers of the bytes of the dump of one performance that \[TX7-MANUAL\]
/// p. 42 leaves unnamed: of each side's thirty, 0, 1, 8, 17 to 25 and 27 to 29,
/// then 60, 62 and 63.
fn unnamed_numbers() -> Vec<usize> {
    let side = [0, 1, 8, 17, 18, 19, 20, 21, 22, 23, 24, 25, 27, 28, 29];
    let mut numbers: Vec<usize> = side.iter().chain(&side.map(|offset| offset + 30)).copied().collect();
    numbers.extend([60, 62, 63]);
    numbers
}

/// The sides of the steps of [`each_byte_lands_in_its_field`], as the
/// manual's table names their fields.
fn sides_of_the_steps() -> [Side; 2] {
    let a = Side {
        polyphony: Polyphony::Mono,
        pitch_bend_range: value(12),
        pitch_bend_step: value(7),
        portamento_time: value(99),
        glissando: true,
        portamento_mode: PortamentoMode::Retain,
        modulation_wheel: effect(15, true, false, false),
        foot_controller: effect(3, false, true, false),
        aftertouch: effect(9, false, false, true),
        breath_controller: effect(5, true, true, true),
        attenuation: value(6),
    };
    let b = Side {
        polyphony: Polyphony::Poly,
        pitch_bend_range: value(2),
        pitch_bend_step: value(12),
        portamento_time: value(50),
        glissando: false,
        portamento_mode: PortamentoMode::Follow,
        modulation_wheel: effect(1, false, true, true),
        foot_controller: effect(14, true, false, true),
        aftertouch: effect(0, true, true, false),
        breath_controller: effect(15, false, false, false),
        attenuation: value(1),
    };
    [a, b]
}

#[test]
fn each_byte_lands_in_its_field() {
    // Steps unlike each other where the ranges allow, so that a byte that set
    // another's field would show; an assignment's bits 0, 1 and 2 are the
    // pitch, the amplitude and the EG bias \[TX7-MANUAL\] p. 40.
    let mut steps = vec![0; 94];
    let a = [
        (2, 1),
        (3, 12),
        (4, 7),
        (5, 99),
        (6, 1),
        (7, 0),
        (9, 15),
        (10, 1),
        (11, 3),
        (12, 2),
        (13, 9),
        (14, 4),
        (15, 5),
        (16, 7),
        (26, 6),
    ];
    let b = [
        (2, 0),
        (3, 2),
        (4, 12),
        (5, 50),
        (6, 0),
        (7, 1),
        (9, 1),
        (10, 6),
        (11, 14),
        (12, 5),
        (13, 0),
        (14, 3),
        (15, 15),
        (16, 0),
        (26, 1),
    ];
    for (offset, step) in a {
        steps[offset] = step;
    }
    for (offset, step) in b {
        steps[30 + offset] = step;
    }
    steps[61] = 1;
    let name = *b"THIRTY LETTERS OF A NAME, ~{|}";
    steps[64..].copy_from_slice(&name);
    let unnamed = unnamed_numbers();
    assert_eq!(unnamed.len(), 33);
    for (byte, &number) in (80..).zip(&unnamed) {
        steps[number] = byte;
    }
    let (performance, findings) = performance_of(&dump(3, 1, &steps), STRICT);
    assert_eq!(findings, []);
    assert_eq!(performance.sides, sides_of_the_steps());
    assert_eq!(performance.voice_select, VoiceSelect::B);
    assert_eq!(performance.key_assign, KeyAssign::Single);
    assert_eq!(performance.name.bytes(), name);
    let expected: Vec<u8> = (80..113).collect();
    assert_eq!(performance.unnamed.bytes().as_slice(), expected);
    for (parameter, &step) in PerformanceParameter::all().zip(&steps) {
        assert_eq!(performance.step(parameter, Rounding::Nearest), step, "{parameter}");
    }
}

#[test]
fn performances_come_back_byte_for_byte() {
    let mut random = SplitMix64(31);
    for round in 0..200_u8 {
        let on = round % 16;
        let message = dump(on, 1, &random_performance_steps(&mut random));
        let (performance, findings) = performance_of(&message, STRICT);
        assert_eq!(findings, []);
        let written = syx::write_performance(&performance, channel(on), Rounding::Nearest);
        assert_eq!(written, (message, PerformanceLoss::default()));
    }
    for round in 0..20_u8 {
        let on = round % 16;
        let (_, message) = random_performances(&mut random, on);
        let (performances, findings) = performances_of(&message, STRICT);
        assert_eq!(findings, []);
        let written = syx::write_performances(&performances, channel(on), Rounding::Nearest);
        assert_eq!(written, (message, PerformanceLoss::default()));
    }
}

#[test]
fn a_performance_is_the_same_in_both_dumps() {
    let mut random = SplitMix64(32);
    let (performances, message) = random_performances(&mut random, 4);
    let (read, _) = performances_of(&message, STRICT);
    for ((steps, key_assign), performance) in performances.iter().zip(read.iter()) {
        let (single, findings) = performance_of(&dump(4, 1, steps), STRICT);
        assert_eq!(findings, []);
        let key_assign = KeyAssign::from_step(*key_assign).expect("a key assign mode");
        assert_eq!(Performance { key_assign, ..single }, *performance);
    }
}

#[test]
fn one_performance_under_format_two() {
    let mut random = SplitMix64(33);
    let steps = random_performance_steps(&mut random);
    let (under_one, _) = performance_of(&dump(5, 1, &steps), STRICT);
    // Read under every option, and reported; written under 1.
    for options in [Options::default(), STRICT] {
        let (under_two, findings) = performance_of(&dump(5, 2, &steps), options);
        assert_eq!(under_two, under_one);
        assert_eq!(findings, [Finding::PerformanceFormat]);
    }
    let (written, _) = syx::write_performance(&under_one, channel(5), Rounding::Nearest);
    assert_eq!(written, dump(5, 1, &steps));
}

#[test]
fn steps_and_bits_out_of_range() {
    // In the dump of one: side A's pitch bend range at 13, side B's modulation
    // wheel's sensitivity at 16, and the voice memory select at 2.
    let mut steps = vec![0; 94];
    steps[3] = 13;
    steps[39] = 16;
    steps[61] = 2;
    let message = dump(0, 1, &steps);
    let (performance, findings) = performance_of(&message, Options::default());
    let beyond = |performance, number, step| Finding::PerformanceOutOfRange {
        performance,
        parameter: parameter(number),
        step,
    };
    assert_eq!(findings, [beyond(0, 3, 13), beyond(0, 39, 16), beyond(0, 61, 2)]);
    assert_eq!(performance.sides[0].pitch_bend_range, value(12));
    assert_eq!(performance.sides[1].modulation_wheel.sensitivity, value(15));
    assert_eq!(performance.voice_select, VoiceSelect::B);
    assert_eq!(syx::decode(&message, STRICT), Err(Error::Refused(findings[0])));
    // In the dump of 64: the first performance's pitch bend range at 13 and its
    // step at 15, its portamento time at 100 and its key assign mode at 3; and
    // unused bits in bytes 0, 8 and 33 of the last.
    let mut data = vec![0; 4096];
    data[1] = 0x7d;
    data[15] = 0x40;
    data[2] = 100;
    data[32] = 0x03;
    data[63 * 64] = 0x01;
    data[63 * 64 + 8] = 0x7f;
    data[63 * 64 + 33] = 0x10;
    let message = dump(0, 2, &data);
    let (performances, findings) = performances_of(&message, Options::default());
    let unused = |byte, bits| Finding::PerformanceUnusedBits {
        performance: 63,
        byte,
        bits,
    };
    assert_eq!(
        findings,
        [
            beyond(0, 3, 13),
            beyond(0, 4, 15),
            beyond(0, 5, 100),
            Finding::KeyAssignOutOfRange {
                performance: 0,
                step: 3
            },
            unused(0, 0x01),
            unused(8, 0x7f),
            unused(33, 0x10),
        ]
    );
    let side = performances[0].sides[0];
    assert_eq!(
        (side.pitch_bend_range, side.pitch_bend_step, side.portamento_time),
        (value(12), value(12), value(99))
    );
    assert_eq!(performances[0].key_assign, KeyAssign::Split);
    assert_eq!(performances[63], performances[1]);
    assert_eq!(syx::decode(&message, STRICT), Err(Error::Refused(findings[0])));
}

#[test]
fn unused_bits_of_a_packed_performance() {
    // Each bit of the first performance set alone: a finding where the layout
    // leaves it unused, and none where a field takes it, every field's single
    // bit being within its range.
    let used = common::performance_used();
    for (byte, &used) in (0..).zip(&used) {
        for bit in 0..7 {
            let mut data = vec![0; 4096];
            data[usize::from(byte)] = 1 << bit;
            let (_, findings) = performances_of(&dump(0, 2, &data), Options::default());
            let expected = if used & 1 << bit == 0 {
                vec![Finding::PerformanceUnusedBits {
                    performance: 0,
                    byte,
                    bits: 1 << bit,
                }]
            } else {
                Vec::new()
            };
            assert_eq!(findings, expected, "byte {byte}, bit {bit}");
        }
    }
}

#[test]
fn what_a_dump_cannot_hold() {
    let mut random = SplitMix64(34);
    let (performance, _) = performance_of(&dump(0, 1, &random_performance_steps(&mut random)), STRICT);
    // The dump of one holds no key assign mode, and the dump of 64 no unnamed
    // byte.
    let dual = Performance {
        key_assign: KeyAssign::Dual,
        ..performance.clone()
    };
    let (_, loss) = syx::write_performance(&dual, channel(0), Rounding::Nearest);
    let key_assign = PerformanceLoss {
        key_assign: vec![0],
        ..PerformanceLoss::default()
    };
    assert_eq!(loss, key_assign);
    let mut performances = vec![
        Performance {
            unnamed: Unnamed::ZERO,
            ..performance.clone()
        };
        64
    ];
    performances[7] = performance.clone();
    let performances = <Box<[Performance; 64]>>::try_from(performances.into_boxed_slice()).expect("64 performances");
    let (_, loss) = syx::write_performances(&performances, channel(0), Rounding::Nearest);
    let unnamed = PerformanceLoss {
        unnamed: vec![7],
        ..PerformanceLoss::default()
    };
    assert_eq!(loss, unnamed);
    // Values between steps are rounded as `syx.export_rounding` says, and
    // reported.
    let mut between = performance;
    between.sides[0].modulation_wheel.sensitivity = Value::new(7.25).expect("a sensitivity");
    between.sides[1].portamento_time = Value::new(41.5).expect("a time");
    for (rounding, sensitivity, time) in [(Rounding::Nearest, 7, 42), (Rounding::Floor, 7, 41)] {
        let (written, loss) = syx::write_performance(&between, channel(0), rounding);
        assert_eq!(loss.rounded, [(0, parameter(9)), (0, parameter(35))]);
        assert_eq!((written[6 + 9], written[6 + 35]), (sensitivity, time));
    }
}

#[test]
fn sensitivities_become_ranges() {
    for s in 0..=15 {
        let linear = SensitivityScale::Linear.range(value(s)).get();
        assert_eq!(linear, 99.0 * f64::from(s) / 15.0);
    }
    let exact: Vec<f64> = [0, 5, 10, 15]
        .map(|s| SensitivityScale::Linear.range(value(s)).get())
        .to_vec();
    assert_eq!(exact, [0.0, 33.0, 66.0, 99.0]);
    // ⌊53s/8⌋, which the DX7's firmware computes as the high byte of 212s
    // shifted left by 3 \[AJXS-ROM\].
    let firmware: Vec<f64> = (0..=15)
        .map(|s| SensitivityScale::Firmware.range(value(s)).get())
        .collect();
    assert_eq!(
        firmware,
        [
            0.0, 6.0, 13.0, 19.0, 26.0, 33.0, 39.0, 46.0, 53.0, 59.0, 66.0, 72.0, 79.0, 86.0, 92.0, 99.0
        ]
    );
    assert_eq!(SensitivityScale::default(), SensitivityScale::Linear);
    // A range of either scale, taken back as 15r/99 and rounded to the nearest,
    // is its sensitivity again.
    for scale in [SensitivityScale::Linear, SensitivityScale::Firmware] {
        for s in 0..=15 {
            let side = Side {
                aftertouch: effect(s, false, false, false),
                ..sides_of_the_steps()[0]
            };
            let again = Side::of_function(&side.function(scale), side.attenuation);
            assert_eq!(again.aftertouch.sensitivity.step(Rounding::Nearest), s, "{scale:?}");
        }
    }
}

#[test]
fn a_side_gives_the_dx7_its_function_data() {
    let [a, _] = sides_of_the_steps();
    let function = a.function(SensitivityScale::Linear);
    assert_eq!(function.polyphony, Polyphony::Mono);
    assert_eq!(
        (function.pitch_bend_range, function.pitch_bend_step),
        (value(12), value(7))
    );
    assert_eq!(
        (function.portamento_mode, function.glissando, function.portamento_time),
        (PortamentoMode::Retain, true, value(99))
    );
    // Each controller by its name, whatever its place in either message.
    let ranges = [
        function.modulation_wheel.range.get(),
        function.foot_controller.range.get(),
        function.breath_controller.range.get(),
        function.aftertouch.range.get(),
    ];
    assert_eq!(ranges, [99.0, 19.8, 33.0, 9.0 * 99.0 / 15.0]);
    assert_eq!(function.modulation_wheel.assign, a.modulation_wheel.assign);
    assert_eq!(function.breath_controller.assign, a.breath_controller.assign);
    assert_eq!(function.aftertouch.assign, a.aftertouch.assign);
    let again = Side::of_function(&function, a.attenuation);
    for (back, sensitivity) in [
        (again.modulation_wheel, 15),
        (again.foot_controller, 3),
        (again.aftertouch, 9),
        (again.breath_controller, 5),
    ] {
        assert_eq!(back.sensitivity.step(Rounding::Nearest), sensitivity);
    }
}

#[test]
fn parameters_and_their_names() {
    assert_eq!(PerformanceParameter::all().count(), 94);
    assert!(PerformanceParameter::new(94).is_none());
    for parameter in PerformanceParameter::all() {
        let number = usize::from(parameter.number());
        assert_eq!(parameter.largest(), common::performance_largest(number), "{parameter}");
    }
    let names = [
        (2, "voice A poly/mono"),
        (6, "voice A portamento/glissando"),
        (33, "voice B pitch bend range"),
        (39, "voice B modulation wheel sensitivity"),
        (44, "voice B aftertouch assign"),
        (56, "voice B attenuation"),
        (61, "voice memory select"),
        (64, "name byte 1"),
        (93, "name byte 30"),
        (0, "byte 0"),
        (60, "byte 60"),
    ];
    for (number, name) in names {
        assert_eq!(parameter(number).to_string(), name);
    }
    let source_select = DxPerformanceParameter::new(1).expect("a parameter");
    let tuning = DxPerformanceParameter::new(64).expect("a parameter");
    let aftertouch = DxPerformanceParameter::new(13).expect("a parameter");
    assert_eq!(
        [source_select, tuning, aftertouch].map(|parameter| parameter.to_string()),
        ["source select", "master tuning", "aftertouch sensitivity"]
    );
    assert_eq!((source_select.side(), tuning.side()), (None, None));
    assert_eq!(aftertouch.side(), Some(parameter(13)));
    assert!(DxPerformanceParameter::new(0).is_none() && DxPerformanceParameter::new(8).is_none());
}

#[test]
fn the_condition_acknowledge() {
    // A TX7's \[TX7-MANUAL\] p. 45: `LM  `, `8950  `, version 1, revision 2,
    // the control change receive, the omni mode and the data entry receive on,
    // channel 5, a battery of 3.1 V, and 0.
    let data = *b"LM  8950  \x01\x02\x58\x05\x1f\x00";
    let message = dump(0, 0x7d, &data);
    let (Content::Condition(condition), findings) = content_of(&message, STRICT) else {
        panic!("not a condition acknowledge");
    };
    assert_eq!(findings, []);
    let expected = Condition {
        classification: UniversalName::new(*b"LM  ").expect("a name"),
        model: UniversalName::new(*b"8950  ").expect("a name"),
        version: Byte::new(1).expect("a byte"),
        revision: Byte::new(2).expect("a byte"),
        flags: ConditionFlags {
            control_change: true,
            omni: true,
            data_entry: true,
            ..ConditionFlags::default()
        },
        receive_channel: channel(5),
        battery: Byte::new(31).expect("a byte"),
        data: Byte::new(0).expect("a byte"),
    };
    assert_eq!(condition, expected);
    assert_eq!(syx::write_condition(&condition, Channel::default()), message);
    // Each flag in its bit.
    for bit in 0..7 {
        assert_eq!(ConditionFlags::of(1 << bit).byte(), 1 << bit);
    }
    assert!(Byte::new(0x80).is_none());
    // A receive channel beyond 15.
    let mut beyond = data;
    beyond[13] = 16;
    let message = dump(0, 0x7d, &beyond);
    let (Content::Condition(condition), findings) = content_of(&message, Options::default()) else {
        panic!("not a condition acknowledge");
    };
    assert_eq!(findings, [Finding::ConditionChannel { channel: 16 }]);
    assert_eq!(condition.receive_channel, channel(15));
    assert_eq!(
        syx::decode(&message, STRICT),
        Err(Error::Refused(Finding::ConditionChannel { channel: 16 }))
    );
}

#[test]
fn byte_counts_of_no_dump() {
    // The dumps of performances and the condition acknowledge with a byte
    // count of another kind's, or one byte of data too few.
    for (format, count) in [(1, 4096), (2, 155), (0x7d, 94)] {
        let data = vec![0; count];
        assert_eq!(
            syx::decode(&dump(0, format, &data), Options::default()),
            Err(Error::Length)
        );
    }
    for (format, count) in [(1, 94), (2, 4096), (0x7d, 16)] {
        let mut message = dump(0, format, &vec![0; count]);
        message.remove(6);
        assert_eq!(syx::decode(&message, Options::default()), Err(Error::Length));
    }
}
