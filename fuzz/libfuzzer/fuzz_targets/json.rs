// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! keptone-model's JSON on what libFuzzer makes up: every input is read with no
//! limit to its nesting, with the default limit and with a limit of one level,
//! giving a value or an error and never a panic, and without taking the
//! thread's stack however deep it nests; and what is read is held to what the
//! parser promises (`properties::read_json`, which the crate's mutation tests
//! check as well).
#![no_main]
#![forbid(unsafe_code)]

#[path = "../../../crates/keptone-model/tests/properties/mod.rs"]
mod properties;

libfuzzer_sys::fuzz_target!(|bytes: &[u8]| properties::read_json(bytes));
