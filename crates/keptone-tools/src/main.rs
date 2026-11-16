// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! `keptone-tools`, Keptone's command line: the offline renderer, the reference
//! renderer and the alias metric (docs/MATH.md §16), the inspection and
//! conversion of `.syx` files, benchmarks, and the replay of regressions.
//!
//! No command exists yet: `render` comes with the engine, in milestone M3
//! (docs/DESIGN.md §16).
#![forbid(unsafe_code)]

use std::process::ExitCode;

const USAGE: &str = "\
usage: keptone-tools <command> [options]

No command exists yet.";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [help] if help == "--help" || help == "-h" || help == "help" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
