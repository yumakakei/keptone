// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! `keptone-tools`, Keptone's command line: `render`, the offline renderer
//! (docs/DESIGN.md §11.5), which renders one note of a patch to a WAV file.
//! The reference renderer and the alias metric (docs/MATH.md §16), the
//! inspection and conversion of `.syx` files, benchmarks and the replay of
//! regressions come with the milestones that give them (docs/DESIGN.md §16).
//!
//! An error in the arguments exits with 2, after the usage; a file that does
//! not read or write, or a render that is not made, with 1.
#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::process::ExitCode;

use keptone_tools::render::{self, Input, Source};

const USAGE: &str = "\
usage: keptone-tools <command> [options]

commands:
  render    one note of a patch rendered to a WAV file

keptone-tools render --help gives its options.";

fn main() -> ExitCode {
    let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
    match arguments.split_first() {
        Some((command, rest)) if command == "render" => run_render(rest),
        Some((help, _)) if help == "--help" || help == "-h" || help == "help" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// `keptone-tools render`: its arguments read, its input read, its note
/// rendered and its WAV file written.
fn run_render(arguments: &[OsString]) -> ExitCode {
    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("{}", render::USAGE);
        return ExitCode::SUCCESS;
    }
    let options = match render::parse(arguments) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("keptone-tools render: {message}\n\n{}", render::USAGE);
            return ExitCode::from(2);
        }
    };
    let path = match &options.input {
        Input::Syx(path) | Input::State(path) => path,
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("keptone-tools render: {}: {error}", path.to_string_lossy());
            return ExitCode::FAILURE;
        }
    };
    let source = match options.input {
        Input::Syx(_) => Source::Syx(&bytes),
        Input::State(_) => Source::State(&bytes),
    };
    let rendered = match render::render(source, &options) {
        Ok(rendered) => rendered,
        Err(error) => {
            eprintln!("keptone-tools render: {error}");
            return ExitCode::FAILURE;
        }
    };
    for finding in &rendered.findings {
        eprintln!("keptone-tools render: {}: {finding}", path.to_string_lossy());
    }
    if let Err(error) = std::fs::write(&options.output, &rendered.wav) {
        eprintln!("keptone-tools render: {}: {error}", options.output.to_string_lossy());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
