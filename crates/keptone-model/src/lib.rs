// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Keptone's patch model, its formats, and the parsers that build it.
//!
//! Every input becomes a typed value at exactly one boundary per format, and
//! every field of a value is a range type, so that a value out of range cannot
//! be represented (docs/DESIGN.md §2, §11). So far, in milestone M2
//! (docs/DESIGN.md §16):
//!
//! - [`voice`]: the DX7's voice, every parameter a range type, and the numbers
//!   the DX7 gives its parameters;
//! - [`syx`]: the DX7's dumps of one voice and of 32, read from `.syx` files
//!   and SysEx messages, however many a file holds, and written back byte for
//!   byte;
//! - [`json`]: the subset of JSON that the native state and the themes are
//!   written in, read by hand, nested as deep as `json.max_depth` lets it be
//!   without taking the thread's stack;
//! - [`sha256`]: the digest the native state carries of its voice.
//!
//! The function data, the parameter changes and requests, the native state and
//! Scala's `.scl` and `.kbm` files come later in the milestone. Every parser is
//! total: any bytes give a value or an error, never a panic and never an
//! unbounded allocation.
#![forbid(unsafe_code)]

pub mod json;
pub mod sha256;
pub mod syx;
pub mod voice;
