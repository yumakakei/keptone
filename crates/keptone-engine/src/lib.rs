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
//! - [`control`]: the controllers' depths, the depths of the LFO's
//!   modulations, the EG bias, the amplitude that an operator keeps and the
//!   level by which that lowers it, the LFO's pitch, and the bend;
//! - [`eg`]: the operator envelope, the laws of its rates and segments, the
//!   rate and its speed, the attack and the jump that starts it, and the wait,
//!   and in [`eg::envelope`] the envelope that runs its segments by them, timed
//!   as the DX7's, through its key events, holds and live changes;
//! - [`feedback`]: an operator's feedback, the scale of FB and the loop gain,
//!   the delay-free point solution and the models for a loop gain of 1 and
//!   more: relaxation, the DX7's recurrence and clamp;
//! - [`graph`]: the order in which a voice evaluates its operators, a graph's
//!   strongly connected components, for any graph;
//! - [`level`]: the level curve, an operator's offset from its output level,
//!   its keyboard level scaling and the key's velocity, the envelope's part of
//!   an operator's total level, the total level and its gain;
//! - [`lfo`]: the LFO, its frequency, phase, waves, sample-and-hold values and
//!   the fade of its delay;
//! - [`peg`]: the pitch envelope, its levels and speeds from the DX7's tables
//!   and the envelope that moves a voice's pitch through them;
//! - [`portamento`]: the portamento's speed, and a voice's glide toward its
//!   target, with glissando or without;
//! - [`pitch`]: the laws of pitch and frequency, from the key after
//!   transposition and 12-TET to an operator's ratio, detune and fixed pitch,
//!   and in [`pitch::phase`] the phase that the frequency advances;
//! - [`random`]: the random source, `SplitMix64` read at counters, and a part's
//!   streams;
//! - [`time`]: the instants at which segments start and from which they count
//!   their time.
#![forbid(unsafe_code)]

mod binary64;
pub mod control;
pub mod eg;
pub mod feedback;
pub mod graph;
pub mod level;
pub mod lfo;
pub mod peg;
pub mod pitch;
pub mod portamento;
pub mod random;
mod steps;
pub mod time;
