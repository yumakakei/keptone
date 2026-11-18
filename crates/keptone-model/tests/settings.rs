// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The settings against docs/MATH.md §17, whose table this file reads: every
//! row a setting, in its order, its name, its values and its default as the
//! table gives them; and each setting takes what its values are, and nothing
//! else.

use keptone_model::id::{Area, ParameterId};
use keptone_model::settings::{self, Bounds, Kind, SPECS, Settings, Value};

/// The rows of MATH §17's table: each setting's name, default and values, as
/// the table writes them.
fn table() -> Vec<(String, String, String)> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/MATH.md");
    let math = std::fs::read_to_string(path).expect("docs/MATH.md");
    let start = math.find("## 17. Settings index").expect("MATH §17");
    math[start..]
        .lines()
        .filter(|line| line.starts_with("| `"))
        .map(|line| {
            let cells: Vec<&str> = line.split(" | ").collect();
            let name = cells[0].trim_start_matches("| `").trim_end_matches('`');
            (name.to_owned(), cells[1].to_owned(), cells[2].to_owned())
        })
        .collect()
}

/// The settings that are counts, which the table writes as "≥ 1" or "≥ 0"
/// as it writes a number.
const COUNTS: [&str; 5] = [
    "graph.operators",
    "feedback.max_iterations",
    "voice.count",
    "part.count",
    "json.max_depth",
];

/// A number as the table writes a default: with a minus sign, a power of two,
/// a quotient, and a note after it.
fn number_of(text: &str) -> f64 {
    let text = text
        .trim_end_matches(" (provisional)")
        .trim_end_matches(" dB")
        .replace('−', "-");
    match text.as_str() {
        "2⁻⁵³" => 2.0_f64.powi(-53),
        "2⁻¹⁰" => 2.0_f64.powi(-10),
        _ => match text.split_once('/') {
            Some((numerator, denominator)) => {
                numerator.parse::<f64>().expect("a numerator") / denominator.parse::<f64>().expect("a denominator")
            }
            None => text.parse().expect("a number"),
        },
    }
}

#[test]
fn every_setting_of_math_17() {
    let table = table();
    assert_eq!(table.len(), SPECS.len());
    for ((name, default, values), spec) in table.iter().zip(SPECS) {
        assert_eq!(name, spec.name);
        match (values.as_str(), spec.kind) {
            ("bool", Kind::Switch) => assert_eq!(spec.default, Value::Switch(default == "true"), "{name}"),
            ("u64", Kind::Seed) => assert_eq!(spec.default, Value::Integer(default.parse().expect("a seed"))),
            ("1, 2, 4, …", Kind::PowerOfTwo) => {
                assert_eq!(spec.default, Value::Integer(default.parse().expect("a power")));
            }
            (bound @ ("≥ 0" | "≥ 1"), Kind::Count(least)) if COUNTS.contains(&name.as_str()) => {
                assert_eq!(bound.ends_with('1'), least == 1, "{name}");
                assert_eq!(
                    spec.default,
                    Value::Integer(default.parse().expect("a count")),
                    "{name}"
                );
            }
            (bound, Kind::Number(bounds) | Kind::NumberOrAutomatic(bounds)) => {
                let expected = match bound {
                    "> 0" => Bounds::Positive,
                    "≥ 0" => Bounds::NonNegative,
                    "< 0" => Bounds::Negative,
                    "(0, 1)" => Bounds::Open,
                    "[0, 1)" => Bounds::HalfOpen,
                    other => panic!("{name}: bounds {other}"),
                };
                assert_eq!(bounds, expected, "{name}");
                if default.starts_with("min(") {
                    assert_eq!(
                        (spec.kind, spec.default),
                        (Kind::NumberOrAutomatic(bounds), Value::Automatic)
                    );
                } else {
                    assert_eq!(spec.default, Value::Number(number_of(default)), "{name}");
                }
            }
            (choices, Kind::Choice(names)) => {
                let listed: Vec<&str> = choices.split(" / ").collect();
                assert_eq!(listed, names, "{name}");
                let default = default.trim_end_matches(" (provisional)");
                let place = names.iter().position(|&choice| choice == default).expect("the default");
                assert_eq!(
                    spec.default,
                    Value::Choice(u8::try_from(place).expect("a few")),
                    "{name}"
                );
            }
            (values, kind) => panic!("{name}: {values} against {kind:?}"),
        }
        assert!(spec.takes(spec.default), "{name}");
    }
}

#[test]
fn numbers_names_and_ids() {
    for (number, spec) in (0..).zip(SPECS) {
        assert_eq!(settings::number_of(spec.name), Some(number));
        let id = ParameterId::new(Area::Setting, 0, number).expect("an ID");
        assert_eq!((id.area(), id.part(), id.number()), (Area::Setting, 0, number));
    }
    assert_eq!(settings::number_of("voice.name"), None);
    let settings = Settings::default();
    for (number, spec) in (0..).zip(SPECS) {
        assert_eq!(settings.get(number), Some(spec.default));
    }
    assert_eq!(settings.get(55), None);
}

/// The number of a setting of a name.
fn number(name: &str) -> u16 {
    settings::number_of(name).expect("a setting")
}

#[test]
fn each_setting_takes_its_values() {
    let mut settings = Settings::new();
    let cases = [
        ("tuning.a4_hz", Value::Number(432.0), true),
        ("tuning.a4_hz", Value::Number(0.0), false),
        ("tuning.a4_hz", Value::Number(f64::NAN), false),
        ("tuning.a4_hz", Value::Number(f64::INFINITY), false),
        ("tuning.a4_hz", Value::Automatic, false),
        ("tuning.a4_hz", Value::Switch(true), false),
        ("eg.end_db", Value::Number(-1.0), true),
        ("eg.end_db", Value::Number(0.0), false),
        ("feedback.beta_clamp", Value::Number(0.5), true),
        ("feedback.beta_clamp", Value::Number(1.0), false),
        ("feedback.beta_clamp", Value::Number(0.0), false),
        ("feedback.series_beta_max", Value::Number(0.0), true),
        ("feedback.series_beta_max", Value::Number(1.0), false),
        ("adaa.droop_eq_hz", Value::Number(18000.0), true),
        ("adaa.droop_eq_hz", Value::Automatic, true),
        ("engine.oversampling", Value::Integer(4), true),
        ("engine.oversampling", Value::Integer(3), false),
        ("engine.oversampling", Value::Integer(0), false),
        ("voice.count", Value::Integer(0), false),
        ("voice.count", Value::Integer(1), true),
        ("json.max_depth", Value::Integer(0), true),
        ("lfo.seed", Value::Integer(u64::MAX), true),
        ("voice.steal", Value::Choice(3), true),
        ("voice.steal", Value::Choice(4), false),
        ("scaling.exact", Value::Switch(true), true),
        ("scaling.exact", Value::Choice(1), false),
    ];
    for (name, value, takes) in cases {
        let before = settings.clone();
        assert_eq!(settings.set(number(name), value), takes, "{name} = {value:?}");
        if takes {
            assert_eq!(settings.get(number(name)), Some(value));
        } else {
            assert_eq!(settings, before, "{name} = {value:?}");
        }
    }
    assert!(!settings.set(55, Value::Switch(true)));
    // −0 is taken as +0.
    assert!(settings.set(number("output.gain"), Value::Number(-0.0)));
    let Some(Value::Number(gain)) = settings.get(number("output.gain")) else {
        panic!("a number");
    };
    assert!(gain.is_sign_positive());
}
