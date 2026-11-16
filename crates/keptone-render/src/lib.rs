// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The scene that the editor builds, and the renderers that draw it
//! (docs/DESIGN.md §8.2–§8.4).
//!
//! A scene is a display list of filled and stroked paths, glyph runs, images,
//! clips, transforms and opacity layers, in logical pixels. The CPU renderer
//! rasterises it by analytic coverage on every system; it is the reference that
//! the GPU backends are tested against, and the fallback when they fail:
//! Direct3D 11 on Windows, Metal on macOS and Vulkan on Linux. Unsafe code is
//! allowed in the backends' files alone; the rest of the crate denies it.
//!
//! Nothing is here yet: the crate is filled in milestone M8a (docs/DESIGN.md
//! §16).
#![deny(unsafe_code)]
