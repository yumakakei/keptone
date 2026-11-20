// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Keptone's synthesizer: voices, envelopes, the LFO, the operator graph,
//! feedback, oversampling and the mix (docs/MATH.md §1–§14).
//!
//! The engine accepts only patches compiled from `keptone-model`'s typed values,
//! and compiles them on the main thread. It owns no threads, and allocates only
//! when it is prepared for a sample rate and a set of settings. Its output is
//! bit-identical however the host splits time into blocks, and the same in the
//! audible band at every sample rate (MATH §0.3).
//!
//! It is filled in milestones M3 and M4 (docs/DESIGN.md §16). So far:
//!
//! - [`graph`]: the order in which a voice evaluates its operators, a graph's
//!   strongly connected components, for any graph;
//! - [`pitch`]: the laws of pitch and frequency, from the key after
//!   transposition and 12-TET to an operator's ratio, detune and fixed pitch,
//!   and in [`pitch::phase`] the phase that the frequency advances.
#![forbid(unsafe_code)]

pub mod graph;
pub mod pitch;
