// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The `.syx` codecs of keptone-model on what libFuzzer makes up: every input
//! is read as a file with every choice of the options, giving a value or an
//! error and never a panic, and every dump read comes back from being written
//! as a valid dump of the same voices (`properties::read_syx`, which the
//! crate's mutation tests check as well).
#![no_main]
#![forbid(unsafe_code)]

#[path = "../../../crates/keptone-model/tests/properties/mod.rs"]
mod properties;

libfuzzer_sys::fuzz_target!(|bytes: &[u8]| properties::read_syx(bytes));
