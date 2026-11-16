// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The range types of a voice, and the DX7's numbers for its parameters,
//! against \[DEXED-SYSEX\], which `common` and this file state again by
//! themselves: the range of every parameter, and the order of the steps of each
//! enumeration. A value
//! is a number of its range and nothing else, and rounds to a step as
//! `syx.export_rounding` says (docs/MATH.md §17).

mod common;

use keptone_model::voice::{Algorithm, Curve, Mode, Name, Parameter, Rounding, Value, Wave};

#[test]
fn every_parameter_has_its_range() {
    for parameter in Parameter::all() {
        // The switches, 155, take six bits.
        let number = usize::from(parameter.number());
        let largest = if number < 155 { common::largest(number) } else { 0x3f };
        assert_eq!(parameter.largest(), largest, "{parameter}");
    }
    assert_eq!(Parameter::all().count(), 156);
    assert_eq!(Parameter::COUNT, 156);
    assert_eq!(Parameter::IN_DUMP, 155);
    assert!(Parameter::new(155).is_some());
    assert!(Parameter::new(156).is_none());
    assert!(Parameter::new(255).is_none());
}

#[test]
fn parameters_are_named() {
    let named = |number| Parameter::new(number).expect("a parameter").to_string();
    assert_eq!(named(0), "OP6 EG rate 1");
    assert_eq!(named(7), "OP6 EG level 4");
    assert_eq!(named(20), "OP6 detune");
    assert_eq!(named(21 + 11), "OP5 left curve");
    assert_eq!(named(105 + 16), "OP1 output level");
    assert_eq!(named(125), "OP1 detune");
    assert_eq!(named(126), "pitch EG rate 1");
    assert_eq!(named(133), "pitch EG level 4");
    assert_eq!(named(134), "algorithm");
    assert_eq!(named(142), "LFO wave");
    assert_eq!(named(144), "transpose");
    assert_eq!(named(145), "name byte 1");
    assert_eq!(named(154), "name byte 10");
    assert_eq!(named(155), "operator switches");
}

#[test]
fn values_are_numbers_of_their_range() {
    assert_eq!(Value::<99>::new(0.0).map(Value::get), Some(0.0));
    assert_eq!(Value::<99>::new(42.25).map(Value::get), Some(42.25));
    assert_eq!(Value::<99>::new(99.0).map(Value::get), Some(99.0));
    // −0 is taken as +0, so that equal values have equal bits.
    assert_eq!(Value::<99>::new(-0.0).map(|value| value.get().to_bits()), Some(0));
    for outside in [
        99.000_000_000_000_01,
        -f64::from_bits(1),
        -1.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        assert!(Value::<99>::new(outside).is_none(), "{outside}");
    }
    assert!(Value::<7>::new(7.5).is_none());
    assert_eq!(Value::<7>::from_step(7), Value::new(7.0));
    assert!(Value::<7>::from_step(8).is_none());
    assert!(Value::<48>::from_step(49).is_none());
}

#[test]
fn values_round_to_steps() {
    // The value, its nearest step (a value halfway to the even one), and the
    // step at or below it.
    let cases = [
        (0.0, 0, 0),
        (0.25, 0, 0),
        (0.5, 0, 0),
        (1.5, 2, 1),
        (2.5, 2, 2),
        (3.5, 4, 3),
        (41.75, 42, 41),
        (98.5, 98, 98),
        (98.500_000_000_000_01, 99, 98),
        (98.999_999_999_999_99, 99, 98),
        (99.0, 99, 99),
    ];
    for (value, nearest, floor) in cases {
        let value = Value::<99>::new(value).expect("a value");
        assert_eq!(value.step(Rounding::Nearest), nearest, "{value:?}");
        assert_eq!(value.step(Rounding::Floor), floor, "{value:?}");
    }
    assert_eq!(Rounding::default(), Rounding::Nearest);
}

#[test]
fn enumerations_take_the_dumps_steps() {
    let curves: Vec<_> = (0..=4).map(Curve::from_step).collect();
    assert_eq!(
        curves,
        [
            Some(Curve::NegativeLinear),
            Some(Curve::NegativeExponential),
            Some(Curve::PositiveExponential),
            Some(Curve::PositiveLinear),
            None
        ]
    );
    let modes: Vec<_> = (0..=2).map(Mode::from_step).collect();
    assert_eq!(modes, [Some(Mode::Ratio), Some(Mode::Fixed), None]);
    let waves: Vec<_> = (0..=6).map(Wave::from_step).collect();
    assert_eq!(
        waves,
        [
            Some(Wave::Triangle),
            Some(Wave::SawDown),
            Some(Wave::SawUp),
            Some(Wave::Square),
            Some(Wave::Sine),
            Some(Wave::SampleAndHold),
            None
        ]
    );
    for step in 0..=u8::MAX {
        if let Some(curve) = Curve::from_step(step) {
            assert_eq!(curve.step(), step);
        }
        if let Some(mode) = Mode::from_step(step) {
            assert_eq!(mode.step(), step);
        }
        if let Some(wave) = Wave::from_step(step) {
            assert_eq!(wave.step(), step);
        }
        // The algorithm's step is one less than its number.
        assert_eq!(
            Algorithm::from_step(step).map(Algorithm::number),
            (step < 32).then(|| step + 1)
        );
        assert_eq!(
            Algorithm::from_step(step).map(Algorithm::step),
            (step < 32).then_some(step)
        );
        assert_eq!(
            Algorithm::new(step).map(Algorithm::number),
            (1..=32).contains(&step).then_some(step)
        );
    }
}

#[test]
fn names_are_bytes_below_128() {
    let name = *b"E.PIANO 1 ";
    assert_eq!(Name::new(name).map(Name::bytes), Some(name));
    assert!(Name::new([0; 10]).is_some());
    assert!(Name::new([0x7f; 10]).is_some());
    let mut high = name;
    high[9] = 0x80;
    assert!(Name::new(high).is_none());
}
