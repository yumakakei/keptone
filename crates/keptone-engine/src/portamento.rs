// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The portamento of docs/MATH.md §7.3: its speed from its time, and a voice's
//! glide toward its target, by the law dd/dt = −κ(1 + 4d) in closed form from
//! its last anchor, or, with glissando, at a constant speed in keys, the key
//! heard stepping a whole key toward the glide. Each is a function of
//! continuous time whose changes lie at instants that it gives in their order,
//! which the voice ends before it reads them (MATH §0.3). Which voices glide,
//! and from where, the part's poly and mono modes say (§7.3), as the voice
//! gives them.
//!
//! Each function documents its error against the law's exact value at its
//! binary64 inputs; tests/portamento.rs measures each against exact values
//! (`scripts/portamento_references.py`).

use std::f64::consts::{LN_2, LOG2_E};

use keptone_math::{exp2m1, log2p1};

use crate::binary64::finite;
use crate::peg;
use crate::time::Instant;

/// κ of MATH §7.3, the portamento's speed in octaves a second at a TIME: the
/// pitch envelope's speed of the rate 99 − TIME, pr\[99 − TIME\]·F/8192 (MATH
/// §5), between and beyond its steps as §5 takes it; from TIME 1 down to 0 the
/// glide's time scale 1/κ falls linearly to 0, κ is κ(1)/TIME, taken at
/// binary64's largest finite number where it lies beyond it; and none at or
/// below 0, where there is no glide, the firmware taking its fastest rate at
/// TIME 0 \[AJXS-ROM `VOICE_ADD_POLY`\].
///
/// From TIME 1 on, the pitch envelope's speed at 99 − TIME as it rounds, exact
/// where TIME lies from 49.5 to 198 and within u|99 − TIME| of itself
/// elsewhere, within that speed's bound (`peg::speed`); below 1, the speed at
/// 98 within 3u and the quotient within u: within 4u.
#[must_use]
pub fn speed(time: f64) -> Option<f64> {
    if time <= 0.0 {
        None
    } else if time < 1.0 {
        Some(finite(peg::speed(98.0) / time))
    } else {
        Some(peg::speed(99.0 - time))
    }
}

/// The unit of a glide's offset (MATH §7.3): octaves where the portamento moves
/// the pitch, keys where it moves the note value (`tuning.modulation = keys`,
/// MATH §2.1), 12 to an octave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// Octaves of pitch.
    Octaves,
    /// Keys of the note value, 12 to an octave.
    Keys,
}

impl Unit {
    /// A quarter of an octave in the unit, which scales the glide's law.
    const fn quarter(self) -> f64 {
        match self {
            Self::Octaves => 0.25,
            Self::Keys => 3.0,
        }
    }
}

/// A glide of MATH §7.3 toward its target without glissando: the offset from
/// the target, whose distance d falls by dd/dt = −κ(1 + 4d) in octaves to 0,
/// in closed form from its anchor, d(t) = d₀ + (d₀ + q)·(e^(−4κt) − 1), q a
/// quarter of an octave in its unit, until it reaches 0 at
/// t* = ln(1 + d₀/q)/(4κ).
///
/// Its offset at an instant is within u(|d₀ + q|(2.24|m| + 3x·e^(−x)) + |d|)
/// of its exact value at its binary64 anchor, x = 4κt and m = e^(−x) − 1: 4κt
/// is within 3u of itself, the product by the samples and by log₂ e, and
/// exp2m1 within 0.62 ulp, d₀ + q within u and the fused step within u|d|;
/// so within 4.35u(d₀ + q) at most. 4κ/fₑ is within u of itself beside κ's
/// error, and the arrival within 5.62u of t* relatively, that u among them,
/// and u more where q is 3, d₀/q's rounding, beside the rounding of its
/// instant (`Instant::after`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glide {
    /// The instant of its anchor: its start or its last change.
    start: Instant,
    /// The offset from the target there, its sign the side the glide comes
    /// from.
    offset: f64,
    /// 4κ/fₑ, a sample's part of the law's rate.
    rate: f64,
    /// The unit of its offset.
    unit: Unit,
    /// The instant at which it reaches its target, if it does.
    end: Option<Instant>,
}

impl Glide {
    /// A glide from an instant on, from an offset from its target in a unit,
    /// at a speed κ in octaves a second ([`speed`]), at the engine's rate.
    #[must_use]
    pub fn new(at: Instant, offset: f64, speed: f64, unit: Unit, sample_rate: f64) -> Self {
        Self::anchored(at, offset, finite(4.0 * speed / sample_rate), unit)
    }

    /// A glide anchored at an instant at a sample's part of its rate.
    fn anchored(at: Instant, offset: f64, rate: f64, unit: Unit) -> Self {
        let distance = offset.abs();
        let end = if distance > 0.0 && rate > 0.0 {
            at.after(LN_2 * log2p1(distance / unit.quarter()) / rate)
        } else {
            None
        };
        Self {
            start: at,
            offset,
            rate,
            unit,
            end,
        }
    }

    /// The offset from the target at an instant at or after its anchor, in its
    /// unit: d₀ + (d₀ + q)·(e^(−4κt) − 1) with the offset's sign, and 0 from
    /// where it reaches the target on.
    #[must_use]
    pub fn offset(&self, at: Instant) -> f64 {
        let samples = at.samples_since(self.start).max(0.0);
        let from = self.offset.abs();
        let falling = exp2m1(-(self.rate * samples) * LOG2_E);
        let distance = (from + self.unit.quarter()).mul_add(falling, from).max(0.0);
        distance.copysign(self.offset)
    }

    /// The instant at which it reaches its target, if it has not and does:
    /// none where it lies at the target or does not move.
    #[must_use]
    pub const fn end(&self) -> Option<Instant> {
        self.end
    }

    /// Reaches its target at its end: from there on its offset is 0.
    pub fn next(&mut self) {
        if let Some(end) = self.end {
            *self = Self {
                start: end,
                offset: 0.0,
                rate: self.rate,
                unit: self.unit,
                end: None,
            };
        }
    }

    /// A change of its target at an instant, which moves the offset by a shift
    /// in its unit (MATH §7.3): re-anchored there from the pitch there.
    pub fn retarget(&mut self, at: Instant, shift: f64) {
        *self = Self::anchored(at, finite(self.offset(at) + shift), self.rate, self.unit);
    }

    /// A change of its speed at an instant (MATH §7.3): re-anchored there from
    /// the pitch there.
    pub fn set_speed(&mut self, at: Instant, speed: f64, sample_rate: f64) {
        *self = Self::new(at, self.offset(at), speed, self.unit, sample_rate);
    }
}

/// A glissando of MATH §7.3: a glide of its own toward the target in keys at
/// a constant speed, 36κ keys a second, and the key heard, which steps a
/// whole key toward that glide each time the glide lies a whole key from it,
/// and takes the target when the glide reaches it, as the firmware moves its
/// glissando's pitch \[AJXS-ROM `PORTA_PROCESS`\]. The key heard is constant
/// between those instants, which [`Glissando::end`] gives in their order. A
/// change keeps the glide where it lies between keys and the key heard
/// (MATH §7.3).
///
/// Its speed in keys a sample, 36κ/fₑ, is within 2u of itself beside κ's
/// error; an instant at which the key heard changes lies within u(|h − g| +
/// |n|)/v of its exact value at its binary64 anchor, h and g the key heard and
/// the glide there, n the distance that the glide covers and v the speed, and
/// 3u of it relatively more, beside the rounding of its instant
/// (`Instant::after`); the glide at an instant within u(|g| + 2vΔ) of its
/// exact value; the key heard within u of itself, one sum from the anchor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glissando {
    /// The instant of its anchor: its start or its last change.
    start: Instant,
    /// The glide's offset from the target there, in keys.
    glide: f64,
    /// The key heard's offset from the target there.
    heard: f64,
    /// The whole keys that the key heard has stepped since the anchor.
    steps: f64,
    /// Its speed in keys a sample, 36κ/fₑ.
    step: f64,
    /// The instant at which the key heard next changes, if it does.
    end: Option<Instant>,
}

impl Glissando {
    /// A glissando from an instant on, its glide and its key heard at an offset
    /// from its target in keys, at a speed κ in octaves a second ([`speed`]),
    /// at the engine's rate.
    #[must_use]
    pub fn new(at: Instant, offset: f64, speed: f64, sample_rate: f64) -> Self {
        Self::anchored(at, offset, offset, finite(36.0 * speed) / sample_rate)
    }

    /// A glissando anchored at an instant with its glide and its key heard.
    fn anchored(at: Instant, glide: f64, heard: f64, step: f64) -> Self {
        let mut glissando = Self {
            start: at,
            glide,
            heard,
            steps: 0.0,
            step,
            end: None,
        };
        glissando.end = glissando.next_end();
        glissando
    }

    /// The direction in which the glide moves: up where it lies below the
    /// target.
    fn direction(&self) -> f64 {
        if self.glide < 0.0 { 1.0 } else { -1.0 }
    }

    /// Whether the key heard steps before the glide reaches its target: the key
    /// after the step still lies on the glide's side of the target.
    fn steps_first(&self) -> bool {
        self.steps + 1.0 < -self.direction() * self.heard
    }

    /// The instant of the next change of the key heard: the glide a whole key
    /// beyond it, or the glide at the target; none where the glide has
    /// reached the target or does not move.
    fn next_end(&self) -> Option<Instant> {
        if self.glide == 0.0 {
            return (self.heard != 0.0).then_some(self.start);
        }
        if self.step <= 0.0 {
            return None;
        }
        let direction = self.direction();
        let samples = if self.steps_first() {
            finite(direction.mul_add(finite(self.heard - self.glide), self.steps + 1.0)) / self.step
        } else {
            -direction * self.glide / self.step
        };
        self.start.after(samples)
    }

    /// The key heard's offset from the target, in keys, from its last change
    /// to its next ([`Glissando::end`]).
    #[must_use]
    pub fn offset(&self) -> f64 {
        finite(self.direction().mul_add(self.steps, self.heard))
    }

    /// The glide's offset from the target at an instant at or after its
    /// anchor, in keys: the anchor's moved at its speed toward the target, and
    /// 0 from where it reaches the target on.
    #[must_use]
    pub fn glide(&self, at: Instant) -> f64 {
        let samples = at.samples_since(self.start).max(0.0);
        let direction = self.direction();
        let glide = finite((direction * self.step).mul_add(samples, self.glide));
        if direction * glide >= 0.0 { 0.0 } else { glide }
    }

    /// The instant at which the key heard next changes, if it does.
    #[must_use]
    pub const fn end(&self) -> Option<Instant> {
        self.end
    }

    /// Changes the key heard at its end: a whole key toward the glide, or the
    /// target where the glide reaches it.
    pub fn next(&mut self) {
        let Some(end) = self.end else {
            return;
        };
        if self.glide != 0.0 && self.steps_first() {
            self.steps += 1.0;
            self.end = self.next_end();
        } else {
            *self = Self {
                start: end,
                glide: 0.0,
                heard: 0.0,
                steps: 0.0,
                step: self.step,
                end: None,
            };
        }
    }

    /// A change of its target at an instant, which moves its offsets by a
    /// shift in keys (MATH §7.3): re-anchored there, its glide and its key
    /// heard kept.
    pub fn retarget(&mut self, at: Instant, shift: f64) {
        let glide = finite(self.glide(at) + shift);
        let heard = finite(self.offset() + shift);
        *self = Self::anchored(at, glide, heard, self.step);
    }

    /// A change of its speed at an instant (MATH §7.3): re-anchored there, its
    /// glide and its key heard kept.
    pub fn set_speed(&mut self, at: Instant, speed: f64, sample_rate: f64) {
        let (glide, heard) = (self.glide(at), self.offset());
        *self = Self::anchored(at, glide, heard, finite(36.0 * speed) / sample_rate);
    }
}
