// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The processor's features, and the level of Keptone's kernels in effect.
//!
//! The kernels of `keptone-kernels` come in forms for levels of the instruction
//! set: the portable form, which is the reference, and forms for AVX2 with FMA,
//! AVX-512 and NEON. The form in effect is the most preferred one that the
//! processor has and the environment allows. `KEPTONE_ISA=<level>` caps the
//! level, and `KEPTONE_KERNELS=portable` forces the portable form, as
//! `engine.kernels = portable` does (docs/MATH.md §0.2, docs/DESIGN.md §10).
//! The choice is made once, so that one build, one choice, one set of settings
//! and one input give the same output bit for bit.
//!
//! Nothing is here yet: the crate is filled with the first vector form, in
//! milestone M7 (docs/DESIGN.md §16).
#![forbid(unsafe_code)]
