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

use keptone_model::syx::{self, Content, Loss, Options, OutOfRange};
use keptone_model::voice::Rounding;

/// A file read with every choice of the options. The messages lie in the file
/// one after another, and every dump read, written again, is read back with
/// nothing to report and the same voices.
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
                let Ok(dump) = message.result else {
                    continue;
                };
                let (written, loss) = match &dump.content {
                    Content::Voice(voice) => syx::write_voice(voice, dump.channel, Rounding::Nearest),
                    Content::Bank(voices) => syx::write_bank(voices, dump.channel, Rounding::Nearest),
                };
                assert_eq!(loss, Loss::default());
                let again = syx::decode(&written, strict).expect("a dump written is valid");
                assert_eq!(again.findings, []);
                assert_eq!(again.channel, dump.channel);
                assert_eq!(again.content, dump.content);
            }
        }
    }
}
