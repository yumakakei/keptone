// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! keptone-model's reader of Scala's scale files on what libFuzzer makes up: every input
//! gives a value or an error that names a line, never a panic, and a value
//! written again in the plainest form of the format reads back the same
//! (`properties::read_scl`, which the crate's mutation tests check as well).
#![no_main]
#![forbid(unsafe_code)]

#[path = "../../../crates/keptone-model/tests/properties/mod.rs"]
mod properties;

libfuzzer_sys::fuzz_target!(|bytes: &[u8]| properties::read_scl(bytes));
