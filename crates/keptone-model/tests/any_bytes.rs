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
//! as kiral's tests of its parsers have it. What a parser makes of them is
//! held to what it promises, by the properties that the fuzz targets of
//! fuzz/libfuzzer check as well (`properties`). The inputs that once made a
//! parser fail are kept in fuzz/regressions/<parser>/, and read on every run.

mod common;
mod properties;

use std::path::Path;

use common::{SplitMix64, random_bank, random_voice};
use keptone_model::syx::{self, Options};
use properties::read_syx;

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
    let tokens: [&[u8]; 8] = [
        &[0xf0],
        &[0xf7],
        &[0xf0, 0x43, 0x00, 0x00, 0x01, 0x1b],
        &[0xf0, 0x43, 0x00, 0x09, 0x20, 0x00],
        &[0x7f],
        &[0x80],
        &[0x63],
        &[0x64],
    ];
    for seed in [&voice[..], &bank, &file] {
        assert!(
            syx::read(seed, Options::default()).any(|message| message.result.is_ok()),
            "a seed with no dump"
        );
        for _ in 0..MUTATIONS {
            read_syx(&mutate(&mut random, seed, &tokens));
        }
    }
    for input in regressions("syx") {
        read_syx(&input);
    }
}
