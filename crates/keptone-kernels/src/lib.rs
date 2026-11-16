// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Vector forms of the inner loops of `keptone-math` and `keptone-engine`.
//!
//! A form in assembly is added only with a measurement showing the portable
//! form slower, recorded beside it with the processor, its clock and the rates,
//! and it is tested against its accuracy contract at every level of the
//! processor, under Intel's Software Development Emulator where no machine at
//! hand has the level (docs/DESIGN.md §10). Kernels are naked functions, one file
//! per architecture and level (`x86_64/avx2.rs`, `aarch64/neon.rs`), declared
//! `extern "sysv64"` on x86-64 and `extern "C"` on AArch64; no intrinsics are
//! used. Each call into assembly is one unsafe block in the module of its
//! architecture, whose comment says what was checked; the rest of the crate
//! denies unsafe code.
//!
//! Nothing is here yet: the first vector forms come in milestone M7
//! (docs/DESIGN.md §16).
#![deny(unsafe_code)]
