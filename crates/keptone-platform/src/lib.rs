// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The editor's window on each windowing system: Win32, AppKit, X11 and
//! Wayland, one file each (docs/DESIGN.md §8.1).
//!
//! A child of the host's window, or a floating window on Wayland, which has no
//! embedding; input, the display's scale, the surfaces a renderer can draw on,
//! and timers. The systems' functions are declared by hand, and the unsafe code
//! of their calls stays in this crate.
//!
//! Nothing is here yet: the crate is filled in milestone M8a (docs/DESIGN.md
//! §16).
