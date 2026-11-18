// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The DX7II's supplements of voices against \[DX7II-MANUAL\], whose layouts
//! `common` states again by itself: the dump of one supplement, the ACED
//! format, 49 bytes, a parameter each, under format number 5; and the dump of
//! 32, the AMEM format, packed into 35 bytes each, under 6. Each byte lands in
//! its field, random valid dumps come back byte for byte, a supplement is the
//! same in both dumps, what is out of range or unused is reported and taken as
//! `syx.out_of_range` says, and values between steps and beyond their scales
//! are rounded as `syx.export_rounding` says, the pitch envelope's range among
//! them, held in octaves.

mod common;

use common::{SplitMix64, dump, random_supplement_steps, random_supplements};
use keptone_model::function::{Polyphony, PortamentoMode};
use keptone_model::id::{Area, Kind, PartParameter};
use keptone_model::part::Part;
use keptone_model::supplement::{
    BendMode, LfoTrigger, Modulation, OperatorSupplement, PitchEgRange, ScalingMode, Supplement, SupplementParameter,
};
use keptone_model::syx::{self, Channel, Content, Error, Finding, Options, OutOfRange, Sysex};
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
fn parameter(number: u8) -> SupplementParameter {
    SupplementParameter::new(number).expect("a parameter")
}

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

/// The supplement of the dump of one.
fn supplement_of(message: &[u8], options: Options) -> (Supplement, Vec<Finding>) {
    match content_of(message, options) {
        (Content::Supplement(supplement), findings) => (*supplement, findings),
        other => panic!("not one supplement: {other:?}"),
    }
}

/// The supplements of the dump of 32.
fn supplements_of(message: &[u8], options: Options) -> (Box<[Supplement; 32]>, Vec<Finding>) {
    match content_of(message, options) {
        (Content::Supplements(supplements), findings) => (supplements, findings),
        other => panic!("not 32 supplements: {other:?}"),
    }
}

/// The ranges of a controller.
fn modulation(pitch: u8, amplitude: u8, eg_bias: u8) -> Modulation {
    Modulation {
        pitch: value(pitch),
        amplitude: value(amplitude),
        eg_bias: value(eg_bias),
    }
}

#[test]
fn parameters_and_their_names() {
    let numbers: Vec<u8> = SupplementParameter::all().map(SupplementParameter::number).collect();
    assert_eq!(numbers, (0..=38).chain(64..=73).collect::<Vec<u8>>());
    assert_eq!(numbers.len(), usize::from(SupplementParameter::COUNT));
    for (index, parameter) in SupplementParameter::all().enumerate() {
        assert_eq!(parameter.largest(), common::supplement_largest(index), "{parameter}");
    }
    assert!(
        [39, 63, 74, 126]
            .into_iter()
            .all(|number| SupplementParameter::new(number).is_none())
    );
    let names = [
        (0, "OP6 scaling mode"),
        (5, "OP1 scaling mode"),
        (6, "OP6 amplitude modulation sensitivity"),
        (15, "poly/mono and unison"),
        (23, "modulation wheel pitch modulation range"),
        (26, "foot controller 1 pitch modulation range"),
        (29, "foot controller 1 volume range"),
        (33, "breath controller pitch bias"),
        (36, "aftertouch EG bias range"),
        (37, "aftertouch pitch bias"),
        (64, "foot controller 2 pitch modulation range"),
        (71, "MIDI control volume range"),
        (73, "foot controller 1 as continuous slider 1"),
    ];
    for (number, name) in names {
        assert_eq!(parameter(number).to_string(), name);
    }
}

#[test]
fn each_step_lands_in_its_field() {
    // Steps unlike each other where the ranges allow, in the order of the ACED
    // format, so that a byte that set another's field would show.
    let steps = [
        1, 0, 1, 0, 0, 1, 7, 6, 5, 4, 3, 2, 2, 1, 1, 2, 11, 10, 1, 6, 1, 9, 98, 97, 96, 95, 94, 93, 92, 91, 90, 89, 88,
        100, 87, 86, 85, 51, 5, 84, 83, 82, 81, 80, 79, 78, 77, 4, 1,
    ];
    let (supplement, findings) = supplement_of(&dump(7, 5, &steps), STRICT);
    assert_eq!(findings, []);
    let operator = |fractional: bool, sensitivity: u8| OperatorSupplement {
        scaling_mode: if fractional {
            ScalingMode::Fractional
        } else {
            ScalingMode::Normal
        },
        amplitude_sensitivity: value(sensitivity),
    };
    let expected = Supplement {
        operators: [
            operator(true, 2),
            operator(false, 3),
            operator(false, 4),
            operator(true, 5),
            operator(false, 6),
            operator(true, 7),
        ],
        pitch_eg_range: PitchEgRange::new(1.0).expect("one octave"),
        lfo_trigger: LfoTrigger::Multi,
        pitch_eg_velocity: true,
        polyphony: Polyphony::Poly,
        unison: true,
        pitch_bend_range: value(11),
        pitch_bend_step: value(10),
        pitch_bend_mode: BendMode::Lowest,
        random_pitch: value(6),
        portamento_mode: PortamentoMode::Follow,
        portamento_step: value(9),
        portamento_time: value(98),
        modulation_wheel: modulation(97, 96, 95),
        foot_controller_1: modulation(94, 93, 92),
        foot_controller_1_volume: value(91),
        breath_controller: modulation(90, 89, 88),
        breath_controller_pitch_bias: value(100),
        aftertouch: modulation(87, 86, 85),
        aftertouch_pitch_bias: value(51),
        pitch_eg_rate_scaling: value(5),
        foot_controller_2: modulation(84, 83, 82),
        foot_controller_2_volume: value(81),
        midi_control: modulation(80, 79, 78),
        midi_control_volume: value(77),
        unison_detune: value(4),
        foot_controller_1_as_slider: true,
    };
    assert_eq!(supplement, expected);
    for (parameter, &step) in SupplementParameter::all().zip(&steps) {
        assert_eq!(supplement.step(parameter, Rounding::Nearest), step, "{parameter}");
    }
}

#[test]
fn supplements_come_back_byte_for_byte() {
    let mut random = SplitMix64(41);
    for round in 0..200_u8 {
        let on = round % 16;
        let message = dump(on, 5, &random_supplement_steps(&mut random));
        let (supplement, findings) = supplement_of(&message, STRICT);
        assert_eq!(findings, []);
        let written = syx::write_supplement(&supplement, channel(on), Rounding::Nearest);
        assert_eq!(written, (message, Vec::new()));
    }
    for round in 0..20_u8 {
        let on = round % 16;
        let (_, message) = random_supplements(&mut random, on);
        let (supplements, findings) = supplements_of(&message, STRICT);
        assert_eq!(findings, []);
        let written = syx::write_supplements(&supplements, channel(on), Rounding::Nearest);
        assert_eq!(written, (message, Vec::new()));
    }
}

#[test]
fn a_supplement_is_the_same_in_both_dumps() {
    let mut random = SplitMix64(42);
    let (steps, message) = random_supplements(&mut random, 2);
    let (read, _) = supplements_of(&message, STRICT);
    for (steps, supplement) in steps.iter().zip(read.iter()) {
        let (single, findings) = supplement_of(&dump(2, 5, steps), STRICT);
        assert_eq!(findings, []);
        assert_eq!(single, *supplement);
    }
}

#[test]
fn unused_bits_of_a_packed_supplement() {
    // Each bit of the first supplement set alone: a finding where the layout
    // leaves it unused, and none where a field takes it, every field's single
    // bit being within its range.
    let used = common::supplement_used();
    for (byte, &used) in (0..).zip(&used) {
        for bit in 0..7 {
            let mut data = vec![0; 1120];
            data[usize::from(byte)] = 1 << bit;
            let (_, findings) = supplements_of(&dump(0, 6, &data), Options::default());
            let expected = if used & 1 << bit == 0 {
                vec![Finding::SupplementUnusedBits {
                    supplement: 0,
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
fn steps_out_of_range() {
    let beyond = |supplement, number, step| Finding::SupplementOutOfRange {
        supplement,
        parameter: parameter(number),
        step,
    };
    // In the dump of one: the pitch bend's range at 13, its mode at 4 and the
    // breath controller's pitch bias at 101.
    let mut steps = vec![0; 49];
    steps[16] = 13;
    steps[18] = 4;
    steps[33] = 101;
    let message = dump(0, 5, &steps);
    let (supplement, findings) = supplement_of(&message, Options::default());
    assert_eq!(findings, [beyond(0, 16, 13), beyond(0, 18, 4), beyond(0, 33, 101)]);
    assert_eq!(supplement.pitch_bend_range, value(12));
    assert_eq!(supplement.pitch_bend_mode, BendMode::KeyOn);
    assert_eq!(supplement.breath_controller_pitch_bias, value(100));
    assert_eq!(syx::decode(&message, STRICT), Err(Error::Refused(findings[0])));
    // In the dump of 32, the last supplement's: the pitch bend's range at 15,
    // its step at 13 and the portamento's step at 13.
    let mut data = vec![0; 1120];
    data[31 * 35 + 5] = 0x3c;
    data[31 * 35 + 6] = 0x0d;
    data[31 * 35 + 7] = 0x1a;
    let message = dump(0, 6, &data);
    let (supplements, findings) = supplements_of(&message, Options::default());
    assert_eq!(findings, [beyond(31, 16, 15), beyond(31, 17, 13), beyond(31, 21, 13)]);
    assert_eq!(supplements[31].portamento_step, value(12));
    assert_eq!(syx::decode(&message, STRICT), Err(Error::Refused(findings[0])));
}

#[test]
fn mono_and_unison() {
    // Parameter 15: bit 0 mono, bit 1 unison.
    let both = parameter(15);
    let mut random = SplitMix64(43);
    let (mut supplement, _) = supplement_of(&dump(0, 5, &random_supplement_steps(&mut random)), STRICT);
    for (step, polyphony, unison) in [
        (0, Polyphony::Poly, false),
        (1, Polyphony::Mono, false),
        (2, Polyphony::Poly, true),
        (3, Polyphony::Mono, true),
    ] {
        assert!(supplement.set_step(both, step));
        assert_eq!((supplement.polyphony, supplement.unison), (polyphony, unison));
        assert_eq!(supplement.step(both, Rounding::Nearest), step);
    }
    assert!(!supplement.set_step(both, 4));
    assert_eq!(supplement.step(both, Rounding::Nearest), 3);
}

#[test]
fn values_between_steps() {
    let mut random = SplitMix64(44);
    let (mut supplement, _) = supplement_of(&dump(0, 5, &random_supplement_steps(&mut random)), STRICT);
    supplement.operators[0].amplitude_sensitivity = Value::new(4.5).expect("a sensitivity");
    supplement.aftertouch_pitch_bias = Value::new(49.75).expect("a bias");
    // OP1's sensitivity is parameter 11, the aftertouch's pitch bias 37.
    for (rounding, sensitivity, bias) in [(Rounding::Nearest, 4, 50), (Rounding::Floor, 4, 49)] {
        let (written, rounded) = syx::write_supplement(&supplement, channel(0), rounding);
        assert_eq!(rounded, [(0, parameter(11)), (0, parameter(37))]);
        assert_eq!((written[6 + 11], written[6 + 37]), (sensitivity, bias));
    }
}

#[test]
fn byte_counts_of_no_dump() {
    // The dumps of supplements with a byte count of another kind's, or one
    // byte of data too few.
    for (format, count) in [(5, 155), (6, 4096)] {
        let data = vec![0; count];
        assert_eq!(
            syx::decode(&dump(0, format, &data), Options::default()),
            Err(Error::Length)
        );
    }
    for (format, count) in [(5, 49), (6, 1120)] {
        let mut message = dump(0, format, &vec![0; count]);
        message.remove(6);
        assert_eq!(syx::decode(&message, Options::default()), Err(Error::Length));
    }
}

/// Whether a message is a system-exclusive one: F0, data bytes below 0x80 and
/// F7.
fn is_sysex(message: &[u8]) -> bool {
    matches!(message, [0xf0, data @ .., 0xf7] if data.iter().all(|&byte| byte < 0x80))
}

#[test]
fn the_pitch_eg_range_in_octaves() {
    // The steps 0 to 3 are ranges of eight, four, one and half an octave
    // (docs/DESIGN.md §11.2), and the part's parameter is on the scale of 0
    // to 8.
    let steps: Vec<u64> = (0..4)
        .map(|step| PitchEgRange::from_step(step).expect("a step").get().to_bits())
        .collect();
    assert_eq!(steps, [8.0, 4.0, 1.0, 0.5].map(f64::to_bits));
    assert!(PitchEgRange::from_step(4).is_none());
    let kind = PartParameter::of_name("supplement.pitch_eg_range").map(PartParameter::kind);
    assert_eq!(kind, Some(Kind::Number(8.0)));
    // Any finite binary64: 0, which holds the pitch still, the numbers below
    // it, which turn the envelope upside down, and those beyond 8; −0 taken
    // as 0 (docs/MATH.md §0.5).
    for octaves in [8.0, 0.5, 2.0, 0.0, -1.0, 16.0, 1e300, -f64::MAX] {
        let bits = PitchEgRange::new(octaves).map(|range| range.get().to_bits());
        assert_eq!(bits, Some(octaves.to_bits()), "{octaves}");
    }
    assert!(PitchEgRange::new(f64::NAN).is_none() && PitchEgRange::new(f64::INFINITY).is_none());
    assert_eq!(PitchEgRange::new(-0.0).map(|range| range.get().to_bits()), Some(0));
    // The nearest step, of two as near the even one; or the widest at or
    // below, and half an octave for a range below it.
    for (octaves, nearest, floor) in [
        (8.0, 0, 0),
        (1e300, 0, 0),
        (6.5, 0, 1),
        (6.0, 0, 1),
        (5.0, 1, 1),
        (4.0, 1, 1),
        (3.0, 1, 2),
        (2.5, 2, 2),
        (2.0, 2, 2),
        (1.0, 2, 2),
        (0.75, 2, 3),
        (0.6, 3, 3),
        (0.5, 3, 3),
        (0.0, 3, 3),
        (-5.0, 3, 3),
        (-f64::MAX, 3, 3),
    ] {
        let range = PitchEgRange::new(octaves).expect("a range");
        assert_eq!(
            (range.step(Rounding::Nearest), range.step(Rounding::Floor)),
            (nearest, floor),
            "{octaves}"
        );
        let is_step = [8.0, 4.0, 1.0, 0.5]
            .iter()
            .any(|step: &f64| step.to_bits() == octaves.to_bits());
        assert_eq!(range.is_step(), is_step, "{octaves}");
    }
    // A step beyond 3 is taken as the narrowest, half an octave.
    let mut random = SplitMix64(47);
    let (mut supplement, _) = supplement_of(&dump(0, 5, &random_supplement_steps(&mut random)), STRICT);
    assert!(!supplement.set_step(parameter(12), 4));
    assert_eq!(supplement.pitch_eg_range.get().to_bits(), 0.5_f64.to_bits());
}

#[test]
fn values_beyond_their_scales_are_written_as_their_ends() {
    // Every number of a supplement far beyond its scale, above it for a
    // parameter of an even number and below it for one of an odd
    // (docs/MATH.md §0.5); the pitch envelope's range below its scale is
    // written as its narrowest step, half an octave.
    let mut random = SplitMix64(46);
    let (supplement, _) = supplement_of(&dump(0, 5, &random_supplement_steps(&mut random)), STRICT);
    let mut part = Part::initial(true);
    part.patch.supplement = Some(supplement);
    let mut beyond = Vec::new();
    for parameter in PartParameter::all().filter(|parameter| parameter.area() == Area::Supplement) {
        let above = parameter.number().is_multiple_of(2);
        if let Kind::Number(most) = parameter.kind() {
            assert!(part.set_value(parameter, if above { 1e300 } else { -f64::MAX }));
            let end = match (above, parameter.name().as_str()) {
                (true, _) => most,
                (false, "supplement.pitch_eg_range") => 0.5,
                (false, _) => 0.0,
            };
            beyond.push((parameter, end));
        }
    }
    let supplement = part.patch.supplement.clone().expect("a supplement");
    for rounding in [Rounding::Nearest, Rounding::Floor] {
        let (message, rounded) = syx::write_supplement(&supplement, channel(0), rounding);
        assert!(is_sysex(&message));
        assert_eq!(rounded.len(), beyond.len());
        let (back, findings) = supplement_of(&message, STRICT);
        assert_eq!(findings, []);
        let mut read = Part::initial(true);
        read.patch.supplement = Some(back.clone());
        for &(parameter, end) in &beyond {
            assert_eq!(
                read.value(parameter).map(f64::to_bits),
                Some(end.to_bits()),
                "{parameter}"
            );
        }
        let supplements: [Supplement; 32] = std::array::from_fn(|_| supplement.clone());
        let (message, rounded) = syx::write_supplements(&supplements, channel(0), rounding);
        assert!(is_sysex(&message));
        assert_eq!(rounded.len(), 32 * beyond.len());
        let (backs, findings) = supplements_of(&message, STRICT);
        assert_eq!(findings, []);
        assert!(backs.iter().all(|each| *each == back));
    }
}
