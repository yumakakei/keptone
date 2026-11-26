// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! `keptone-tools`, Keptone's command line, as a library that its binary and
//! its tests share: the offline renderer, `render` (docs/DESIGN.md §11.5), and
//! the WAV files that it writes. The reference renderer and the alias metric
//! (docs/MATH.md §16), the inspection and conversion of `.syx` files,
//! benchmarks and the replay of regressions come with the milestones that
//! give them (docs/DESIGN.md §16).
#![forbid(unsafe_code)]

pub mod render;
pub mod wav;
