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
//! every field of a value is typed: a number is any finite binary64, on its
//! instrument's scale or beyond it (docs/MATH.md §0.5), and an enumeration, a
//! switch or an index is one of what there is, so that nothing that the engine
//! cannot take can be represented (docs/DESIGN.md §2, §11). So far, in milestone
//! M2
//! (docs/DESIGN.md §16):
//!
//! - [`voice`]: the DX7's voice, every parameter typed, and the numbers
//!   the DX7 gives its parameters;
//! - [`function`]: the DX7's function data, typed as well, and their
//!   numbers;
//! - [`performance`]: the performances of the DX1 and the TX7, two sides of
//!   function data each, and their numbers;
//! - [`supplement`]: the DX7II's supplement of a voice, its additional
//!   parameters, typed as well, and their numbers;
//! - [`dx7ii`]: the DX7II's performances, its system set-up, its micro
//!   tunings and its fractional scalings, and their numbers;
//! - [`tx802`]: the TX802's performances, of eight tone generators, and its
//!   system set-up, and their numbers;
//! - [`patch`]: what a part plays, a voice with the supplement and the
//!   fractional scaling it has;
//! - [`part`]: a part, the notes it takes, its function data and its patch,
//!   and how it starts;
//! - [`settings`]: the settings of docs/MATH.md §17, each with the values it
//!   takes and its default;
//! - [`id`]: the parameters' stable numbers and names, the IDs a host knows
//!   them by, and a part's parameters read and set by them;
//! - [`syx`]: the DX7's system-exclusive messages and its family's, read from
//!   `.syx` files and from hosts, however many a file holds, and written back
//!   byte for byte: the dumps of voices, of the DX7II's supplements and of
//!   performances, the DX7II's universal bulk dumps of its performances, its
//!   system set-up, its micro tunings and its fractional scalings, the DX7s's
//!   of its system set-up, and the TX802's of its performances and its system
//!   set-up, the parameter changes of voices, of function data, of
//!   supplements, of performances, of the system set-up, of micro tunings and
//!   of fractional scalings, the presses of the DX7's panel buttons and of the
//!   DX7II's and the TX802's switches, the DX7s's system reset, the TX7's own
//!   settings and condition, and dump requests;
//! - [`json`]: the subset of JSON that the native state and the themes are
//!   written in, read by hand, nested as deep as `json.max_depth` lets it be
//!   without taking the thread's stack;
//! - [`sha256`]: the digest the native state carries of each patch;
//! - [`scala`]: Scala's scales and keyboard mappings, `.scl` and `.kbm`, the
//!   tuning of docs/MATH.md §2.1;
//! - [`state`]: the native state, every setting, the tuning's Scala files and
//!   every part with its patch, read from its text and written as it, what a
//!   later version adds kept and written back.
//!
//! Every parser is total: any bytes give a value or an error, never a panic
//! and never an unbounded allocation.
#![forbid(unsafe_code)]

pub mod dx7ii;
pub mod function;
pub mod id;
pub mod json;
pub mod part;
pub mod patch;
pub mod performance;
pub mod scala;
pub mod settings;
pub mod sha256;
pub mod state;
pub mod supplement;
pub mod syx;
pub mod tx802;
pub mod voice;
