// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The DX7's dumps against \[DEXED-SYSEX\], whose layout `common` states again
//! by itself. Each byte of a single-voice dump lands in the field of the voice
//! it names, and a voice is the same in a single-voice dump and in a bank.
//! Random valid dumps, of one voice and of 32, come back byte for byte when
//! they are read and written again (docs/MATH.md §16). Values between steps
//! are rounded as `syx.export_rounding` says, and what a dump cannot hold is
//! reported; wrong checksums and bytes out of range are reported and taken as
//! `syx.accept_bad_checksum` and `syx.out_of_range` say; and a file is scanned
//! for every message in it, whatever lies around them.

mod common;

use common::{SplitMix64, dump, pack, random_bank, random_steps, random_voice};
use keptone_model::syx::{self, Change, Channel, Content, Dump, Error, Finding, Loss, Options, OutOfRange, Sysex};
use keptone_model::voice::{Curve, Parameter, Rounding, Value, Voice, Wave};

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
fn parameter(number: u8) -> Parameter {
    Parameter::new(number).expect("a parameter")
}

/// The dump that a message decoded is.
fn dump_in(sysex: Sysex) -> Dump {
    match sysex {
        Sysex::Dump(dump) => dump,
        other => panic!("not a dump: {other:?}"),
    }
}

/// The voice of a single-voice dump.
fn voice_of(dump: Dump) -> Voice {
    match dump.content {
        Content::Voice(voice) => *voice,
        other => panic!("not a voice: {other:?}"),
    }
}

/// The voices of a 32-voice dump.
fn bank_of(dump: Dump) -> Box<[Voice; 32]> {
    match dump.content {
        Content::Bank(voices) => voices,
        other => panic!("not a bank: {other:?}"),
    }
}

/// The steps of a voice's parameters in a dump, rounded to the nearest.
fn steps_of(voice: &Voice) -> Vec<u8> {
    Parameter::all()
        .take(155)
        .map(|parameter| voice.step(parameter, Rounding::Nearest))
        .collect()
}

#[test]
fn each_byte_lands_in_its_field() {
    // Steps as unlike one another as the ranges let them be.
    let steps: Vec<u8> = (0..155)
        .map(|number| u8::try_from((7 * number + 3) % (usize::from(common::largest(number)) + 1)).expect("a step"))
        .collect();
    let voice = voice_of(dump_in(syx::decode(&dump(0, 0, &steps), STRICT).expect("a valid dump")));
    for (k, operator) in voice.operators.iter().enumerate() {
        // OP1's parameters come last, OP6's first.
        let step = |offset: usize| steps[21 * (5 - k) + offset];
        for i in 0..4 {
            assert_eq!(operator.envelope.rates[i], value(step(i)));
            assert_eq!(operator.envelope.levels[i], value(step(4 + i)));
        }
        assert_eq!(operator.break_point, value(step(8)));
        assert_eq!(operator.left_depth, value(step(9)));
        assert_eq!(operator.right_depth, value(step(10)));
        assert_eq!(operator.left_curve.step(), step(11));
        assert_eq!(operator.right_curve.step(), step(12));
        assert_eq!(operator.rate_scaling, value(step(13)));
        assert_eq!(operator.amplitude_sensitivity, value(step(14)));
        assert_eq!(operator.velocity_sensitivity, value(step(15)));
        assert_eq!(operator.output_level, value(step(16)));
        assert_eq!(operator.mode.step(), step(17));
        assert_eq!(operator.coarse, value(step(18)));
        assert_eq!(operator.fine, value(step(19)));
        assert_eq!(operator.detune, value(step(20)));
        assert!(operator.on);
    }
    for i in 0..4 {
        assert_eq!(voice.pitch_envelope.rates[i], value(steps[126 + i]));
        assert_eq!(voice.pitch_envelope.levels[i], value(steps[130 + i]));
    }
    assert_eq!(voice.algorithm.step(), steps[134]);
    assert_eq!(voice.feedback, value(steps[135]));
    assert_eq!(voice.oscillator_sync, steps[136] != 0);
    assert_eq!(voice.lfo.speed, value(steps[137]));
    assert_eq!(voice.lfo.delay, value(steps[138]));
    assert_eq!(voice.lfo.pitch_depth, value(steps[139]));
    assert_eq!(voice.lfo.amplitude_depth, value(steps[140]));
    assert_eq!(voice.lfo.sync, steps[141] != 0);
    assert_eq!(voice.lfo.wave.step(), steps[142]);
    assert_eq!(voice.pitch_sensitivity, value(steps[143]));
    assert_eq!(voice.transpose, value(steps[144]));
    assert_eq!(voice.name.bytes()[..], steps[145..]);
    assert_eq!(steps_of(&voice), steps);
}

#[test]
fn single_voice_dumps_come_back_byte_for_byte() {
    let mut random = SplitMix64(1);
    for _ in 0..2000 {
        let channel = random.up_to(15);
        let message = random_voice(&mut random, channel);
        let dump = dump_in(syx::decode(&message, STRICT).expect("a valid dump"));
        assert_eq!(dump.findings, []);
        assert_eq!(dump.channel.get(), channel);
        let voice = voice_of(dump);
        let channel = Channel::new(channel).expect("a channel");
        for rounding in [Rounding::Nearest, Rounding::Floor] {
            let (written, loss) = syx::write_voice(&voice, channel, rounding);
            assert_eq!(written, message);
            assert_eq!(loss, Loss::default());
        }
    }
}

#[test]
fn bank_dumps_come_back_byte_for_byte() {
    let mut random = SplitMix64(2);
    for _ in 0..200 {
        let channel = random.up_to(15);
        let (steps, message) = random_bank(&mut random, channel);
        assert_eq!(message.len(), 4104);
        let dump = dump_in(syx::decode(&message, STRICT).expect("a valid dump"));
        assert_eq!(dump.findings, []);
        assert_eq!(dump.channel.get(), channel);
        let channel = dump.channel;
        let voices = bank_of(dump);
        for (voice, steps) in voices.iter().zip(&steps) {
            assert_eq!(&steps_of(voice), steps);
        }
        let (written, loss) = syx::write_bank(&voices, channel, Rounding::Nearest);
        assert_eq!(written, message);
        assert_eq!(loss, Loss::default());
    }
}

#[test]
fn a_voice_is_the_same_in_both_dumps() {
    let mut random = SplitMix64(3);
    for _ in 0..20 {
        let (steps, message) = random_bank(&mut random, 0);
        let voices = bank_of(dump_in(syx::decode(&message, STRICT).expect("a valid dump")));
        for (voice, steps) in voices.iter().zip(&steps) {
            let single = voice_of(dump_in(syx::decode(&dump(0, 0, steps), STRICT).expect("a valid dump")));
            assert_eq!(&single, voice);
        }
    }
}

#[test]
fn values_between_steps_are_rounded_and_reported() {
    let mut random = SplitMix64(4);
    let mut voice = voice_of(dump_in(
        syx::decode(&random_voice(&mut random, 0), STRICT).expect("a valid dump"),
    ));
    // OP6's output level, parameter 16; OP1's R1, 105; the feedback, 135; and
    // the transposition, 144.
    voice.operators[5].output_level = Value::new(3.5).expect("a level");
    voice.operators[0].envelope.rates[0] = Value::new(2.5).expect("a rate");
    voice.feedback = Value::new(6.75).expect("a feedback");
    voice.transpose = Value::new(23.25).expect("a transposition");
    for (rounding, expected) in [(Rounding::Nearest, [4, 2, 7, 23]), (Rounding::Floor, [3, 2, 6, 23])] {
        let (message, loss) = syx::write_voice(&voice, Channel::default(), rounding);
        let data = &message[6..161];
        assert_eq!([data[16], data[105], data[135], data[144]], expected);
        let rounded: Vec<(u8, Parameter)> = [16, 105, 135, 144].map(|number| (0, parameter(number))).into();
        assert_eq!(loss.rounded, rounded);
        assert_eq!(loss.switched_off, []);
        // Read back, the voice holds the steps.
        let back = voice_of(dump_in(syx::decode(&message, STRICT).expect("a valid dump")));
        assert_eq!(back.operators[5].output_level, value(expected[0]));
        assert_eq!(back.transpose, value(expected[3]));
    }
}

#[test]
fn switches_are_reported_lost() {
    let mut random = SplitMix64(5);
    let (_, message) = random_bank(&mut random, 0);
    let mut voices = bank_of(dump_in(syx::decode(&message, STRICT).expect("a valid dump")));
    let switches = parameter(155);
    assert!(
        voices
            .iter()
            .all(|voice| voice.step(switches, Rounding::Nearest) == 0x3f)
    );
    // OP3 off in voice 4: OP1's switch is bit 5, OP3's bit 3.
    voices[4].operators[2].on = false;
    assert_eq!(voices[4].step(switches, Rounding::Nearest), 0b11_0111);
    let (written, loss) = syx::write_bank(&voices, Channel::default(), Rounding::Nearest);
    assert_eq!(loss.switched_off, [4]);
    assert_eq!(loss.rounded, []);
    assert_eq!(written, message);
    // Setting the switches by their bits: OP6 alone on, then more than six
    // bits, which turns every operator on and says so.
    let voice = &mut voices[0];
    assert!(voice.set_step(switches, 0b00_0001));
    let on: Vec<bool> = voice.operators.iter().map(|operator| operator.on).collect();
    assert_eq!(on, [false, false, false, false, false, true]);
    assert!(!voice.set_step(switches, 0x40));
    assert!(voice.operators.iter().all(|operator| operator.on));
}

#[test]
fn wrong_checksums_are_reported() {
    let mut random = SplitMix64(6);
    for format in [0, 9] {
        let mut message = if format == 0 {
            random_voice(&mut random, 3)
        } else {
            random_bank(&mut random, 3).1
        };
        let at = message.len() - 2;
        let right = message[at];
        let wrong = (right + 1) & 0x7f;
        message[at] = wrong;
        let finding = Finding::Checksum {
            expected: right,
            found: wrong,
        };
        let dump = dump_in(syx::decode(&message, Options::default()).expect("a dump"));
        assert_eq!(dump.findings, [finding]);
        let refusing = Options {
            accept_bad_checksum: false,
            ..Options::default()
        };
        assert_eq!(syx::decode(&message, refusing), Err(Error::Refused(finding)));
        // Written again, the dump has its right checksum.
        let channel = dump.channel;
        let (written, _) = match dump.content {
            Content::Voice(voice) => syx::write_voice(&voice, channel, Rounding::Nearest),
            Content::Bank(voices) => syx::write_bank(&voices, channel, Rounding::Nearest),
            other => panic!("neither a voice nor a bank: {other:?}"),
        };
        message[at] = right;
        assert_eq!(written, message);
    }
}

#[test]
fn steps_out_of_range_are_clamped_or_refused() {
    let mut random = SplitMix64(7);
    let mut steps = random_steps(&mut random);
    // OP6's R1, left curve and detune; the algorithm; the LFO's wave; the
    // transposition.
    let beyond = [(0, 100), (11, 4), (20, 15), (134, 32), (142, 6), (144, 127)];
    for (number, step) in beyond {
        steps[number] = step;
    }
    let message = dump(0, 0, &steps);
    let dump = dump_in(syx::decode(&message, Options::default()).expect("a dump"));
    let findings: Vec<Finding> = beyond
        .iter()
        .map(|&(number, step)| Finding::OutOfRange {
            voice: 0,
            parameter: parameter(u8::try_from(number).expect("a number")),
            step,
        })
        .collect();
    assert_eq!(dump.findings, findings);
    let voice = voice_of(dump);
    assert_eq!(voice.operators[5].envelope.rates[0], value(99));
    assert_eq!(voice.operators[5].left_curve, Curve::PositiveLinear);
    assert_eq!(voice.operators[5].detune, value(14));
    assert_eq!(voice.algorithm.number(), 32);
    assert_eq!(voice.lfo.wave, Wave::SampleAndHold);
    assert_eq!(voice.transpose, value(48));
    let rejecting = Options {
        out_of_range: OutOfRange::Reject,
        ..Options::default()
    };
    assert_eq!(syx::decode(&message, rejecting), Err(Error::Refused(findings[0])));
    // Written again, the dump is valid.
    let (written, _) = syx::write_voice(&voice, Channel::default(), Rounding::Nearest);
    let again = dump_in(syx::decode(&written, STRICT).expect("a valid dump"));
    assert_eq!(voice_of(again), voice);
}

#[test]
fn packed_bytes_out_of_range_are_clamped_or_refused() {
    let mut random = SplitMix64(8);
    let (_, mut message) = random_bank(&mut random, 0);
    let byte = |voice: usize, byte: usize| 6 + 128 * voice + byte;
    // Voice 0: bits 6–4 of byte 111, which holds the key sync and the
    // feedback in bits 3–0.
    message[byte(0, 111)] |= 0x70;
    // Voice 7: OP6's detune, bits 6–3 of byte 12, at 15.
    message[byte(7, 12)] |= 0x78;
    // Voice 9: the LFO's wave, bits 3–1 of byte 116, at 7.
    message[byte(9, 116)] |= 0x0e;
    // Voice 31: bit 6 of OP1's byte 15, which holds the coarse frequency and
    // the mode in bits 5–0, and OP1's output level, byte 14, at 127.
    message[byte(31, 85 + 15)] |= 0x40;
    message[byte(31, 85 + 14)] = 0x7f;
    let at = message.len() - 2;
    message[at] = common::checksum(&message[6..at]);
    let dump = dump_in(syx::decode(&message, Options::default()).expect("a dump"));
    let expected = [
        Finding::UnusedBits {
            voice: 0,
            byte: 111,
            bits: 0x70,
        },
        Finding::OutOfRange {
            voice: 7,
            parameter: parameter(20),
            step: 15,
        },
        Finding::OutOfRange {
            voice: 9,
            parameter: parameter(142),
            step: 7,
        },
        Finding::UnusedBits {
            voice: 31,
            byte: 100,
            bits: 0x40,
        },
        Finding::OutOfRange {
            voice: 31,
            parameter: parameter(105 + 16),
            step: 127,
        },
    ];
    assert_eq!(dump.findings, expected);
    let voices = bank_of(dump);
    assert_eq!(voices[7].operators[5].detune, value(14));
    assert_eq!(voices[9].lfo.wave, Wave::SampleAndHold);
    assert_eq!(voices[31].operators[0].output_level, value(99));
    let rejecting = Options {
        out_of_range: OutOfRange::Reject,
        ..Options::default()
    };
    assert_eq!(syx::decode(&message, rejecting), Err(Error::Refused(expected[0])));
    // Written again, the bank is valid, and holds the same voices.
    let (written, _) = syx::write_bank(&voices, Channel::default(), Rounding::Nearest);
    assert_eq!(
        bank_of(dump_in(syx::decode(&written, STRICT).expect("a valid dump"))),
        voices
    );
}

#[test]
fn messages_that_are_not_dumps() {
    let options = Options::default();
    let mut random = SplitMix64(9);
    let voice = random_voice(&mut random, 0);
    assert_eq!(syx::decode(&[], options), Err(Error::NotSysEx));
    assert_eq!(syx::decode(&[0xf0], options), Err(Error::NotSysEx));
    assert_eq!(syx::decode(&[0xf0, 0xf7], options), Err(Error::Unsupported));
    assert_eq!(syx::decode(&[0xf0, 0x43, 0xf7], options), Err(Error::Unsupported));
    assert_eq!(syx::decode(&voice[..162], options), Err(Error::NotSysEx));
    assert_eq!(syx::decode(&voice[1..], options), Err(Error::NotSysEx));
    let changed = |at: usize, byte: u8| {
        let mut message = voice.clone();
        message[at] = byte;
        syx::decode(&message, options)
    };
    // A byte with its high bit set in the middle; another maker's message; a
    // parameter change's sub-status, whose data is then far more than its one
    // byte; a sub-status of no kind; a format of neither dump; a byte count
    // that is not the format's.
    assert_eq!(changed(80, 0x80), Err(Error::NotSysEx));
    assert_eq!(changed(1, 0x41), Err(Error::Unsupported));
    assert_eq!(changed(2, 0x10), Err(Error::Length));
    assert_eq!(changed(2, 0x30), Err(Error::Unsupported));
    assert_eq!(changed(3, 5), Err(Error::Unsupported));
    assert_eq!(changed(5, 0x1c), Err(Error::Length));
    assert_eq!(changed(4, 0x20), Err(Error::Length));
    // A byte more, and a byte less.
    let mut longer = voice.clone();
    longer.insert(50, 0);
    assert_eq!(syx::decode(&longer, options), Err(Error::Length));
    let mut shorter = voice.clone();
    shorter.remove(50);
    assert_eq!(syx::decode(&shorter, options), Err(Error::Length));
    // A bank's format and count with a voice's data.
    let mut bank = voice.clone();
    bank[3..6].copy_from_slice(&[9, 0x20, 0x00]);
    assert_eq!(syx::decode(&bank, options), Err(Error::Length));
}

#[test]
fn files_are_scanned_for_every_message() {
    let mut random = SplitMix64(10);
    let voice = random_voice(&mut random, 1);
    let (_, bank) = random_bank(&mut random, 2);
    let mut file = b"bytes before \xf7 and a stray end".to_vec();
    let mut expected = Vec::new();
    let mut add = |file: &mut Vec<u8>, bytes: &[u8], length: usize| {
        expected.push((file.len(), length));
        file.extend_from_slice(bytes);
    };
    add(&mut file, &voice, 163);
    // A note between messages.
    file.extend_from_slice(&[0x90, 0x3c, 0x40]);
    add(&mut file, &bank, 4104);
    // A parameter change of the transposition, 144, to 5; another maker's
    // message; and a message cut short by the F0 of the next.
    add(&mut file, &[0xf0, 0x43, 0x10, 0x01, 0x10, 0x05, 0xf7], 7);
    add(&mut file, &[0xf0, 0x41, 0x10, 0x42, 0x12, 0xf7], 6);
    add(&mut file, &[0xf0, 0x43, 0x00, 0x00, 0x01], 5);
    add(&mut file, &voice, 163);
    // A dump cut short by a status byte, after which the rest of it is passed
    // over up to the next F0.
    let mut cut = voice.clone();
    cut[60] = 0x90;
    add(&mut file, &cut, 60);
    add(&mut file, &voice, 163);
    // And one cut short by the end of the file.
    add(&mut file, &[0xf0, 0x43, 0x00], 3);
    let messages: Vec<syx::Message> = syx::read(&file, Options::default()).collect();
    let found: Vec<(usize, usize)> = messages
        .iter()
        .map(|message| (message.offset, message.length))
        .collect();
    assert_eq!(found, expected);
    let results: Vec<Result<(), Error>> = messages
        .iter()
        .map(|message| message.result.as_ref().map(|_| ()).map_err(|&error| error))
        .collect();
    assert_eq!(
        results,
        [
            Ok(()),
            Ok(()),
            Ok(()),
            Err(Error::Unsupported),
            Err(Error::Unterminated),
            Ok(()),
            Err(Error::Unterminated),
            Ok(()),
            Err(Error::Unterminated),
        ]
    );
    let Ok(Sysex::Dump(dump)) = &messages[1].result else {
        panic!("a dump")
    };
    let Ok(Sysex::Change(change)) = &messages[2].result else {
        panic!("a parameter change")
    };
    let transposition = Parameter::new(144).expect("a parameter");
    assert!(matches!(change.change, Change::Voice(step) if step.parameter() == transposition && step.step() == 5));
    assert_eq!((change.channel.get(), change.finding), (0, None));
    assert_eq!(dump.channel.get(), 2);
    assert!(matches!(dump.content, Content::Bank(_)));
    // A file of nothing, and of no message.
    assert_eq!(syx::read(&[], Options::default()).count(), 0);
    assert_eq!(syx::read(b"no message here", Options::default()).count(), 0);
}

#[test]
fn channels() {
    assert_eq!(Channel::new(15).map(Channel::get), Some(15));
    assert!(Channel::new(16).is_none());
    assert_eq!(Channel::default().get(), 0);
}

#[test]
fn every_step_at_its_largest() {
    // A voice of every step its largest fills every bit a packed voice uses.
    let steps: Vec<u8> = (0..155).map(common::largest).collect();
    let voice = voice_of(dump_in(syx::decode(&dump(0, 0, &steps), STRICT).expect("a valid dump")));
    assert_eq!(steps_of(&voice), steps);
    let voices: [Voice; 32] = std::array::from_fn(|_| voice.clone());
    let (written, loss) = syx::write_bank(&voices, Channel::default(), Rounding::Nearest);
    assert_eq!(loss, Loss::default());
    assert_eq!(&written[6..134], pack(&steps).as_slice());
    assert_eq!(
        bank_of(dump_in(syx::decode(&written, STRICT).expect("a valid dump"))).as_slice(),
        voices
    );
}
