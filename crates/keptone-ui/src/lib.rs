// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Keptone's editor (docs/UI.md): its state, layout and widgets, the algorithm
//! graph editor, undo, themes and text.
//!
//! A frame is a pure function of the editor's state, a read-only view of the
//! model and the latest telemetry, and it builds a scene of `keptone-render`:
//! the editor never draws, nor touches a GPU. Text is shaped with HarfBuzz and
//! rasterised by FreeType through the one module allowed unsafe code; the rest
//! of the crate denies it. Every colour and size comes from a theme, and the
//! built-in themes are files in `assets/themes/` read by the same parser as a
//! user's.
//!
//! Nothing is here yet: the crate is filled in milestone M8b (docs/DESIGN.md
//! §16).
#![deny(unsafe_code)]
