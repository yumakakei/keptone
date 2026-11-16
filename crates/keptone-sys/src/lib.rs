// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The operating systems' functions Keptone calls, declared by hand: time,
//! hints for threads and, for MTS-ESP, loading a shared library at run time
//! (docs/DESIGN.md §4). No crate brings them: there is no `libc`, `windows-sys`
//! or `objc2`. Each call is one unsafe block whose comment says what makes it
//! sound, and the functions here are safe to call.
//!
//! Nothing is here yet: the crate gains each function with the first milestone
//! that needs it.
