// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The native state (docs/STATE-FORMAT.md), held to the document and to states
//! written apart from keptone-model: fuzz/seeds/state/ holds states that
//! `scripts/native_state.py` writes from the document's tables and MATH §17's
//! defaults alone, with digests of its own canonical form, which V8's
//! `JSON.stringify` gives the same. The state Keptone starts in is the first of
//! them byte for byte; each of them is read with nothing found and written
//! back byte for byte; states of random values come back as they were; what
//! the document says a reader refuses, it refuses, where the document says;
//! and what it does not know, it keeps.

mod common;

use std::path::Path;

use common::SplitMix64;
use keptone_model::dx7ii::Offset;
use keptone_model::id::{Area, Kind, PartParameter};
use keptone_model::json::DEFAULT_MAX_DEPTH;
use keptone_model::settings::{self, Bounds, SPECS};
use keptone_model::state::{self, Finding, ScalaFile, State, Tuning};
use keptone_model::voice::{Name, Value};

/// A seed of the state target, which `scripts/native_state.py` wrote.
fn seed(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fuzz/seeds/state")
        .join(name);
    std::fs::read(path).expect("a seed of fuzz/seeds/state/")
}

/// A state of a text in which nothing is found.
fn read(bytes: &[u8]) -> State {
    let (state, findings) = state::read(bytes, DEFAULT_MAX_DEPTH).expect("a state");
    assert_eq!(findings, []);
    state
}

/// The number of a setting.
fn setting(name: &str) -> u16 {
    settings::number_of(name).expect("a setting")
}

/// The initial state's text, changed by replacing the first of a text with
/// another.
fn initial_with(from: &str, to: &str) -> String {
    let text = String::from_utf8(seed("initial.json")).expect("UTF-8");
    assert!(text.contains(from), "{from}");
    text.replacen(from, to, 1)
}

/// The error of reading a text.
fn refusal(text: &str) -> state::Error {
    state::read(text.as_bytes(), DEFAULT_MAX_DEPTH).expect_err("refused")
}

#[test]
fn keptone_starts_in_the_state_of_the_document() {
    let initial = State::new();
    assert_eq!(state::write(&initial).as_bytes(), seed("initial.json"));
    assert_eq!(read(&seed("initial.json")), initial);
    assert_eq!(initial, State::default());
    assert_eq!(initial.parts().len(), 2);
    assert!(initial.parts()[0].on && !initial.parts()[1].on);
    assert_eq!(initial.tuning, Tuning::default());
}

#[test]
fn states_written_apart_come_back_byte_for_byte() {
    for name in ["initial.json", "random.json", "single.json"] {
        let bytes = seed(name);
        assert_eq!(state::write(&read(&bytes)).as_bytes(), bytes, "{name}");
    }
    let random = read(&seed("random.json"));
    let patches: Vec<(bool, bool)> = random
        .parts()
        .iter()
        .map(|part| (part.patch.supplement.is_some(), part.patch.fractional_scaling.is_some()))
        .collect();
    assert_eq!(patches, [(true, false), (false, true), (true, true)]);
    let scale = random.tuning.scale.as_ref().expect("a scale");
    assert_eq!(scale.value().pitches.len(), 12);
    assert!(scale.text().starts_with("! equal.scl"));
    assert!(random.tuning.keyboard_mapping.is_some());
    assert_eq!(read(&seed("single.json")).parts().len(), 1);
}

/// A binary64 of [0, 1).
fn unit(random: &mut SplitMix64) -> f64 {
    f64::from_bits(random.next_u64() >> 12 | 0x3ff0_0000_0000_0000) - 1.0
}

/// A random integer from `least` to `most`, as a binary64.
fn integer(random: &mut SplitMix64, least: u32, most: u32) -> f64 {
    let span = usize::try_from(most - least).expect("a span") + 1;
    f64::from(least + u32::try_from(random.below(span)).expect("below 2³²"))
}

/// A random value of a parameter's kind: an end, an integer or any binary64 of
/// its scale, or a number beyond the scale.
fn random_value(random: &mut SplitMix64, kind: Kind) -> f64 {
    let number = |random: &mut SplitMix64, least: f64, most: f64| match random.below(6) {
        0 => least,
        1 => most,
        2 => (least + unit(random) * (most - least + 1.0)).floor().min(most),
        3 => least + unit(random) * (most - least),
        // Beyond the scale's ends, as far as 10³⁰⁰ (docs/MATH.md §0.5).
        4 => most + 10.0_f64.powf(unit(random) * 300.0),
        _ => least - 10.0_f64.powf(unit(random) * 300.0),
    };
    match kind {
        Kind::Number(most) => number(random, 0.0, most),
        Kind::Signed(most) => number(random, -most, most),
        Kind::Integer(least, most) => integer(random, least, most),
        Kind::Count => integer(random, 0, u32::MAX),
        Kind::Choice(names) => integer(random, 0, u32::try_from(names.len()).expect("a few names") - 1),
        Kind::Switch => integer(random, 0, 1),
    }
}

/// A random value of a setting, one it takes.
fn random_setting(random: &mut SplitMix64, kind: settings::Kind) -> settings::Value {
    let magnitude = 10.0_f64.powf(unit(random) * 12.0 - 6.0);
    let number = |random: &mut SplitMix64, bounds: Bounds| match bounds {
        Bounds::NonNegative if random.below(2) == 0 => 0.0,
        Bounds::Positive | Bounds::NonNegative => magnitude,
        Bounds::Negative => -magnitude,
        Bounds::Open => unit(random).max(f64::MIN_POSITIVE),
        Bounds::HalfOpen => unit(random),
    };
    match kind {
        settings::Kind::Switch => settings::Value::Switch(random.below(2) == 1),
        settings::Kind::Choice(names) => {
            settings::Value::Choice(u8::try_from(random.below(names.len())).expect("a few names"))
        }
        settings::Kind::NumberOrAutomatic(_) if random.below(2) == 0 => settings::Value::Automatic,
        settings::Kind::Number(bounds) | settings::Kind::NumberOrAutomatic(bounds) => {
            settings::Value::Number(number(random, bounds))
        }
        settings::Kind::Count(least) => settings::Value::Integer(least + random.next_u64() % 1000),
        settings::Kind::PowerOfTwo => settings::Value::Integer(1 << random.below(10)),
        settings::Kind::Seed => settings::Value::Integer(random.next_u64()),
    }
}

/// A state of random values: every setting, one to four parts, patches with
/// and without the supplements and the fractional scalings of `template`'s,
/// names of any seven bits, and Scala files or none.
fn random_state(random: &mut SplitMix64, template: &State) -> State {
    let mut state = State::new();
    for (number, spec) in (0_u16..).zip(SPECS.iter()) {
        let value = random_setting(random, spec.kind);
        assert!(state.set_setting(number, value), "{}", spec.name);
    }
    let count = 1 + random.next_u64() % 4;
    assert!(state.set_setting(setting("part.count"), settings::Value::Integer(count)));
    for (index, part) in state.parts_mut().iter_mut().enumerate() {
        let patch = &template.parts()[(index + random.below(3)) % 3].patch;
        part.patch.supplement.clone_from(&patch.supplement);
        part.patch.fractional_scaling.clone_from(&patch.fractional_scaling);
        for parameter in PartParameter::all() {
            let value = random_value(random, parameter.kind());
            let has = parameter.area() != Area::Supplement || part.patch.supplement.is_some();
            assert_eq!(part.set_value(parameter, value), has, "{parameter} {value}");
        }
        let name: [u8; 10] = std::array::from_fn(|_| random.byte() & 0x7f);
        part.patch.voice.name = Name::new(name).expect("a name");
        if let Some(scaling) = &mut part.patch.fractional_scaling {
            for operator in &mut scaling.operators {
                let offset = random_value(random, Kind::Number(255.0)) - 128.0;
                operator.offset = Offset::new(offset).expect("an offset");
                for level in &mut operator.levels {
                    *level = Value::new(random_value(random, Kind::Number(255.0))).expect("a level");
                }
            }
        }
    }
    state.tuning.scale =
        (random.below(2) == 0).then(|| ScalaFile::scale("! a scale\nOne step\n1\n2/1\n".to_owned()).expect("a scale"));
    state.tuning.keyboard_mapping = (random.below(2) == 0)
        .then(|| ScalaFile::keyboard_mapping("0\n0\n127\n60\n69\n440.0\n0\n".to_owned()).expect("a mapping"));
    state
}

#[test]
fn random_states_come_back_as_they_were() {
    let rounds = if cfg!(debug_assertions) { 100 } else { 1000 };
    let template = read(&seed("random.json"));
    let mut random = SplitMix64(23);
    for _ in 0..rounds {
        let state = random_state(&mut random, &template);
        let written = state::write(&state);
        let again = read(written.as_bytes());
        assert_eq!(again, state);
        assert_eq!(state::write(&again), written);
    }
}

/// Changes of the initial state's text that a reader refuses: the text
/// changed, what it becomes, and the place and the message of the refusal.
const REFUSED: [(&str, &str, &str, &str); 29] = [
    (
        "\"keptone-state\"",
        "\"keptone\"",
        "/format",
        "a format other than keptone-state",
    ),
    (
        "\"version\": 1",
        "\"version\": 0",
        "/version",
        "a version that is not an integer above 0",
    ),
    (
        "\"version\": 1",
        "\"version\": 1.0",
        "/version",
        "a version that is not an integer above 0",
    ),
    (
        "\"version\": 1",
        "\"version\": \"1\"",
        "/version",
        "a version that is not an integer above 0",
    ),
    (
        "\"version\": 1",
        "\"version\": 18446744073709551616",
        "/version",
        "a version that is not an integer above 0",
    ),
    (
        "\"settings\": {",
        "\"settings\": [], \"x\": {",
        "/settings",
        "a value that is not an object",
    ),
    (
        "\"tuning.a4_hz\": 440",
        "\"tuning.a4_hz\": -440",
        "/settings/tuning.a4_hz",
        "a value that the setting does not take",
    ),
    (
        "\"tuning.a4_hz\": 440",
        "\"tuning.a4_hz\": 1e400",
        "/settings/tuning.a4_hz",
        "a value that the setting does not take",
    ),
    (
        "\"adaa.order\": \"2\"",
        "\"adaa.order\": 2",
        "/settings/adaa.order",
        "a value that the setting does not take",
    ),
    (
        "\"lfo.seed\": 0",
        "\"lfo.seed\": 0.0",
        "/settings/lfo.seed",
        "a value that the setting does not take",
    ),
    (
        "\"engine.oversampling\": 2",
        "\"engine.oversampling\": 3",
        "/settings/engine.oversampling",
        "a value that the setting does not take",
    ),
    (
        "\"adaa.droop_eq_hz\": \"auto\"",
        "\"adaa.droop_eq_hz\": \"off\"",
        "/settings/adaa.droop_eq_hz",
        "a value that the setting does not take",
    ),
    (
        "\"part.count\": 2",
        "\"part.count\": 3",
        "/parts",
        "a count of parts other than part.count",
    ),
    (
        "\"part.count\": 2",
        "\"part.count\": 1",
        "/parts",
        "a count of parts other than part.count",
    ),
    (
        "\"scale\": null",
        "\"scale\": 5",
        "/tuning/scale",
        "a value that is neither a string nor null",
    ),
    (
        "\"scale\": null",
        "\"scale\": \"a scale\\n12\\n100.0\\n\"",
        "/tuning/scale",
        "fewer pitches than the count of notes, at the Scala file's line 4",
    ),
    (
        "\"keyboard_mapping\": null",
        "\"keyboard_mapping\": \"\"",
        "/tuning/keyboard_mapping",
        "a file without its size of map, at the Scala file's line 1",
    ),
    (
        "\"parts\": [",
        "\"parts\": {}, \"x\": [",
        "/parts",
        "a value that is not an array",
    ),
    (
        "\"voice.op6.eg.rate.1\": 99",
        "\"voice.op6.eg.rate.1\": 99.5e400",
        "/parts/0/voice.op6.eg.rate.1",
        "a number that the parameter does not take",
    ),
    (
        "\"voice.op6.eg.rate.1\": 99",
        "\"voice.op6.eg.rate.1\": \"99\"",
        "/parts/0/voice.op6.eg.rate.1",
        "a value that is not a number",
    ),
    (
        "\"voice.algorithm\": 1",
        "\"voice.algorithm\": 1.5",
        "/parts/0/voice.algorithm",
        "a number that the parameter does not take",
    ),
    (
        "\"voice.algorithm\": 1",
        "\"voice.algorithm\": 0",
        "/parts/0/voice.algorithm",
        "a number that the parameter does not take",
    ),
    (
        "\"voice.lfo.wave\": \"triangle\"",
        "\"voice.lfo.wave\": \"Triangle\"",
        "/parts/0/voice.lfo.wave",
        "a value that is none of the parameter's names",
    ),
    (
        "\"part.on\": true",
        "\"part.on\": 1",
        "/parts/0/part.on",
        "a value that is neither true nor false",
    ),
    (
        "\"part.note_shift\": 0",
        "\"part.note_shift\": -1e400",
        "/parts/0/part.note_shift",
        "a number that the parameter does not take",
    ),
    (
        "\"part.note_limit\": 0",
        "\"part.note_limit\": 4294967296",
        "/parts/0/part.note_limit",
        "a number that the parameter does not take",
    ),
    (
        "\"voice.name\": \"INIT VOICE\"",
        "\"voice.name\": \"INIT\"",
        "/parts/0/voice.name",
        "a name that is not ten characters, each below U+0080",
    ),
    (
        "\"voice.name\": \"INIT VOICE\"",
        "\"voice.name\": \"INIT VOICé\"",
        "/parts/0/voice.name",
        "a name that is not ten characters, each below U+0080",
    ),
    (
        "\"patch.sha256\": \"",
        "\"patch.sha256\": 7, \"x\": \"",
        "/parts/0/patch.sha256",
        "a digest that is not a string",
    ),
];

#[test]
fn what_a_reader_refuses_and_what_it_says() {
    for (from, to, pointer, message) in REFUSED {
        let error = refusal(&initial_with(from, to));
        assert_eq!(
            (error.pointer.as_str(), error.message.as_str()),
            (pointer, message),
            "{to}"
        );
    }
}

#[test]
fn a_refusal_says_where() {
    // The member's name, where its value is refused.
    let error = refusal(&initial_with(
        "\"voice.op6.eg.rate.1\": 99",
        "\"voice.op6.eg.rate.1\": 1e400",
    ));
    assert_eq!((error.line, error.column), (97, 7));
    assert_eq!(
        error.to_string(),
        "a number that the parameter does not take at /parts/0/voice.op6.eg.rate.1, line 97, column 7"
    );
    // The opening brace of the object that lacks a member.
    let error = refusal(&initial_with("    \"lfo.seed\": 0,\n", ""));
    assert_eq!(
        (error.pointer.as_str(), error.line, error.column),
        ("/settings/lfo.seed", 4, 15)
    );
    assert_eq!(error.message, "a member that the state lacks");
    let error = refusal(&initial_with("      \"part.on\": false,\n", ""));
    assert_eq!(
        (error.pointer.as_str(), error.line, error.column),
        ("/parts/1/part.on", 251, 5)
    );
    // The text itself, where it is not JSON, or not an object.
    let error = refusal("{\"format\": }");
    assert_eq!((error.pointer.as_str(), error.line, error.column), ("", 1, 12));
    assert_eq!(
        error.to_string(),
        "a character that begins no value at line 1, column 12"
    );
    let error = refusal("\n  [1]");
    assert_eq!(
        (error.pointer.as_str(), error.message.as_str()),
        ("", "a text that is not a JSON object")
    );
    assert_eq!((error.line, error.column), (2, 3));
    let error = refusal("{}");
    assert_eq!((error.pointer.as_str(), error.offset), ("/format", 0));
    // A part that is not an object is placed at the parts' member.
    let text = initial_with("\"parts\": [\n    {", "\"parts\": [7, {\"x\": 1}],\n  \"y\": [{");
    let error = refusal(&text);
    assert_eq!(
        (error.pointer.as_str(), error.message.as_str()),
        ("/parts/0", "a part that is not an object")
    );
    // Nesting beyond the limit is JSON's to refuse.
    let initial = seed("initial.json");
    assert!(state::read(&initial, 3).is_ok());
    assert_eq!(state::read(&initial, 2).expect_err("too deep").pointer, "");
}

/// The text of a part's member, its name and its value.
fn member(name: &str, value: &str) -> String {
    format!("      \"{name}\": {value},\n")
}

#[test]
fn a_supplement_and_a_fractional_scaling_are_there_whole_or_not() {
    let digest = "      \"patch.sha256\"";
    // One member of the supplement asks for every other.
    let text = initial_with(digest, &format!("{}{digest}", member("supplement.unison", "true")));
    let error = refusal(&text);
    assert_eq!(error.pointer, "/parts/0/supplement.op1.scaling_mode");
    assert_eq!(error.message, "a member that the state lacks");
    // So does one of the fractional scaling.
    let levels = format!("[{}]", vec!["0"; 40].join(", "));
    let text = initial_with(
        digest,
        &format!("{}{digest}", member("fractional_scaling.op3.levels", &levels)),
    );
    assert_eq!(refusal(&text).pointer, "/parts/0/fractional_scaling.op1.offset");
    // A whole fractional scaling is read, its offsets any number from −128 to
    // +127, −0 taken as 0.
    let mut scaling = String::new();
    for (operator, offset) in (1..=6).zip(["-128", "-1.5", "0", "0.25", "127", "-0"]) {
        scaling += &member(&format!("fractional_scaling.op{operator}.offset"), offset);
        scaling += &member(&format!("fractional_scaling.op{operator}.levels"), &levels);
    }
    let (state, findings) = state::read(
        initial_with(digest, &format!("{scaling}{digest}")).as_bytes(),
        DEFAULT_MAX_DEPTH,
    )
    .expect("a state");
    assert!(matches!(findings.as_slice(), [Finding::Digest { part: 0, .. }]));
    let offsets: Vec<u64> = state.parts()[0]
        .patch
        .fractional_scaling
        .as_ref()
        .expect("a fractional scaling")
        .operators
        .iter()
        .map(|operator| operator.offset.get().to_bits())
        .collect();
    assert_eq!(offsets, [-128.0, -1.5, 0.0, 0.25, 127.0, 0.0].map(f64::to_bits));
}

#[test]
fn a_fractional_scaling_s_members_are_refused_where_wrong() {
    let digest = "      \"patch.sha256\"";
    let levels = format!("[{}]", vec!["0"; 40].join(", "));
    let refused = |offset: &str, first_levels: &str| {
        let mut text = String::new();
        for operator in 1..=6 {
            let (offset, operator_levels) = if operator == 1 {
                (offset, first_levels)
            } else {
                ("0", levels.as_str())
            };
            text += &member(&format!("fractional_scaling.op{operator}.offset"), offset);
            text += &member(&format!("fractional_scaling.op{operator}.levels"), operator_levels);
        }
        refusal(&initial_with(digest, &format!("{text}{digest}")))
    };
    let mut beyond = vec!["0"; 40];
    beyond[3] = "-1e400";
    let beyond = format!("[{}]", beyond.join(", "));
    let forty_one = format!("[{}]", vec!["0"; 41].join(", "));
    let offset = "an offset that is not a finite number";
    for (offset_text, first_levels, pointer, message) in [
        (
            "1e400",
            levels.as_str(),
            "/parts/0/fractional_scaling.op1.offset",
            offset,
        ),
        (
            "\"0\"",
            levels.as_str(),
            "/parts/0/fractional_scaling.op1.offset",
            offset,
        ),
        (
            "0",
            "[1]",
            "/parts/0/fractional_scaling.op1.levels",
            "a value that is not an array of 40 levels",
        ),
        (
            "0",
            "{}",
            "/parts/0/fractional_scaling.op1.levels",
            "a value that is not an array of 40 levels",
        ),
        (
            "0",
            forty_one.as_str(),
            "/parts/0/fractional_scaling.op1.levels",
            "a value that is not an array of 40 levels",
        ),
        (
            "0",
            beyond.as_str(),
            "/parts/0/fractional_scaling.op1.levels/3",
            "a level that is not a finite number",
        ),
    ] {
        let error = refused(offset_text, first_levels);
        assert_eq!(
            (error.pointer.as_str(), error.message.as_str()),
            (pointer, message),
            "{offset_text}"
        );
    }
}

#[test]
fn a_digest_identifies_the_patch_whatever_its_text() {
    let initial = String::from_utf8(seed("initial.json")).expect("UTF-8");
    // STATE-FORMAT.md §7 gives the VOICE INIT's digest.
    let document = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/STATE-FORMAT.md"))
        .expect("docs/STATE-FORMAT.md");
    let start = document.find("the patch a part starts with, is `").expect("the digest") + 34;
    let digest = &document[start..start + 64];
    assert!(initial.contains(&format!("\"patch.sha256\": \"{digest}\"")));
    // Its members in another order and their numbers written otherwise have
    // the same digest.
    let moved = initial
        .replacen("      \"voice.op6.eg.rate.1\": 99,\n", "", 1)
        .replacen(
            "      \"voice.name\"",
            "      \"voice.op6.eg.rate.1\": 9.90e1,\n      \"voice.name\"",
            1,
        )
        .replacen(
            "\"voice.op1.output_level\": 99",
            "\"voice.op1.output_level\": 99.000",
            1,
        );
    assert_ne!(moved, initial);
    assert_eq!(state::write(&read(moved.as_bytes())), initial);
    // A digest that is not the patch's is reported, and written right.
    let wrong = initial.replacen(digest, "not a digest", 1);
    let (state, findings) = state::read(wrong.as_bytes(), DEFAULT_MAX_DEPTH).expect("a state");
    assert_eq!(
        findings,
        [Finding::Digest {
            part: 0,
            found: "not a digest".to_owned(),
            computed: digest.to_owned(),
        }]
    );
    assert_eq!(state::write(&state), initial);
    // A patch changed has another digest.
    let changed = initial.replacen("\"voice.feedback\": 0", "\"voice.feedback\": 7", 1);
    let (_, findings) = state::read(changed.as_bytes(), DEFAULT_MAX_DEPTH).expect("a state");
    assert!(matches!(findings.as_slice(), [Finding::Digest { part: 0, .. }]));
    // What a part has of its own, and its function data, are not the patch's.
    let own = initial.replacen("\"part.omni\": true", "\"part.omni\": false", 1);
    let own = own.replacen("\"function.glissando\": false", "\"function.glissando\": true", 1);
    assert_eq!(state::read(own.as_bytes(), DEFAULT_MAX_DEPTH).expect("a state").1, []);
}

#[test]
fn what_a_reader_does_not_know_it_keeps() {
    let text = initial_with("\"version\": 1", "\"version\": 7")
        .replacen("\"format\"", "\"later\": {\"b\": [1E2, -0]},\n  \"format\"", 1)
        .replacen(
            "    \"tuning.a4_hz\"",
            "    \"settings.later\": \"x\",\n    \"tuning.a4_hz\"",
            1,
        )
        .replacen("    \"scale\"", "    \"tuning.later\": null,\n    \"scale\"", 1)
        .replacen(
            "      \"part.on\": false",
            "      \"part.later\": {},\n      \"part.on\": false",
            1,
        );
    let state = read(text.as_bytes());
    let written = state::write(&state);
    // Each object's are written after what the reader knows, as they were
    // read, and the version is the writer's.
    assert!(written.starts_with("{\n  \"format\": \"keptone-state\",\n  \"version\": 1,\n"));
    assert!(written.ends_with("  ],\n  \"later\": {\n    \"b\": [1E2, -0]\n  }\n}\n"));
    // The settings' after the last setting, whichever it is.
    let last = format!("    \"{}\": ", SPECS.last().expect("a setting").name);
    let (_, after) = written.split_once(&last).expect("the last setting");
    assert!(
        after
            .split_once('\n')
            .is_some_and(|(_, next)| next.starts_with("    \"settings.later\": \"x\"\n  },"))
    );
    assert!(written.contains("    \"keyboard_mapping\": null,\n    \"tuning.later\": null\n  },"));
    assert!(written.contains("\",\n      \"part.later\": {}\n    }\n  ],"));
    assert_eq!(read(written.as_bytes()), state);
    // A member of a patch that the reader does not know is in its digest.
    let later = initial_with(
        "      \"patch.sha256\"",
        "      \"voice.later\": 1,\n      \"patch.sha256\"",
    );
    let (_, findings) = state::read(later.as_bytes(), DEFAULT_MAX_DEPTH).expect("a state");
    assert!(matches!(findings.as_slice(), [Finding::Digest { part: 0, .. }]));
    // A part taken away goes with what it kept; one added has nothing kept.
    let mut fewer = state.clone();
    assert!(fewer.set_setting(setting("part.count"), settings::Value::Integer(1)));
    assert!(!state::write(&fewer).contains("part.later"));
    assert!(fewer.set_setting(setting("part.count"), settings::Value::Integer(2)));
    assert_eq!(fewer.parts(), State::new().parts());
    assert!(!state::write(&fewer).contains("part.later"));
}

#[test]
fn the_count_of_parts_follows_part_count() {
    let mut state = State::new();
    let count = setting("part.count");
    assert!(state.set_setting(count, settings::Value::Integer(5)));
    assert_eq!(state.parts().len(), 5);
    assert!(state.parts()[1..].iter().all(|part| !part.on));
    assert!(state.set_setting(count, settings::Value::Integer(1)));
    assert_eq!(state.parts(), &State::new().parts()[..1]);
    // What the setting does not take, and more parts than memory can be had
    // for, leave the state as it was.
    let before = state.clone();
    assert!(!state.set_setting(count, settings::Value::Integer(0)));
    assert!(!state.set_setting(count, settings::Value::Number(2.0)));
    assert!(!state.set_setting(count, settings::Value::Integer(u64::MAX)));
    assert!(!state.set_setting(u16::MAX, settings::Value::Integer(2)));
    assert_eq!(state, before);
    state.parts_mut()[0].on = false;
    assert_ne!(state, before);
}

/// The rows of each table of STATE-FORMAT.md, by the heading of its section,
/// its header rows left out.
fn tables() -> Vec<(String, Vec<String>)> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/STATE-FORMAT.md");
    let document = std::fs::read_to_string(path).expect("docs/STATE-FORMAT.md");
    let mut heading = String::new();
    let mut rows = Vec::new();
    for line in document.lines() {
        if line.starts_with('#') {
            line.trim_start_matches('#').trim().clone_into(&mut heading);
        } else if line.starts_with("| ") && !line.starts_with("| ---") {
            let cells: Vec<String> = line
                .trim_matches('|')
                .split(" | ")
                .map(|cell| cell.trim().to_owned())
                .collect();
            if !["Number", "Place", "Name", "Member"].contains(&cells[0].as_str()) {
                rows.push((heading.clone(), cells));
            }
        }
    }
    rows
}

/// What a table's Values say of a parameter's kind.
fn values_of(kind: Kind) -> String {
    let number = |value: f64| {
        keptone_model::json::Number::from_f64(value)
            .expect("a number")
            .as_text()
            .to_owned()
    };
    match kind {
        Kind::Number(most) => format!("0 to {}", number(most)),
        Kind::Signed(most) => format!("\u{2212}{} to {}", number(most), number(most)),
        Kind::Integer(least, most) => format!("integers {least} to {most}"),
        Kind::Count => "integers 0 to 4294967295".to_owned(),
        Kind::Choice(names) => names
            .iter()
            .map(|name| format!("`\"{name}\"`"))
            .collect::<Vec<_>>()
            .join(", "),
        Kind::Switch => "`true`, `false`".to_owned(),
    }
}

#[test]
fn the_document_lists_every_member_by_its_name_and_number() {
    let rows = tables();
    let of = |heading: &str| -> Vec<&Vec<String>> {
        rows.iter()
            .filter(|(at, _)| at == heading)
            .map(|(_, cells)| cells)
            .collect()
    };
    // The parameters, an operator's by the rule of their numbers.
    let mut listed = Vec::new();
    let voice = of("5.1 The voice");
    for operator in 1..=6_u16 {
        for cells in &voice[..21] {
            let place: u16 = cells[0].parse().expect("a place");
            let name = format!("voice.op{operator}.{}", cells[1].trim_matches('`'));
            listed.push((Area::Voice, 21 * (6 - operator) + place, name, cells[2].clone()));
        }
    }
    let supplement = of("5.3 The supplement");
    for operator in 1..=6_u16 {
        for cells in &supplement[..2] {
            let place: u16 = cells[0].parse().expect("a place");
            let name = format!("supplement.op{operator}.{}", cells[1].trim_matches('`'));
            listed.push((Area::Supplement, 2 * (operator - 1) + place, name, cells[2].clone()));
        }
    }
    for (area, cells) in [(Area::Voice, &voice[21..]), (Area::Supplement, &supplement[2..])]
        .into_iter()
        .chain([
            (Area::Function, &of("5.2 The function data")[..]),
            (Area::Part, &of("5.5 What a part has of its own")[..]),
        ])
    {
        for cells in cells {
            let number = cells[0].parse().expect("a number");
            listed.push((area, number, cells[1].trim_matches('`').to_owned(), cells[2].clone()));
        }
    }
    listed.sort_by_key(|(area, number, _, _)| (*area, *number));
    // No name holds what a JSON Pointer would escape.
    assert!(listed.iter().all(|(_, _, name, _)| !name.contains(['~', '/'])));
    assert!(SPECS.iter().all(|spec| !spec.name.contains(['~', '/'])));
    let every: Vec<(Area, u16, String, String)> = PartParameter::all()
        .map(|parameter| {
            (
                parameter.area(),
                parameter.number(),
                parameter.name(),
                values_of(parameter.kind()),
            )
        })
        .collect();
    assert_eq!(listed, every);
    // The settings, in MATH §17's order, as each is written.
    let settings: Vec<(String, String)> = of("3. Settings")
        .iter()
        .map(|cells| {
            assert_eq!(
                cells[0].parse::<u16>().ok(),
                settings::number_of(cells[1].trim_matches('`'))
            );
            (cells[1].trim_matches('`').to_owned(), cells[2].clone())
        })
        .collect();
    let bounds = |bounds: Bounds| match bounds {
        Bounds::Positive => "a number above 0",
        Bounds::NonNegative => "a number of at least 0",
        Bounds::Negative => "a number below 0",
        Bounds::Open => "a number above 0 and below 1",
        Bounds::HalfOpen => "a number of at least 0 and below 1",
    };
    let specs: Vec<(String, String)> = SPECS
        .iter()
        .map(|spec| {
            let written = match spec.kind {
                settings::Kind::Switch => "`true`, `false`".to_owned(),
                settings::Kind::Choice(names) => names
                    .iter()
                    .map(|name| format!("`\"{name}\"`"))
                    .collect::<Vec<_>>()
                    .join(", "),
                settings::Kind::Number(limits) => bounds(limits).to_owned(),
                settings::Kind::NumberOrAutomatic(limits) => format!("{}, or `\"auto\"`", bounds(limits)),
                settings::Kind::Count(least) => format!("an integer of at least {least}"),
                settings::Kind::PowerOfTwo => "a power of two: 1, 2, 4 and on".to_owned(),
                settings::Kind::Seed => "an integer from 0 to 18446744073709551615".to_owned(),
            };
            (spec.name.to_owned(), written)
        })
        .collect();
    assert_eq!(settings, specs);
}
