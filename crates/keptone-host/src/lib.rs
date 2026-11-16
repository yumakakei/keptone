// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Keptone's processor, independent of the plugin format.
//!
//! The parameter table and its smoothing, the decoding of events (CLAP's, MIDI
//! 1.0, MIDI 2.0's Universal MIDI Packets and SysEx), the division of the work
//! between the main, worker and audio threads, and the saving and loading of the
//! state (docs/DESIGN.md §5, §7). Nothing that the audio thread can reach
//! allocates, frees, locks, makes a system call or panics (DESIGN §9).
//!
//! Nothing is here yet: the crate is filled in milestone M5 (docs/DESIGN.md
//! §16).
#![forbid(unsafe_code)]
