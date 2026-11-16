// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Keptone as a CLAP plugin: the ABI declared by hand, the entry, the factory
//! and the extensions (docs/DESIGN.md §7.1).
//!
//! The `#[repr(C)]` structures and function tables are written from the CLAP
//! headers, and C23 programs in `tests/layout/` check every offset, size and
//! alignment of them against the headers. The crate builds the shared library
//! that is the CLAP plugin, and a static library that the VST3, AUv2 and
//! standalone wrappers link. Its unsafe code is the foreign-function interface,
//! each block one operation with the reason it is sound.
//!
//! Nothing is here yet: the crate is filled in milestone M5 (docs/DESIGN.md
//! §16).
