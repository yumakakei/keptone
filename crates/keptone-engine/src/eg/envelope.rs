// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The envelope of an operator (docs/MATH.md §4): its segments toward L1 to L4,
//! timed as the DX7's are, its key events and holds, and the re-anchoring of
//! live changes, its level at each instant in closed form from the start of
//! the piece of law that it is in.
//!
//! The envelope keeps two levels (MATH §4.6). The DX7's, max(T, 16), times
//! its segments: a segment starts, waits and ends where the DX7's would, with
//! its level and its target taken no lower than its floor, 16. Its own, T,
//! which it gives, goes on below the floor toward the target of the segment it is
//! in, by that segment's law, and stays where it reaches it; a segment that
//! rises as the DX7 takes it starts it from max(T, 16). So max(T, 16) is the
//! DX7's level at every instant.
//!
//! Its levels are total levels T at the offset O that its segment started
//! with, which the laws that depend on T take (MATH §4.7); the level that it
//! gives at an instant adds the offset's change since, so that a change of O
//! moves T and every target and leaves the segments where they were. A piece
//! of law counts its progress ∫ρ dt from an instant at its speed, which holds
//! still within it: a change of a rate, a level, the rate scaling or the key
//! of the rates starts a new piece there (§4.7).
//!
//! Its time is the engine's, in instants (§0.1). At each instant, the segments
//! that end at or before it end first ([`Envelope::end`], [`Envelope::next`]),
//! as a sample at or after a segment's end takes the next segment; then the
//! events there act; and then the level is read (MATH §0.3).

use crate::binary64::finite;
use crate::level::{envelope as envelope_level, total};
use crate::time::Instant;

use super::{FLOOR, RateLaw, attack, attack_progress, jump, rate, speed, wait};

/// The settings that an envelope takes (MATH §17), and the engine's rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// The engine's sample rate fₑ, at which instants count samples.
    pub sample_rate: f64,
    /// `eg.clock_hz`.
    pub clock_hz: f64,
    /// `eg.rate_law`.
    pub rate_law: RateLaw,
    /// `eg.attack_jump`.
    pub attack_jump: bool,
    /// `eg.static_segments`: `dx7`, true, waits; `none`, false, does not.
    pub waits: bool,
}

/// An operator's parameters that its envelope takes (MATH §4): its rates R1 to
/// R4 and its levels L1 to L4, 0 to 99 on their scales, and its rate scaling
/// KRS, 0 to 7.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parameters {
    /// R1 to R4.
    pub rates: [f64; 4],
    /// L1 to L4.
    pub levels: [f64; 4],
    /// KRS.
    pub rate_scaling: f64,
}

/// Where an envelope is (MATH §4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// In a segment, 0 to 3, toward L1 to L4.
    Segment(u8),
    /// Past segment 2, holding at L3 while the key or the pedal holds the note.
    Held,
    /// Past segment 3, at L4.
    Resting,
}

/// The law of a piece: the level as a function of its progress.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Law {
    /// A level that holds still, or silence, none being −∞.
    Still(Option<f64>),
    /// A fall at the speed from a level to a target, or on toward −∞.
    Fall { from: f64, to: Option<f64> },
    /// An attack from a level to a target (§4.3).
    Rise { from: f64, to: f64 },
}

impl Law {
    /// The level after a progress of 0 or more: a fall's T₀ − Φ, and an attack,
    /// each stopping at its target.
    fn level(self, progress: f64) -> Option<f64> {
        match self {
            Self::Still(level) => level,
            Self::Fall { from, to } => {
                let fallen = finite(from - progress);
                Some(to.map_or(fallen, |to| fallen.max(to)))
            }
            Self::Rise { from, to } => Some(attack(from, progress).min(to)),
        }
    }
}

/// A piece of law: a law from an instant on, at a speed.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Piece {
    /// The instant from which its progress counts.
    start: Instant,
    /// Its progress a sample, ρ/fₑ.
    step: f64,
    /// Its law.
    law: Law,
    /// The progress from its start at which its segment ends, none where the
    /// envelope holds.
    end: Option<f64>,
}

/// The level of a target L, at an offset: T(L) = E(L) + O − 4256, none for an
/// L at or below 0, whose target is −∞ (MATH §3.3, §4.6).
fn target(level: f64, offset: f64) -> Option<f64> {
    envelope_level(level).map(|envelope| total(envelope, offset))
}

/// A level as the DX7 takes it, no lower than its floor, −∞ at the floor.
fn shown(level: Option<f64>) -> f64 {
    level.map_or(FLOOR, |level| level.max(FLOOR))
}

/// The law of a segment that the DX7 does not see rise, its own level going
/// toward its own target as its own direction says (MATH §4.6): a fall, a rise
/// that jumps as at a segment's start, there being one below the floor, or a
/// level that holds. From −∞, a rise takes its target at once where it jumps
/// and never leaves −∞ where it does not.
fn own_law(from: Option<f64>, to: Option<f64>, jumps: bool) -> Law {
    match (from, to) {
        (None, Some(to)) if jumps => Law::Still(Some(to)),
        (None, _) => Law::Still(None),
        (Some(level), None) => Law::Fall { from: level, to: None },
        (Some(level), Some(to)) if to < level => Law::Fall {
            from: level,
            to: Some(to),
        },
        (Some(level), Some(to)) if to > level => Law::Rise {
            from: if jumps { jump(level, to) } else { level },
            to,
        },
        (Some(level), Some(_)) => Law::Still(Some(level)),
    }
}

/// The envelope of an operator (MATH §4).
///
/// Its level at an instant follows its piece's law at the progress Φ = step·Δ,
/// Δ the samples since the piece's start and the step ρ/fₑ. The speed is
/// within 2.5u of its value at the rate that [`rate`] gives, 2.86u under the
/// exponential law ([`speed`]), and within a quarter of the rate's error more,
/// the speed's slope in the rate being at most a quarter of the speed; the
/// quotient rounds within u, Δ within u(|Δ| + 1/2) and the product within u:
/// the progress lies within Φ(ε + 2u) + u·step/2 of ∫ρ dt at the parameters'
/// binary64 values, ε the step's relative error, and a step more for each
/// sample of its start's error. A law passes the progress's error on by its
/// slope, 1 for a fall and max(1, 2 + (3840 − T₀)/256) for an attack from T₀,
/// beside its own ([`attack`]); a piece starts from a target, which carries
/// the errors of E(L) and of the total level ([`envelope_level`], [`total`]),
/// or from where the piece before it was, whose error its law passes on; and
/// an end lies within its start's error, and its progress's and the step's
/// over the step, of the exact instant. `scripts/envelope_references.py`
/// follows these through every piece of the scenarios that tests/envelope.rs
/// runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Envelope {
    /// Where it is.
    stage: Stage,
    /// Its piece of law.
    piece: Piece,
    /// The offset O at its segment's start (MATH §4.7).
    offset: f64,
    /// The progress that its segment made before its piece started, which its
    /// wait counts (MATH §4.7).
    counted: f64,
}

impl Envelope {
    /// The envelope of a voice that has had no note, or whose note has ended
    /// (MATH §4.5): resting at L4's level at an offset, from an instant on, as
    /// a key-on takes it.
    #[must_use]
    pub fn new(parameters: &Parameters, offset: f64, at: Instant) -> Self {
        let law = Law::Still(target(parameters.levels[3], offset));
        Self {
            stage: Stage::Resting,
            piece: Piece {
                start: at,
                step: 0.0,
                law,
                end: None,
            },
            offset,
            counted: 0.0,
        }
    }

    /// Where it is.
    #[must_use]
    pub const fn stage(&self) -> Stage {
        self.stage
    }

    /// The progress of its piece at an instant, from its start: none before it.
    fn progress(&self, at: Instant) -> f64 {
        finite(self.piece.step * at.samples_since(self.piece.start).max(0.0))
    }

    /// T at an instant at or after its piece's start, at the offset O there; none
    /// where it is silent, at −∞ (MATH §4.6). An operator whose T lies below
    /// [`super::end_level`]'s level has gain 0 besides.
    #[must_use]
    pub fn level(&self, at: Instant, offset: f64) -> Option<f64> {
        let level = self.piece.law.level(self.progress(at));
        level.map(|level| finite(level + (offset - self.offset)))
    }

    /// The instant at which its segment ends, where the DX7's level has reached
    /// its target and its wait is over (MATH §4): none while it holds, and none
    /// where its speed is 0 or the instant lies beyond the samples that an
    /// instant counts.
    #[must_use]
    pub fn end(&self) -> Option<Instant> {
        let progress = self.piece.end?;
        if progress <= 0.0 {
            Some(self.piece.start)
        } else {
            self.piece.start.after(progress / self.piece.step)
        }
    }

    /// Ends its segment at [`Envelope::end`] and starts what follows there, at
    /// the offset O there (MATH §4.5): after segment 0 or 1 the next, from the
    /// level that the segment's law gives at its end's progress; after segment
    /// 2 the hold at L3, and after segment 3 the rest at L4, in each of which
    /// the level goes on by the segment's law. Nothing where it holds.
    pub fn next(&mut self, parameters: &Parameters, settings: &Settings, key: f64, offset: f64) {
        let (Some(at), Some(progress)) = (self.end(), self.piece.end) else {
            return;
        };
        match self.stage {
            Stage::Segment(segment @ 0..=1) => {
                let level = self
                    .piece
                    .law
                    .level(progress)
                    .map(|level| finite(level + (offset - self.offset)));
                self.begin(segment + 1, at, level, parameters, settings, key, offset);
            }
            Stage::Segment(2) => {
                self.stage = Stage::Held;
                self.piece.end = None;
            }
            Stage::Segment(_) => {
                self.stage = Stage::Resting;
                self.piece.end = None;
            }
            Stage::Held | Stage::Resting => {}
        }
    }

    /// A key-on at an instant, at the key of the rates and the offset O there
    /// (MATH §4.5): segment 0 from the level there, as the DX7's firmware has
    /// the EGS start it.
    pub fn key_on(&mut self, at: Instant, parameters: &Parameters, settings: &Settings, key: f64, offset: f64) {
        let level = self.level(at, offset);
        self.begin(0, at, level, parameters, settings, key, offset);
    }

    /// A key-off at an instant, once neither the key nor the pedal holds the note
    /// (MATH §4.5): segment 3 from the level there, by the rise or the fall as
    /// its direction says, a wait ending with the segment it was in. Nothing
    /// where segment 3 or the rest after it has begun.
    pub fn key_off(&mut self, at: Instant, parameters: &Parameters, settings: &Settings, key: f64, offset: f64) {
        if matches!(self.stage, Stage::Segment(3) | Stage::Resting) {
            return;
        }
        let level = self.level(at, offset);
        self.begin(3, at, level, parameters, settings, key, offset);
    }

    /// A change at an instant to its rates or levels, its rate scaling or the key
    /// of the rates (MATH §4.7): its law re-anchored there, from its level there
    /// toward its new target at its new speed, a rise jumping as at a segment's
    /// start, at its segment's offset, and its wait keeping what it has counted.
    /// A change of the offset alone needs none: [`Envelope::level`] takes it.
    pub fn change(&mut self, at: Instant, parameters: &Parameters, settings: &Settings, key: f64) {
        let progress = self.progress(at);
        let level = self.piece.law.level(progress);
        if matches!(self.stage, Stage::Segment(_)) {
            self.counted = finite(self.counted + progress);
        }
        self.anchor(at, level, parameters, settings, key);
    }

    /// Starts a segment at an instant from a level, at an offset.
    #[expect(
        clippy::too_many_arguments,
        reason = "a segment's start takes all that it depends on"
    )]
    fn begin(
        &mut self,
        segment: u8,
        at: Instant,
        from: Option<f64>,
        parameters: &Parameters,
        settings: &Settings,
        key: f64,
        offset: f64,
    ) {
        self.stage = Stage::Segment(segment);
        self.offset = offset;
        self.counted = 0.0;
        self.anchor(at, from, parameters, settings, key);
    }

    /// Anchors its law at an instant, from a level there toward its target at
    /// its speed, and its end where the DX7's segment ends (MATH §4.3–§4.6):
    /// where the DX7 sees it rise, an attack from max(T, 16), jumping, and
    /// elsewhere its own level's law, the DX7 falling or holding still; its wait
    /// counting what it has counted before.
    fn anchor(&mut self, at: Instant, from: Option<f64>, parameters: &Parameters, settings: &Settings, key: f64) {
        let index = match self.stage {
            Stage::Segment(segment) => usize::from(segment),
            Stage::Held => 2,
            Stage::Resting => 3,
        };
        let to = target(parameters.levels[index], self.offset);
        let rho = speed(
            rate(parameters.rates[index], parameters.rate_scaling, key),
            settings.clock_hz,
            settings.rate_law,
        );
        let (shown_from, shown_to) = (shown(from), shown(to));
        let (moved, law) = if shown_to > shown_from {
            let start = if settings.attack_jump {
                jump(shown_from, shown_to)
            } else {
                shown_from
            };
            (
                attack_progress(start, shown_to),
                Law::Rise {
                    from: start,
                    to: shown_to,
                },
            )
        } else {
            (shown_from - shown_to, own_law(from, to, settings.attack_jump))
        };
        let waited = if settings.waits {
            wait(index == 0, shown_to - shown_from)
        } else {
            0.0
        };
        let end = matches!(self.stage, Stage::Segment(_)).then(|| moved.max(waited - self.counted));
        self.piece = Piece {
            start: at,
            step: rho / settings.sample_rate,
            law,
            end,
        };
    }
}
