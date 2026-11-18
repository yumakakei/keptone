// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The parameters' IDs and names (docs/DESIGN.md §7.4), which never change, so
//! that this file lists every one of a part's again by itself: an ID is the
//! area in bits 31 to 28, the part in 27 to 12 and the number in 11 to 0; the
//! voice's numbers are the DX7's \[DEXED-SYSEX\] with the operators' switches
//! at 155 to 160; and each parameter is read and set as a binary64 of its
//! kind, and refuses every other value, the part left as it was.

mod common;

use common::dump;
use keptone_model::id::{Area, Kind, ParameterId, PartParameter};
use keptone_model::part::Part;
use keptone_model::syx::{self, Content, Options, Sysex};

/// The names of an operator's parameters after `voice.opN.`, in the order of
/// the DX7's numbers within its 21.
const OPERATOR: [&str; 21] = [
    "eg.rate.1",
    "eg.rate.2",
    "eg.rate.3",
    "eg.rate.4",
    "eg.level.1",
    "eg.level.2",
    "eg.level.3",
    "eg.level.4",
    "break_point",
    "left_depth",
    "right_depth",
    "left_curve",
    "right_curve",
    "rate_scaling",
    "amplitude_sensitivity",
    "velocity_sensitivity",
    "output_level",
    "mode",
    "coarse",
    "fine",
    "detune",
];

/// The names of the voice's own parameters after `voice.`, the DX7's 126 to
/// 144.
const VOICE: [&str; 19] = [
    "pitch_eg.rate.1",
    "pitch_eg.rate.2",
    "pitch_eg.rate.3",
    "pitch_eg.rate.4",
    "pitch_eg.level.1",
    "pitch_eg.level.2",
    "pitch_eg.level.3",
    "pitch_eg.level.4",
    "algorithm",
    "feedback",
    "oscillator_sync",
    "lfo.speed",
    "lfo.delay",
    "lfo.pitch_depth",
    "lfo.amplitude_depth",
    "lfo.sync",
    "lfo.wave",
    "pitch_sensitivity",
    "transpose",
];

/// The names of the supplement's parameters after its operators', after
/// `supplement.`.
const SUPPLEMENT: [&str; 38] = [
    "pitch_eg_range",
    "lfo_trigger",
    "pitch_eg_velocity",
    "polyphony",
    "unison",
    "pitch_bend_range",
    "pitch_bend_step",
    "pitch_bend_mode",
    "random_pitch",
    "portamento_mode",
    "portamento_step",
    "portamento_time",
    "modulation_wheel.pitch",
    "modulation_wheel.amplitude",
    "modulation_wheel.eg_bias",
    "foot_controller_1.pitch",
    "foot_controller_1.amplitude",
    "foot_controller_1.eg_bias",
    "foot_controller_1_volume",
    "breath_controller.pitch",
    "breath_controller.amplitude",
    "breath_controller.eg_bias",
    "breath_controller_pitch_bias",
    "aftertouch.pitch",
    "aftertouch.amplitude",
    "aftertouch.eg_bias",
    "aftertouch_pitch_bias",
    "pitch_eg_rate_scaling",
    "foot_controller_2.pitch",
    "foot_controller_2.amplitude",
    "foot_controller_2.eg_bias",
    "foot_controller_2_volume",
    "midi_control.pitch",
    "midi_control.amplitude",
    "midi_control.eg_bias",
    "midi_control_volume",
    "unison_detune",
    "foot_controller_1_as_slider",
];

/// The names of what a part has of its own, after `part.`.
const PART: [&str; 10] = [
    "on",
    "omni",
    "group",
    "channel",
    "lowest_key",
    "highest_key",
    "key_filter",
    "note_shift",
    "note_limit",
    "alternate",
];

/// Every parameter of a part, its area, its number and its name, in the order
/// of their IDs.
fn every_parameter() -> Vec<(Area, u16, String)> {
    let mut parameters = Vec::new();
    for number in 0..126_u16 {
        let name = format!("voice.op{}.{}", 6 - number / 21, OPERATOR[usize::from(number % 21)]);
        parameters.push((Area::Voice, number, name));
    }
    for (number, name) in (126..).zip(VOICE) {
        parameters.push((Area::Voice, number, format!("voice.{name}")));
    }
    for operator in 1..=6_u16 {
        parameters.push((Area::Voice, 154 + operator, format!("voice.op{operator}.on")));
    }
    let mut function = vec![
        "polyphony".to_owned(),
        "pitch_bend_range".to_owned(),
        "pitch_bend_step".to_owned(),
        "portamento_mode".to_owned(),
        "glissando".to_owned(),
        "portamento_time".to_owned(),
    ];
    for controller in ["modulation_wheel", "foot_controller", "breath_controller", "aftertouch"] {
        for field in ["range", "assign.pitch", "assign.amplitude", "assign.eg_bias"] {
            function.push(format!("{controller}.{field}"));
        }
    }
    for (number, name) in (0..).zip(function) {
        parameters.push((Area::Function, number, format!("function.{name}")));
    }
    let mut supplement = Vec::new();
    for operator in 1..=6 {
        supplement.push(format!("op{operator}.scaling_mode"));
        supplement.push(format!("op{operator}.amplitude_sensitivity"));
    }
    supplement.extend(SUPPLEMENT.map(str::to_owned));
    for (number, name) in (0..).zip(supplement) {
        parameters.push((Area::Supplement, number, format!("supplement.{name}")));
    }
    for (number, name) in (0..).zip(PART) {
        parameters.push((Area::Part, number, format!("part.{name}")));
    }
    parameters
}

#[test]
fn every_name_and_number() {
    let expected = every_parameter();
    let found: Vec<_> = PartParameter::all()
        .map(|parameter| (parameter.area(), parameter.number(), parameter.name()))
        .collect();
    assert_eq!(found, expected);
    assert_eq!(found.len(), PartParameter::COUNT);
    for (area, number, name) in expected {
        let parameter = PartParameter::new(area, number).expect("a parameter");
        assert_eq!(PartParameter::of_name(&name), Some(parameter), "{name}");
        assert_eq!(parameter.to_string(), name);
    }
    // The bytes of the name, the DX7's 155 itself and every number beyond an
    // area's last name no parameter; nor does any number of the settings'.
    for number in (145..=154).chain([161, 4095]) {
        assert_eq!(PartParameter::new(Area::Voice, number), None, "{number}");
    }
    assert_eq!(PartParameter::new(Area::Function, 22), None);
    assert_eq!(PartParameter::new(Area::Supplement, 50), None);
    assert_eq!(PartParameter::new(Area::Part, 10), None);
    assert_eq!(PartParameter::new(Area::Setting, 0), None);
    assert_eq!(PartParameter::of_name("voice.name"), None);
}

#[test]
fn the_layout_of_an_id() {
    let areas = [Area::Setting, Area::Voice, Area::Function, Area::Supplement, Area::Part];
    for (bits, area) in (0..).zip(areas) {
        assert_eq!((area.number(), Area::of_number(bits)), (bits, Some(area)));
    }
    assert_eq!(Area::of_number(5), None);
    let id = ParameterId::new(Area::Supplement, 0xbeef, 0xabc).expect("an ID");
    assert_eq!(id.bits(), 0x3bee_fabc);
    assert_eq!((id.area(), id.part(), id.number()), (Area::Supplement, 0xbeef, 0xabc));
    assert_eq!(id.to_string(), "0x3beefabc");
    assert_eq!(ParameterId::from_bits(0x3bee_fabc), Some(id));
    // A number of 2¹², a part of a setting, and the areas no one has.
    assert_eq!(ParameterId::new(Area::Voice, 0, 0x1000), None);
    assert_eq!(ParameterId::new(Area::Setting, 1, 0), None);
    for bits in [0x0000_1000, 0x5000_0000, 0xf000_0000] {
        assert_eq!(ParameterId::from_bits(bits), None, "{bits:#x}");
    }
    for parameter in PartParameter::all() {
        for part in [0, 1, 0xffff] {
            let id = parameter.id(part);
            assert_eq!(
                (id.area(), id.part(), id.number()),
                (parameter.area(), part, parameter.number())
            );
            assert_eq!(ParameterId::from_bits(id.bits()), Some(id));
        }
    }
    assert_eq!(
        PartParameter::of_name("voice.op6.eg.rate.1").map(|parameter| parameter.id(1).bits()),
        Some(0x1000_1000)
    );
}

/// A part whose patch has a supplement, every step of it 0.
fn part_with_supplement() -> Part {
    let message = dump(0, 5, &[0; 49]);
    let Ok(Sysex::Dump(dump)) = syx::decode(&message, Options::default()) else {
        panic!("a dump of a supplement");
    };
    let Content::Supplement(supplement) = dump.content else {
        panic!("a supplement");
    };
    let mut part = Part::initial(true);
    part.patch.supplement = Some(*supplement);
    part
}

/// Values a parameter of a kind takes, and values it refuses.
fn values_of(kind: Kind) -> (Vec<f64>, Vec<f64>) {
    let common = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
    let (takes, refuses) = match kind {
        // Any finite number, on the scale and beyond its ends (docs/MATH.md
        // §0.5).
        Kind::Number(most) => (
            vec![
                0.0,
                most / 2.0 + 0.25,
                most,
                -1.0,
                most + 1.0,
                -0.5,
                f64::MAX,
                -f64::MAX,
            ],
            Vec::new(),
        ),
        Kind::Signed(most) => (
            vec![-most, 0.5, most, -most - 1.0, most + 0.5, f64::MAX, -f64::MAX],
            Vec::new(),
        ),
        Kind::Integer(least, most) => (
            vec![f64::from(least), f64::from(most)],
            vec![f64::from(least) - 1.0, f64::from(most) + 1.0, f64::from(least) + 0.5],
        ),
        Kind::Choice(names) => {
            // Every choice, so that each of them is read back as it was set.
            let count = f64::from(u32::try_from(names.len()).expect("a few names"));
            let every = (0..names.len()).map(|index| f64::from(u32::try_from(index).expect("a few")));
            (every.collect(), vec![-1.0, count, 0.5])
        }
        Kind::Switch => (vec![0.0, 1.0], vec![-1.0, 2.0, 0.5]),
        Kind::Count => (
            vec![0.0, 1.0, f64::from(u32::MAX)],
            vec![-1.0, 0.5, f64::from(u32::MAX) + 1.0],
        ),
    };
    (takes, [&refuses[..], &common].concat())
}

#[test]
fn every_parameter_takes_its_values_and_no_other() {
    for parameter in PartParameter::all() {
        let (takes, refuses) = values_of(parameter.kind());
        for value in takes {
            let mut part = part_with_supplement();
            assert!(part.set_value(parameter, value), "{parameter} = {value}");
            assert_eq!(part.value(parameter), Some(value), "{parameter}");
        }
        for value in refuses {
            let mut part = part_with_supplement();
            let before = part.clone();
            assert!(!part.set_value(parameter, value), "{parameter} = {value}");
            assert_eq!(part, before, "{parameter} = {value}");
        }
        // −0 is taken as +0, so that equal values have equal bits.
        let mut part = part_with_supplement();
        if part.set_value(parameter, -0.0) {
            assert!(part.value(parameter).is_some_and(f64::is_sign_positive), "{parameter}");
        }
    }
}

#[test]
fn each_parameter_sets_its_field_alone() {
    // Every parameter set to the value furthest from the start that it takes,
    // the others left as they were.
    for parameter in PartParameter::all() {
        let start = part_with_supplement();
        let (takes, _) = values_of(parameter.kind());
        let Some(value) = takes.into_iter().find(|&value| {
            start
                .value(parameter)
                .is_some_and(|now| now.to_bits() != value.to_bits())
        }) else {
            continue;
        };
        let mut part = start.clone();
        assert!(part.set_value(parameter, value));
        for other in PartParameter::all().filter(|&other| other != parameter) {
            assert_eq!(part.value(other), start.value(other), "{parameter} moved {other}");
        }
    }
}

#[test]
fn the_supplement_is_the_patch_s() {
    let mut part = Part::initial(true);
    let parameter = PartParameter::of_name("supplement.unison").expect("a parameter");
    assert_eq!(part.value(parameter), None);
    assert!(!part.set_value(parameter, 1.0));
    assert_eq!(part, Part::initial(true));
    // The voice's algorithm is read and set as its number, 1 to 32.
    let algorithm = PartParameter::of_name("voice.algorithm").expect("a parameter");
    assert_eq!(part.value(algorithm), Some(1.0));
    assert!(part.set_value(algorithm, 32.0));
    assert_eq!(part.patch.voice.algorithm.number(), 32);
    // OP3's switch is OP3's alone.
    let switch = PartParameter::of_name("voice.op3.on").expect("a parameter");
    assert!(part.set_value(switch, 0.0));
    let on: Vec<_> = part.patch.voice.operators.iter().map(|operator| operator.on).collect();
    assert_eq!(on, [true, true, false, true, true, true]);
}
