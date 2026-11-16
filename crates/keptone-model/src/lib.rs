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
//! be represented (docs/DESIGN.md §2, §11): the voices and function data of the
//! DX7 and their SysEx messages (single-voice and 32-voice dumps, parameter
//! changes, requests), the native state, and Scala's `.scl` and `.kbm` files.
//! Every parser is total: any bytes give a value or an error, never a panic and
//! never an unbounded allocation.
//!
//! Nothing is here yet: the crate is filled in milestone M2, with the
//! specification of the native state (docs/DESIGN.md §16).
#![forbid(unsafe_code)]
