// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! What a parser makes of any bytes, held to what it promises: a property of
//! an input of any bytes for each parser, which panics where the parser breaks
//! its promise. The mutation tests of keptone-model (`any_bytes.rs`) check them
//! on inputs changed at random, and the fuzz targets of fuzz/libfuzzer, which
//! take this file by its path, on what libFuzzer makes up: the two check the
//! same.

#![allow(dead_code, reason = "each fuzz target takes the property of its own parser")]

use std::fmt::Write as _;

use keptone_model::json::{self, DEFAULT_MAX_DEPTH, Value};
use keptone_model::scala::{self, Mapping, Pitch, Scale};
use keptone_model::syx::{self, Content, Dump, Loss, Options, OutOfRange, ParameterChange, PerformanceLoss, Sysex};
use keptone_model::voice::Rounding;

/// A file read with every choice of the options. The messages lie in the file
/// one after another, and every message read, written again, is read back with
/// nothing to report and the same in every way: a dump with the same channel
/// and voices, a parameter change with the same channel and change, a request
/// with the same channel and request. A message read with nothing to report is
/// written back byte for byte as it was (docs/MATH.md §16).
pub fn read_syx(input: &[u8]) {
    let strict = Options {
        accept_bad_checksum: false,
        out_of_range: OutOfRange::Reject,
    };
    for accept_bad_checksum in [true, false] {
        for out_of_range in [OutOfRange::Clamp, OutOfRange::Reject] {
            let options = Options {
                accept_bad_checksum,
                out_of_range,
            };
            let mut end = 0;
            for message in syx::read(input, options) {
                assert!(message.offset >= end && message.length > 0);
                end = message.offset + message.length;
                assert!(end <= input.len());
                let Ok(sysex) = message.result else {
                    continue;
                };
                let (_, from) = input.split_at(message.offset);
                let (read, _) = from.split_at(message.length);
                match sysex {
                    Sysex::Dump(dump) => {
                        let written = written_again(&dump);
                        if dump.findings.is_empty() {
                            assert_eq!(written, read);
                        }
                        let Ok(Sysex::Dump(again)) = syx::decode(&written, strict) else {
                            panic!("a dump written is not read back as one");
                        };
                        assert_eq!(again.findings, []);
                        assert_eq!(again.channel, dump.channel);
                        assert_eq!(again.content, dump.content);
                    }
                    Sysex::Change(change) => {
                        let written = syx::write_change(change.channel, change.change);
                        if change.finding.is_none() {
                            assert_eq!(written, read);
                        }
                        let again = syx::decode(&written, strict);
                        let expected = ParameterChange {
                            finding: None,
                            ..change
                        };
                        assert_eq!(again, Ok(Sysex::Change(expected)));
                    }
                    Sysex::Request(request) => {
                        let written = syx::write_request(request.channel, request.requested);
                        assert_eq!(written, read);
                        assert_eq!(syx::decode(&written, strict), Ok(Sysex::Request(request)));
                    }
                }
            }
        }
    }
}

/// A dump written again from what was read of it, which it holds exactly.
fn written_again(dump: &Dump) -> Vec<u8> {
    let (channel, rounding) = (dump.channel, Rounding::Nearest);
    match &dump.content {
        Content::Voice(voice) => {
            let (written, loss) = syx::write_voice(voice, channel, rounding);
            assert_eq!(loss, Loss::default());
            written
        }
        Content::Bank(voices) => {
            let (written, loss) = syx::write_bank(voices, channel, rounding);
            assert_eq!(loss, Loss::default());
            written
        }
        Content::Performance(performance) => {
            let (written, loss) = syx::write_performance(performance, channel, rounding);
            assert_eq!(loss, PerformanceLoss::default());
            written
        }
        Content::Performances(performances) => {
            let (written, loss) = syx::write_performances(performances, channel, rounding);
            assert_eq!(loss, PerformanceLoss::default());
            written
        }
        Content::Condition(condition) => syx::write_condition(condition, channel),
        Content::Supplement(supplement) => {
            let (written, rounded) = syx::write_supplement(supplement, channel, rounding);
            assert_eq!(rounded, []);
            written
        }
        Content::Supplements(supplements) => {
            let (written, rounded) = syx::write_supplements(supplements, channel, rounding);
            assert_eq!(rounded, []);
            written
        }
    }
}

/// A text read with no limit to its nesting, with the default limit and with
/// a limit of one level. Whatever reads within a limit reads the same with no
/// limit, and what does not read with no limit reads with none; a value
/// nests no deeper than its limit and gives no name twice in an object; and
/// an error lies within the text.
pub fn read_json(input: &[u8]) {
    let unlimited = json::parse(input, usize::MAX);
    let limited = json::parse(input, DEFAULT_MAX_DEPTH);
    let flat = json::parse(input, 1);
    if let Ok(value) = &limited {
        assert!(depth(value) <= DEFAULT_MAX_DEPTH);
        assert!(names_once(value));
        assert_eq!(unlimited.as_ref().ok(), Some(value));
    }
    if let Ok(value) = &flat {
        assert!(depth(value) <= 1);
        assert_eq!(limited.as_ref().ok(), Some(value));
    }
    if let Ok(value) = &unlimited {
        assert!(names_once(value));
    } else {
        assert!(limited.is_err() && flat.is_err());
    }
    for error in [&unlimited, &limited, &flat]
        .into_iter()
        .filter_map(|result| result.as_ref().err())
    {
        assert!(error.offset <= input.len() && error.line >= 1 && error.column >= 1);
    }
}

/// How deep a value's arrays and objects nest, counted without recursion.
fn depth(value: &Value) -> usize {
    let mut deepest = 0;
    let mut stack = vec![(value, 0)];
    while let Some((value, level)) = stack.pop() {
        let within: Vec<&Value> = match value {
            Value::Array(values) => values.iter().collect(),
            Value::Object(object) => object.iter().map(|(_, value)| value).collect(),
            _ => continue,
        };
        deepest = deepest.max(level + 1);
        stack.extend(within.into_iter().map(|value| (value, level + 1)));
    }
    deepest
}

/// Whether no object within a value gives a name twice, checked without
/// recursion.
fn names_once(value: &Value) -> bool {
    let mut stack = vec![value];
    while let Some(value) = stack.pop() {
        match value {
            Value::Array(values) => stack.extend(values),
            Value::Object(object) => {
                let mut names: Vec<&str> = object.iter().map(|(name, _)| name).collect();
                names.sort_unstable();
                if names.windows(2).any(|pair| pair[0] == pair[1]) {
                    return false;
                }
                stack.extend(object.iter().map(|(_, value)| value));
            }
            _ => {}
        }
    }
    true
}

/// A scale file read, and, where it reads, the scale written again in the
/// plainest form the format has and read back the same; an error names a line
/// of the file, or the one after its last.
pub fn read_scl(input: &[u8]) {
    match scala::parse_scl(input) {
        Ok(scale) => {
            let again = scala::parse_scl(&write_scl(&scale)).expect("a scale written reads");
            assert_eq!(again, scale);
        }
        Err(error) => assert!(error.line >= 1 && error.line <= lines(input) + 1),
    }
}

/// A mapping file read, and, where it reads, the mapping written again in
/// the plainest form the format has and read back the same; an error names a
/// line of the file, or the one after its last.
pub fn read_kbm(input: &[u8]) {
    match scala::parse_kbm(input) {
        Ok(mapping) => {
            assert!(mapping.degrees().len() <= mapping.size());
            let again = scala::parse_kbm(&write_kbm(&mapping)).expect("a mapping written reads");
            assert_eq!(again, mapping);
        }
        Err(error) => assert!(error.line >= 1 && error.line <= lines(input) + 1),
    }
}

/// The number of lines in a file, as the Scala parsers count them.
fn lines(input: &[u8]) -> usize {
    let mut count = 0;
    let mut rest = input;
    while !rest.is_empty() {
        count += 1;
        let Some(end) = rest.iter().position(|&byte| byte == b'\n' || byte == b'\r') else {
            break;
        };
        let ending = if rest[end..].starts_with(b"\r\n") { 2 } else { 1 };
        rest = &rest[end + ending..];
    }
    count
}

/// A scale as a `.scl` file, its cents in fixed notation with a period, as
/// the format has them.
fn write_scl(scale: &Scale) -> Vec<u8> {
    let mut text = format!("{}\n{}\n", scale.description, scale.pitches.len());
    for pitch in &scale.pitches {
        match pitch {
            Pitch::Cents(cents) => {
                let mut value = cents.get().to_string();
                if !value.contains('.') {
                    value.push('.');
                }
                text += &value;
            }
            Pitch::Ratio(ratio) => {
                write!(text, "{}/{}", ratio.numerator(), ratio.denominator()).expect("a string takes any text");
            }
        }
        text.push('\n');
    }
    text.into_bytes()
}

/// A mapping as a `.kbm` file.
fn write_kbm(mapping: &Mapping) -> Vec<u8> {
    let mut text = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n",
        mapping.size(),
        mapping.first().get(),
        mapping.last().get(),
        mapping.middle().get(),
        mapping.reference().get(),
        mapping.frequency().get(),
        mapping.octave()
    );
    for degree in mapping.degrees() {
        match degree {
            Some(degree) => writeln!(text, "{degree}").expect("a string takes any text"),
            None => text += "x\n",
        }
    }
    text.into_bytes()
}
