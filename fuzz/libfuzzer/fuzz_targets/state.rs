// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! keptone-model's native state on what libFuzzer makes up: every input is read
//! as a state, giving a state or an error and never a panic; and what is read
//! is held to what the reader promises (`properties::read_state`, which the
//! crate's mutation tests check as well): written back and read again, it is
//! the same state with nothing found, and an error lies within the text and
//! names its place in the state.
#![no_main]
#![forbid(unsafe_code)]

#[path = "../../../crates/keptone-model/tests/properties/mod.rs"]
mod properties;

libfuzzer_sys::fuzz_target!(|bytes: &[u8]| properties::read_state(bytes));
