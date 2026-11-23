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
//! of the rates starts a new piece there (§4.7). Where the key of the rates
//! moves within a sample, [`Envelope::run`] drives it over that sample: each
//! piece of the key adds the progress that it makes to a base from which the
//! piece of law counts, and a segment ends at the instant within the sample
//! at which its progress reaches its end.
//!
//! Its time is the engine's, in instants (§0.1). At each instant, the segments
//! that end at or before it end first ([`Envelope::end`], [`Envelope::next`]),
//! as a sample at or after a segment's end takes the next segment; then the
//! events there act; and then the level is read (MATH §0.3).

use keptone_math::eft::two_sum;

use crate::binary64::finite;
use crate::level::{envelope as envelope_level, total};
use crate::pieces::Keys;
use crate::pitch::phase::gauss_instants;
use crate::time::Instant;

use super::progress::{Rates, Reach, advance};
use super::{FLOOR, RateLaw, attack, attack_progress, jump, rate, speed, wait};

/// The most segments that end within one sample: segments 0, 1 and 2 to the
/// hold, or segment 3 to the rest, and one more for a key-on's.
const ENDS: usize = 4;

/// The instant τ samples after the start of a sample, exactly for τ from 0 to 1.
fn instant_after(from: Instant, samples: f64) -> Instant {
    from.after(samples).unwrap_or(from)
}

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
    /// The instant from which its progress counts at its step.
    start: Instant,
    /// The progress that it had made by `start`: 0 where it begins there, and
    /// where the rates' key moves (MATH §4.7) the sum of what the samples
    /// since its beginning have made, beside `low`.
    base: f64,
    /// What the sums of `base` have rounded away.
    low: f64,
    /// Its progress a sample from `start`, ρ/fₑ.
    step: f64,
    /// The rates' key at which its step is taken.
    key: f64,
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
/// Its level at an instant follows its piece's law at the progress Φ = Φ₀ +
/// step·Δ, Δ the samples since the piece's start, the step ρ/fₑ and Φ₀ the
/// progress that the samples driven since the law's start made, 0 where none
/// was: each piece of the key's within the bound that
/// `super::progress::advance` gives, or past the budget the rule's on the
/// sample's equal parts, summed by two-sums, whose total rounds within u|Φ|.
/// The speed is within 2.5u of its value at the rate that [`rate`] gives, 2.86u
/// under the exponential law ([`speed`]), and within a quarter of the rate's
/// error more, the speed's slope in the rate being at most a quarter of the
/// speed; the quotient rounds within u, Δ within u(|Δ| + 1/2) and the product
/// within u: the progress lies within Φ(ε + 2u) + u·step/2 of ∫ρ dt at the
/// parameters' binary64 values, ε the step's relative error, and a step more
/// for each sample of its start's error. A law passes the progress's error on
/// by its slope, 1 for a fall and max(1, 2 + (3840 − T₀)/256) for an attack
/// from T₀, beside its own ([`attack`]); a piece starts from a target, which
/// carries the errors of E(L) and of the total level ([`envelope_level`],
/// [`total`]), or from where the piece before it was, whose error its law
/// passes on; and an end lies within its start's error, and its progress's and
/// the step's over the step, of the exact instant.
/// `scripts/envelope_references.py` follows these through every piece of the
/// scenarios that tests/envelope.rs runs, and `scripts/drive_references.py`
/// through the driven samples of those that tests/drive.rs runs.
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
                base: 0.0,
                low: 0.0,
                step: 0.0,
                key: f64::NEG_INFINITY,
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

    /// The progress of its piece at an instant: what it had made by its start
    /// and its step times the samples since, none before it.
    fn progress(&self, at: Instant) -> f64 {
        let moved = finite(self.piece.step * at.samples_since(self.piece.start).max(0.0));
        finite(self.piece.base + (self.piece.low + moved))
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
        let left = finite(self.piece.end? - (self.piece.base + self.piece.low));
        if left <= 0.0 {
            Some(self.piece.start)
        } else {
            self.piece.start.after(left / self.piece.step)
        }
    }

    /// Ends its segment at [`Envelope::end`] and starts what follows there, at
    /// the offset O there (MATH §4.5): after segment 0 or 1 the next, from the
    /// level that the segment's law gives at its end's progress; after segment
    /// 2 the hold at L3, and after segment 3 the rest at L4, in each of which
    /// the level goes on by the segment's law. Nothing where it holds.
    pub fn next(&mut self, parameters: &Parameters, settings: &Settings, key: f64, offset: f64) {
        if let Some(at) = self.end() {
            self.finish(at, parameters, settings, key, offset);
        }
    }

    /// Ends its segment at an instant at which its progress reaches the
    /// segment's end, and starts what follows there, as [`Envelope::next`]
    /// does at [`Envelope::end`].
    fn finish(&mut self, at: Instant, parameters: &Parameters, settings: &Settings, key: f64, offset: f64) {
        let Some(progress) = self.piece.end else {
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

    /// The rates of its stage (MATH §4.1): the rate R of its segment, R3 while
    /// it holds and R4 while it rests, and the operator's rate scaling, with
    /// the settings.
    fn rates(&self, parameters: &Parameters, settings: &Settings) -> Rates {
        let index = match self.stage {
            Stage::Segment(segment) => usize::from(segment),
            Stage::Held => 2,
            Stage::Resting => 3,
        };
        Rates {
            rate: parameters.rates[index],
            sensitivity: parameters.rate_scaling,
            clock_hz: settings.clock_hz,
            law: settings.rate_law,
            sample_rate: settings.sample_rate,
        }
    }

    /// Takes its piece's progress at an instant into its base, from which it
    /// counts at no step: the progress that a sample on which the rates' key
    /// moves adds to.
    fn freeze(&mut self, at: Instant) {
        let moved = finite(self.piece.step * at.samples_since(self.piece.start).max(0.0));
        let (base, error) = two_sum(self.piece.base, finite(self.piece.low + moved));
        self.piece.base = base;
        self.piece.low = error;
        self.piece.start = at;
        self.piece.step = 0.0;
    }

    /// Adds a progress to its base, keeping what the sum rounds away beside it.
    fn add(&mut self, progress: f64) {
        let (base, error) = two_sum(self.piece.base, progress);
        self.piece.base = base;
        self.piece.low = finite(self.piece.low + error);
    }

    /// Its closed form from an instant on at the speed of a key, which a sample
    /// where the key holds still there takes on.
    fn settle(&mut self, at: Instant, key: f64, parameters: &Parameters, settings: &Settings) {
        let rates = self.rates(parameters, settings);
        self.piece.start = at;
        self.piece.key = key;
        self.piece.step = rates.step(key);
    }

    /// The progress left in its segment from its base, none where it holds.
    fn remaining(&self) -> Option<f64> {
        let made = finite(self.piece.base + self.piece.low);
        self.piece.end.map(|end| finite(end - made).max(0.0))
    }

    /// Runs it over the sample that runs one engine sample from an instant
    /// (MATH §4.7), at the rates' key over that sample, `keys`, and the offset
    /// O that a segment starting at an instant within it takes, calling `ends`
    /// with the instant of each segment that ends within the sample, in their
    /// order, once the sample is done.
    ///
    /// Where the key holds still over the sample at the key that its piece's
    /// speed was taken at, its closed form goes on, each segment ending at
    /// [`Envelope::end`]. Elsewhere each piece of the key gives its progress
    /// (`super::progress::advance`), which a two-sum adds to the piece's base,
    /// and a segment whose progress reaches its end within a piece ends at
    /// that instant, where the next starts and the piece goes on at its
    /// rates; at the sample's end the closed form takes over from there at the
    /// key's speed there. A sample past the budget of `osc.max_pieces` takes
    /// instead the rule on that many equal parts of the sample, ρ at its
    /// instants, a segment that ends within a part ending where its progress,
    /// taken as even across the part, reaches its end.
    pub fn run(
        &mut self,
        from: Instant,
        keys: &Keys,
        parameters: &Parameters,
        settings: &Settings,
        offset: impl Fn(Instant) -> f64,
        mut ends: impl FnMut(Instant),
    ) {
        let to = instant_after(from, 1.0);
        if let Some(key) = keys.still()
            && finite(key - self.piece.key) == 0.0
        {
            for _ in 0..ENDS {
                let Some(end) = self.end().filter(|end| *end <= to) else {
                    break;
                };
                self.next(parameters, settings, key, offset(end));
                ends(end);
            }
            return;
        }
        let saved = *self;
        let mut found = [None; ENDS];
        let walked = keys
            .pieces()
            .is_some_and(|pieces| self.drive(from, pieces, keys.limit(), parameters, settings, &offset, &mut found));
        if !walked {
            *self = saved;
            found = [None; ENDS];
            self.fallback(from, keys, parameters, settings, &offset, &mut found);
        }
        for end in found.into_iter().flatten() {
            ends(end);
        }
    }

    /// The driven walk over a sample's pieces of the key, within the budget;
    /// false where it runs out.
    #[expect(clippy::too_many_arguments, reason = "a sample's run takes all that it depends on")]
    fn drive(
        &mut self,
        from: Instant,
        pieces: &[crate::pieces::KeyPiece],
        limit: u32,
        parameters: &Parameters,
        settings: &Settings,
        offset: &impl Fn(Instant) -> f64,
        found: &mut [Option<Instant>; ENDS],
    ) -> bool {
        self.freeze(from);
        let mut budget = f64::from(limit);
        let mut count = 0;
        let mut last = self.piece.key;
        for piece in pieces {
            let (mut key, mut start, mut length) = (piece.key, piece.start, piece.length);
            loop {
                let rates = self.rates(parameters, settings);
                let left = self.remaining();
                match advance(&rates, &key, length, left, &mut budget) {
                    None => return false,
                    Some(Reach::Beyond(progress)) => {
                        self.add(progress);
                        last = key.at(length);
                        break;
                    }
                    Some(Reach::Within(samples)) => {
                        if count >= ENDS {
                            return false;
                        }
                        let at = instant_after(from, start + samples);
                        let reached = key.at(samples);
                        // The progress to the end, which a hold or the rest
                        // after it goes on from by the same law.
                        self.add(left.unwrap_or(0.0));
                        self.finish(at, parameters, settings, reached, offset(at));
                        self.freeze(at);
                        found[count] = Some(at);
                        count += 1;
                        key = key.later(samples);
                        start += samples;
                        length -= samples;
                    }
                }
            }
        }
        self.settle(instant_after(from, 1.0), last, parameters, settings);
        true
    }

    /// The fallback past the budget: the rule on `osc.max_pieces` equal parts
    /// of the sample, the rates' key at its instants from the sample's pieces
    /// or, past their budget, as the key's own fallback took it.
    fn fallback(
        &mut self,
        from: Instant,
        keys: &Keys,
        parameters: &Parameters,
        settings: &Settings,
        offset: &impl Fn(Instant) -> f64,
        found: &mut [Option<Instant>; ENDS],
    ) {
        self.freeze(from);
        let count = keys.limit();
        let boundary = |index: u32| {
            if index >= count {
                1.0
            } else {
                f64::from(index) / f64::from(count)
            }
        };
        let held = self.piece.key;
        let key_at = |index: u32, node: usize, samples: f64| match keys.pieces() {
            Some(pieces) => pieces
                .iter()
                .find(|piece| samples < piece.start + piece.length)
                .or(pieces.last())
                .map_or(held, |piece| piece.key.at(samples - piece.start)),
            None => keys
                .nodes()
                .get(usize::try_from(index).unwrap_or(usize::MAX))
                .map_or(held, |nodes| nodes[node]),
        };
        let mut ended = 0;
        let mut last = held;
        for index in 0..count {
            let start = boundary(index);
            let length = boundary(index + 1) - start;
            let instants = gauss_instants(length);
            let at_nodes = [0, 1, 2].map(|node| key_at(index, node, start + instants[node]));
            let mut taken = 0.0;
            loop {
                let rates = self.rates(parameters, settings);
                let [first, middle, end] = at_nodes.map(|key| rates.step(key));
                let whole = finite(length * 5.0_f64.mul_add(first + end, 8.0 * middle) / 18.0);
                let rest = finite(whole * (1.0 - taken));
                match self.remaining() {
                    Some(left) if left <= rest && whole > 0.0 && ended < ENDS => {
                        taken = (taken + left / whole).min(1.0);
                        let at = instant_after(from, start + taken * length);
                        self.add(left);
                        self.finish(at, parameters, settings, at_nodes[1], offset(at));
                        self.freeze(at);
                        found[ended] = Some(at);
                        ended += 1;
                    }
                    _ => {
                        self.add(rest);
                        break;
                    }
                }
            }
            last = at_nodes[2];
        }
        self.settle(instant_after(from, 1.0), last, parameters, settings);
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
            base: 0.0,
            low: 0.0,
            step: rho / settings.sample_rate,
            key,
            law,
            end,
        };
    }
}
