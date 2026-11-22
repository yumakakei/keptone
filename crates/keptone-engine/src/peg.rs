// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The pitch envelope (docs/MATH.md §5): its levels and speeds from the DX7's
//! tables, and the envelope itself, which moves a voice's pitch linearly
//! through L1, L2 and L3, holds it at L3, and takes it to L4 at the note's
//! end, its level at each instant in closed form from the start of its piece
//! of law.
//!
//! A level is a pitch in octaves, the pitch envelope's of MATH §2.4, 0 at
//! L = 50; a speed is in
//! octaves a second. Each function documents its error against the law's exact
//! value at its binary64 inputs; tests/peg.rs measures each against exact
//! values (`scripts/peg_references.py`). At each instant, the segments that end
//! at or before it end first ([`PitchEnvelope::end`], [`PitchEnvelope::next`]),
//! then the events there act, and then the level is read, as the operators'
//! envelopes are (MATH §0.3).

use keptone_math::exp2;

use crate::binary64::finite;
use crate::eg::envelope::Stage;
use crate::steps::between;
use crate::time::Instant;

/// pt of MATH §5 at the steps 0 to 99, in 1/32 octave: MSFA's `pitchtab`
/// \[MSFA `pitchenv.cc`\], the firmware's values as well \[AJXS-ROM
/// `PITCH_EG_PROCESS`\].
const LEVELS: [f64; 100] = [
    -128.0, -116.0, -104.0, -95.0, -85.0, -76.0, -68.0, -61.0, -56.0, -52.0, -49.0, -46.0, -43.0, -41.0, -39.0, -37.0,
    -35.0, -33.0, -32.0, -31.0, -30.0, -29.0, -28.0, -27.0, -26.0, -25.0, -24.0, -23.0, -22.0, -21.0, -20.0, -19.0,
    -18.0, -17.0, -16.0, -15.0, -14.0, -13.0, -12.0, -11.0, -10.0, -9.0, -8.0, -7.0, -6.0, -5.0, -4.0, -3.0, -2.0,
    -1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 19.0,
    20.0, 21.0, 22.0, 23.0, 24.0, 25.0, 26.0, 27.0, 28.0, 29.0, 30.0, 31.0, 32.0, 33.0, 34.0, 35.0, 38.0, 40.0, 43.0,
    46.0, 49.0, 53.0, 58.0, 65.0, 73.0, 82.0, 92.0, 103.0, 115.0, 127.0,
];

/// pr of MATH §5 at the steps 0 to 99, in 1/4096 octave at every second
/// interrupt of the firmware's timer: MSFA's `ratetab` \[MSFA `pitchenv.cc`\],
/// the firmware's values as well \[AJXS-ROM `PITCH_EG_PROCESS`\].
const RATES: [f64; 100] = [
    1.0, 2.0, 3.0, 3.0, 4.0, 4.0, 5.0, 5.0, 6.0, 6.0, 7.0, 7.0, 8.0, 8.0, 9.0, 9.0, 10.0, 10.0, 11.0, 11.0, 12.0, 12.0,
    13.0, 13.0, 14.0, 14.0, 15.0, 16.0, 16.0, 17.0, 18.0, 18.0, 19.0, 20.0, 21.0, 22.0, 23.0, 24.0, 25.0, 26.0, 27.0,
    28.0, 30.0, 31.0, 33.0, 34.0, 36.0, 37.0, 38.0, 39.0, 41.0, 42.0, 44.0, 46.0, 47.0, 49.0, 51.0, 53.0, 54.0, 56.0,
    58.0, 60.0, 62.0, 64.0, 66.0, 68.0, 70.0, 72.0, 74.0, 76.0, 79.0, 82.0, 85.0, 88.0, 91.0, 94.0, 98.0, 102.0, 106.0,
    110.0, 115.0, 120.0, 125.0, 130.0, 135.0, 141.0, 147.0, 153.0, 159.0, 165.0, 171.0, 178.0, 185.0, 193.0, 202.0,
    211.0, 232.0, 243.0, 254.0, 255.0,
];

/// The rate of the firmware's timer over 8192, 9.4265 MHz/(8·3165·8192),
/// correctly rounded: the octaves a
/// second that a step of pr gives, pr/4096 octave at every second interrupt of
/// the firmware's timer (MATH §5).
const OCTAVES_PER_RATE: f64 = f64::from_bits(0x3fa7_44b6_cca3_6315);

/// A whole step of 0 to 99 as an index of the tables.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole binary64 of [0, 99] converts exactly"
)]
fn index(step: f64) -> usize {
    step as usize
}

/// The level of MATH §5 of an EG level L, in octaves: pt(L)/32 at the steps,
/// linear between them (MATH §0.6), and beyond the scale along its end steps,
/// 12/32 octave a step below 0 and above 99; 0 at L = 50.
///
/// At a step exact; between the steps one fused rounding of the step's part,
/// itself exact, beyond the scale the rounding of L − 99 as well, and the
/// division by 32 exact: within u|pt|/32 of the law, and u·12|L − 99|/32 more
/// above 99.
#[must_use]
pub fn level(level: f64) -> f64 {
    let pitch = if level < 0.0 {
        finite(12.0_f64.mul_add(level, -128.0))
    } else if level > 99.0 {
        finite(12.0_f64.mul_add(level - 99.0, 127.0))
    } else {
        between(level, |step| LEVELS[index(step)])
    };
    pitch / 32.0
}

/// pr(R) of MATH §5: the table at the steps, linear between them (MATH §0.6),
/// along its last step above 99, and below 0 halving at each step and
/// between the steps in its logarithm, 2^R, as the LFO's speed does (§6.1).
fn rate(rate: f64) -> f64 {
    if rate < 0.0 {
        exp2(rate)
    } else if rate > 99.0 {
        finite((rate - 99.0) + 255.0)
    } else {
        between(rate, |step| RATES[index(step)])
    }
}

/// The speed of MATH §5 of a rate R, in octaves a second: pr(R) times the
/// rate of the firmware's timer, 9.4265 MHz/(8·3165), over 8192, pr/22.004,
/// the timer moving the level by pr/4096 octave at every second interrupt
/// \[AJXS-ROM `HANDLER_OCF`, `PITCH_EG_PROCESS`\]. A speed below binary64's
/// least is 0, at which a segment never moves.
///
/// pr is exact at the steps and within u of itself between them, within
/// u(|R − 99| + |pr|) above 99 and within exp2's 0.93 ulp, 1.86u, below 0; the
/// constant and the product round within u each: within 3u of the speed
/// relatively from 0 to 99, 3.86u below 0, and u|R − 99|/22.004 octaves a
/// second more above 99.
#[must_use]
pub fn speed(rate_parameter: f64) -> f64 {
    finite(rate(rate_parameter) * OCTAVES_PER_RATE)
}

/// A voice's parameters that its pitch envelope takes (MATH §5): its rates R1
/// to R4 and its levels L1 to L4, 0 to 99 on their scales.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Parameters {
    /// R1 to R4.
    pub rates: [f64; 4],
    /// L1 to L4.
    pub levels: [f64; 4],
}

/// A piece of the pitch envelope's law: a level moving linearly from an instant
/// on toward a target at a speed.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Piece {
    /// The instant from which its progress counts.
    start: Instant,
    /// Its progress a sample, the speed over the engine's rate, in octaves.
    step: f64,
    /// Its level at its start.
    from: f64,
    /// Its target.
    to: f64,
    /// The progress at which its segment ends, |to − from|, none where the
    /// envelope holds.
    end: Option<f64>,
}

impl Piece {
    /// The level after a progress: from + Φ or from − Φ toward the target, and
    /// the target from there on.
    fn level(self, progress: f64) -> f64 {
        if self.to >= self.from {
            finite(self.from + progress).min(self.to)
        } else {
            finite(self.from - progress).max(self.to)
        }
    }
}

/// The pitch envelope of a voice (MATH §5).
///
/// Its level at an instant is its piece's start moved by the progress
/// step·Δ toward its target, Δ the samples since the piece's start and the
/// step the speed over the engine's rate: the speed within the bound that
/// [`speed`] documents, the quotient within u, Δ within u(|Δ| + 1/2) and the
/// product within u, so that the progress lies within Φ(ε + 2u) + u·step/2
/// of its exact value, ε the step's relative error, and a step more for each
/// sample of its start's error; the sum rounds within u of the level, and the
/// target, which [`level`] gives, stops it exactly. A piece starts from a
/// target, or from where the piece before it was; an end lies within its
/// start's error, and its distance's and the step's over the step, of the
/// exact instant. `scripts/peg_references.py` follows these through every
/// piece of the scenarios that tests/peg.rs runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchEnvelope {
    /// Where it is: in a segment, held at L3 past segment 2, or at L4 past
    /// segment 3.
    stage: Stage,
    /// Its piece of law.
    piece: Piece,
}

impl PitchEnvelope {
    /// The pitch envelope of a voice that has had no note: at L4's level, from
    /// an instant on.
    #[must_use]
    pub fn new(parameters: &Parameters, at: Instant) -> Self {
        let rest = level(parameters.levels[3]);
        Self {
            stage: Stage::Resting,
            piece: Piece {
                start: at,
                step: 0.0,
                from: rest,
                to: rest,
                end: None,
            },
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

    /// The level in octaves at an instant at or after its piece's start.
    #[must_use]
    pub fn level(&self, at: Instant) -> f64 {
        self.piece.level(self.progress(at))
    }

    /// The instant at which its segment ends, where its level reaches its
    /// target, for the pitch envelope has no waits (MATH §5): at once where the
    /// target is the level the segment starts at; none while it holds, and
    /// none where its speed is 0 or the instant lies beyond the samples that an
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

    /// Ends its segment at [`PitchEnvelope::end`] and starts what follows there:
    /// after segment 0 or 1 the next, from the target reached; after segment 2
    /// the hold at L3, and after segment 3 the rest at L4. Nothing where it
    /// holds.
    pub fn next(&mut self, parameters: &Parameters, sample_rate: f64) {
        let Some(at) = self.end() else {
            return;
        };
        match self.stage {
            Stage::Segment(segment @ 0..=1) => self.begin(segment + 1, at, self.piece.to, parameters, sample_rate),
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

    /// A key-on at an instant (MATH §5): segment 0 from L4's level, which the
    /// level takes there at once, as the DX7's firmware starts the pitch
    /// envelope at every key-on of a voice. A mono part's legato key-on, which
    /// another held key precedes, sends none (§7.3).
    pub fn key_on(&mut self, at: Instant, parameters: &Parameters, sample_rate: f64) {
        self.begin(0, at, level(parameters.levels[3]), parameters, sample_rate);
    }

    /// A key-off at an instant, once neither the key nor the pedal holds the note
    /// (MATH §5): segment 3 from the level there toward L4. Nothing where segment
    /// 3 or the rest after it has begun.
    pub fn key_off(&mut self, at: Instant, parameters: &Parameters, sample_rate: f64) {
        if matches!(self.stage, Stage::Segment(3) | Stage::Resting) {
            return;
        }
        let from = self.level(at);
        self.begin(3, at, from, parameters, sample_rate);
    }

    /// A change of its rates or levels at an instant (MATH §4.7): its law
    /// re-anchored there, from its level there toward its new target at its new
    /// speed.
    pub fn change(&mut self, at: Instant, parameters: &Parameters, sample_rate: f64) {
        let from = self.level(at);
        self.anchor(at, from, parameters, sample_rate);
    }

    /// Starts a segment at an instant from a level.
    fn begin(&mut self, segment: u8, at: Instant, from: f64, parameters: &Parameters, sample_rate: f64) {
        self.stage = Stage::Segment(segment);
        self.anchor(at, from, parameters, sample_rate);
    }

    /// Anchors its law at an instant, from a level there toward its target at its
    /// speed, and its end where the level reaches the target.
    fn anchor(&mut self, at: Instant, from: f64, parameters: &Parameters, sample_rate: f64) {
        let index = match self.stage {
            Stage::Segment(segment) => usize::from(segment),
            Stage::Held => 2,
            Stage::Resting => 3,
        };
        let to = level(parameters.levels[index]);
        let end = matches!(self.stage, Stage::Segment(_)).then(|| finite(to - from).abs());
        self.piece = Piece {
            start: at,
            step: speed(parameters.rates[index]) / sample_rate,
            from,
            to,
            end,
        };
    }
}
