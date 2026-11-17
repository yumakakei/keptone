// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The parsers take any bytes: inputs made by changing valid ones at random --
//! bits flipped, bytes put in, taken out, repeated, the end cut off, bytes that
//! mean something to the format put in -- are read or refused, never a panic,
//! as kiral's tests of its parsers have it. What a parser makes of them, and of
//! the valid inputs themselves, is held to what it promises, by the properties
//! that the fuzz targets of fuzz/libfuzzer check as well (`properties`). The
//! inputs that once made a parser fail are kept in fuzz/regressions/<parser>/,
//! and read on every run.

mod common;
mod properties;

use std::path::Path;

use common::{
    SplitMix64, dump, random_bank, random_performance_steps, random_performances, random_supplement_steps,
    random_supplements, random_voice,
};
use keptone_model::json;
use keptone_model::scala;
use keptone_model::syx::{self, Options};
use properties::{read_json, read_kbm, read_scl, read_syx};

/// How many inputs are made of each seed: fewer in the dev profile, whose
/// code is some ten times slower.
const MUTATIONS: usize = if cfg!(debug_assertions) { 2000 } else { 20_000 };

/// The input changed in one to four ways at random.
fn mutate(random: &mut SplitMix64, input: &[u8], tokens: &[&[u8]]) -> Vec<u8> {
    let mut bytes = input.to_vec();
    for _ in 0..=random.below(4) {
        let at = random.below(bytes.len() + 1);
        match random.below(6) {
            0 if at < bytes.len() => bytes[at] ^= 1 << random.below(8),
            1 => bytes.insert(at, random.byte()),
            2 if at < bytes.len() => {
                let end = (at + 1 + random.below(8)).min(bytes.len());
                bytes.drain(at..end);
            }
            3 if at < bytes.len() => {
                let end = (at + 1 + random.below(16)).min(bytes.len());
                let repeated = bytes[at..end].to_vec();
                bytes.splice(at..at, repeated);
            }
            4 => bytes.truncate(at),
            _ => {
                let token = tokens[random.below(tokens.len())];
                bytes.splice(at..at, token.iter().copied());
            }
        }
    }
    bytes
}

/// The inputs kept in fuzz/regressions/<parser>/, none where there are none.
fn regressions(parser: &str) -> Vec<Vec<u8>> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fuzz/regressions")
        .join(parser);
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };
    entries
        .map(|entry| std::fs::read(entry.expect("a directory entry").path()).expect("a regression input"))
        .collect()
}

#[test]
fn syx_takes_any_bytes() {
    let mut random = SplitMix64(11);
    let voice = random_voice(&mut random, 5);
    let (_, bank) = random_bank(&mut random, 0);
    let file = [
        &b"before"[..],
        &voice,
        &[0x90, 0x40],
        &bank,
        &voice[..80],
        &voice,
        b"after",
    ]
    .concat();
    // Parameter changes of a voice parameter, the operators' switches, a
    // function parameter, a button of the DX7's panel, a switch of the DX7II's,
    // and the TX7's of a performance's parameter, of its own settings and of
    // its operator select; and dump requests, a universal one among them.
    let short = [
        &[0xf0, 0x43, 0x10, 0x01, 0x10, 0x05, 0xf7][..],
        &[0xf0, 0x43, 0x11, 0x01, 0x1b, 0x3f, 0xf7],
        &[0xf0, 0x43, 0x12, 0x08, 0x41, 0x0b, 0xf7],
        &[0xf0, 0x43, 0x13, 0x08, 0x27, 0x7f, 0xf7],
        &[0xf0, 0x43, 0x14, 0x1b, 0x2d, 0x00, 0xf7],
        &[0xf0, 0x43, 0x15, 0x04, 0x01, 0x0b, 0xf7],
        &[0xf0, 0x43, 0x15, 0x04, 0x0d, 0x09, 0xf7],
        &[0xf0, 0x43, 0x16, 0x11, 0x05, 0x15, 0xf7],
        &[0xf0, 0x43, 0x16, 0x11, 0x07, 0x7f, 0xf7],
        &[0xf0, 0x43, 0x17, 0x01, 0x1c, 0x03, 0xf7],
        &[0xf0, 0x43, 0x18, 0x18, 0x10, 0x0c, 0xf7],
        &[0xf0, 0x43, 0x18, 0x18, 0x49, 0x01, 0xf7],
        &[0xf0, 0x43, 0x25, 0x00, 0xf7],
        &[0xf0, 0x43, 0x26, 0x09, 0xf7],
        &[0xf0, 0x43, 0x28, 0x01, 0xf7],
        &[0xf0, 0x43, 0x28, 0x02, 0xf7],
        &[0xf0, 0x43, 0x28, 0x7d, 0xf7],
        b"\xf0\x43\x27\x7eLM  8973PE\xf7",
    ]
    .concat();
    // The dumps of one performance, under format 1 and under 2, and of 64,
    // a condition acknowledge, and the DX7II's dumps of one supplement and of
    // 32.
    let steps = random_performance_steps(&mut random);
    let (_, performances) = random_performances(&mut random, 3);
    let performance = [
        dump(1, 1, &steps),
        dump(2, 2, &steps),
        dump(0, 0x7d, b"LM  8950  \x01\x02\x58\x05\x1f\x00"),
        dump(5, 5, &random_supplement_steps(&mut random)),
    ]
    .concat();
    let (_, supplements) = random_supplements(&mut random, 6);
    let tokens: [&[u8]; 20] = [
        &[0xf0],
        &[0xf7],
        &[0xf0, 0x43, 0x00, 0x00, 0x01, 0x1b],
        &[0xf0, 0x43, 0x00, 0x09, 0x20, 0x00],
        &[0xf0, 0x43, 0x00, 0x01, 0x00, 0x5e],
        &[0xf0, 0x43, 0x00, 0x02, 0x20, 0x00],
        &[0xf0, 0x43, 0x00, 0x7d, 0x00, 0x10],
        &[0xf0, 0x43, 0x00, 0x05, 0x00, 0x31],
        &[0xf0, 0x43, 0x00, 0x06, 0x08, 0x60],
        &[0xf0, 0x43, 0x10, 0x18],
        &[0xf0, 0x43, 0x10, 0x04],
        &[0xf0, 0x43, 0x10, 0x08],
        &[0xf0, 0x43, 0x10, 0x11],
        &[0xf0, 0x43, 0x10, 0x1b],
        &[0xf0, 0x43, 0x20],
        &[0x7f],
        &[0x80],
        &[0x63],
        &[0x64],
        b"LM  ",
    ];
    for seed in [
        &voice[..],
        &bank,
        &file,
        &short,
        &performance,
        &performances,
        &supplements,
    ] {
        assert!(
            syx::read(seed, Options::default()).any(|message| message.result.is_ok()),
            "a seed with no message read"
        );
        read_syx(seed);
        for _ in 0..MUTATIONS {
            read_syx(&mutate(&mut random, seed, &tokens));
        }
    }
    for input in regressions("syx") {
        read_syx(&input);
    }
}

#[test]
fn json_takes_any_bytes() {
    let seeds: [&[u8]; 6] = [
        br#"{"format": 1, "parameters": {"op1.eg.rate1": 99, "op1.output_level": 71.25}, "lfo.seed": 18446744073709551615}"#,
        "{\"\\u00e3\\u0081\\u00ae\": 240, \"name\": \"E.PIANO 1\", \"ア\": [1, 2]}".as_bytes(),
        br#"[[[[]]], {"a": [true, false, null]}, -0.5e-3, "\ud83d\ude00", 1e400]"#,
        br#"{"b": 1, "a": {"c": [{"d": {}}]}}"#,
        b"  12345678901234567890123  ",
        b"\"\\\"\\/\\b\\f\\n\\r\\t\"",
    ];
    let tokens: [&[u8]; 12] = [
        b"{", b"}", b"[", b"]", b"\"", b"\\u", b",", b":", b"-0e", b"null", b"\\ud800", b"\xc3",
    ];
    let mut random = SplitMix64(12);
    for seed in seeds {
        assert!(
            json::parse(seed, json::DEFAULT_MAX_DEPTH).is_ok(),
            "a seed that is not JSON"
        );
        read_json(seed);
        for _ in 0..MUTATIONS / 5 {
            read_json(&mutate(&mut random, seed, &tokens));
        }
    }
    for input in regressions("json") {
        read_json(&input);
    }
}

#[test]
fn scl_takes_any_bytes() {
    let seeds: [&[u8]; 4] = [
        b"! equal.scl\n!\nTwelve equal steps of the octave\n 12\n!\n100.0\n200.\n300.0\n400.0\n500.0\n600.0\n\
          700.0\n800.0\n900.0\n1000.0\n1100.0\n2/1\n",
        b"Five-limit just intonation\r\n12\r\n16/15\r\n9/8\r\n6/5\r\n5/4 E\r\n4/3\r\n45/32\r\n3/2\r\n\
          8/5\r\n5/3\r\n9/5\r\n15/8\r\n2/1\r\n",
        b"Caf\xe9\r3\r-5.0\r10/20\r 3 cents\r",
        b"\n0\n",
    ];
    let tokens: [&[u8]; 10] = [
        b"!",
        b"\n",
        b"\r\n",
        b"\r",
        b".",
        b"/",
        b"-",
        b" ",
        b"18446744073709551615",
        b"\xe9",
    ];
    let mut random = SplitMix64(13);
    for seed in seeds {
        assert!(scala::parse_scl(seed).is_ok(), "a seed that is not a scale");
        read_scl(seed);
        for _ in 0..MUTATIONS / 4 {
            read_scl(&mutate(&mut random, seed, &tokens));
        }
    }
    for input in regressions("scl") {
        read_scl(&input);
    }
}

#[test]
fn kbm_takes_any_bytes() {
    let seeds: [&[u8]; 3] = [
        b"! a mapping of twelve keys\n12\n0\n127\n60\n69\n440.0\n12\n! the entries\n0\n1\n2\n3\n4\n5\n\
          6\n7\n8\n9\n10\n11\n",
        b"0\r\n0\r\n127\r\n60\r\n60\r\n261.6255653\r\n0\r\n",
        b"7\n21\n108\n60\n62\n293.66\n-7\n0\nx\n2\n-1\n",
    ];
    let tokens: [&[u8]; 9] = [b"!", b"\n", b"\r", b"x", b"-", b".", b"127", b"128", b"0"];
    let mut random = SplitMix64(14);
    for seed in seeds {
        assert!(scala::parse_kbm(seed).is_ok(), "a seed that is not a mapping");
        read_kbm(seed);
        for _ in 0..MUTATIONS / 4 {
            read_kbm(&mutate(&mut random, seed, &tokens));
        }
    }
    for input in regressions("kbm") {
        read_kbm(&input);
    }
}
