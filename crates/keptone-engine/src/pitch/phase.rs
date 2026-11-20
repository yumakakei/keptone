// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The phase of an operator (docs/MATH.md §2.5): its increment over a piece of
//! a sample, in closed form where the pitch moves linearly and by the
//! Gauss–Legendre rule where it moves smoothly, and the phase that the
//! increments advance, in turns.
//!
//! A piece is the caller's: a sample split at every corner and every jump of
//! the pitch (MATH §2.5). Each increment is r·∫2^p(τ) dτ over the piece, the
//! pitch held at 1023 octaves, a frequency ratio r times the pitch's frequency
//! integrated, and documents its error against the exact value at its binary64
//! inputs; tests/phase.rs measures each against mpmath's
//! (`scripts/phase_references.py`). An increment of 2⁵² turns or more is a
//! whole number of turns at binary64's precision, and needs no more than to be
//! finite.

use core::f64::consts::LN_2;

use keptone_math::eft::{two_product, two_sum};
use keptone_math::{exp2, exp2m1};

use super::{HIGHEST_PITCH, finite, scaled, split};

/// Below this the mean of 2⁻ˣ over [0, w] is 1 − w ln 2/2 within u/2500.
const SMALL: f64 = 1.0 / 1_073_741_824.0;

/// F(w) = (1 − 2⁻ʷ)/(w ln 2) for w ≥ 0, the mean of 2⁻ˣ over x from 0 to w,
/// F(0) = 1: from 1 down toward 1/(w ln 2), and 0 at w = +∞.
///
/// Below 2⁻³⁰ it is 1 − w·(ln 2/2), whose truncation, (w ln 2)²/6, is below
/// 0.0004u, and whose roundings are u of the product's small term and u of the
/// difference: within 1.001u. Above, −exp2m1(−w)/(w ln 2), exp2m1 within
/// 0.62 ulp, 1.24u, ln 2 and the product u each, and the division u: within
/// 4.24u relatively, but for terms of the order of u².
fn mean_of_falling(w: f64) -> f64 {
    if w < SMALL {
        (-w).mul_add(LN_2 / 2.0, 1.0)
    } else {
        -exp2m1(-w) / (w * LN_2)
    }
}

/// The increment over a piece of `length` seconds on which the pitch moves
/// linearly, from `pitch` octaves at `slope` octaves a second, times `ratio`
/// (MATH §2.5): r·∫₀ʰ 2^(p + bτ) dτ = r·h·2^p·E(bh), E(z) = (2^z − 1)/(z ln 2),
/// in closed form, which keeps its accuracy however slowly the pitch moves; the
/// pitch held at 1023 octaves, the piece split where it crosses them.
///
/// E is taken as 2^(max(z, 0))·F(|z|), F the mean of 2⁻ˣ from 0 to |z|, so that
/// nothing overflows on the way: z = bh is formed exactly as a double-double
/// (two-product), its low part z′ entering 2^z as 1 + z′ ln 2 where the pitch
/// rises, rounded once, u/2, and moving F by u/2 at most; p and max(z, 0) are
/// split
/// exactly into integers and parts within 1/2, the integers summed exactly
/// and the parts' sum rounded once, 0.69u of the result; and the product,
/// r·h·F·2^(parts)·(1 + z′ ln 2), four roundings, is scaled by 2 to the sum
/// of the integers exactly in the normal range. With F's 4.24u and exp2's
/// 1.86u, the increment is within 12u of the exact relatively, where it and
/// r·h·F lie in the normal range, |p| and |bh| below 2⁵², and r·h below
/// binary64's largest by a factor of 4. Where the pitch crosses 1023 octaves
/// within the piece, the instant τ of the crossing rounds within 2u of
/// itself, the integrand being continuous there, and the two parts' sum
/// once: within 13u + 8u²(w² + w) then, w = |b|τ ln 2.
#[must_use]
pub fn linear_increment(ratio: f64, pitch: f64, slope: f64, length: f64) -> f64 {
    let end = slope.mul_add(length, pitch);
    if pitch <= HIGHEST_PITCH && end <= HIGHEST_PITCH {
        return closed(ratio, pitch, slope, length);
    }
    if pitch >= HIGHEST_PITCH && end >= HIGHEST_PITCH {
        return scaled(ratio * length, HIGHEST_PITCH);
    }
    let crossing = (HIGHEST_PITCH - pitch) / slope;
    let sum = if slope > 0.0 {
        closed(ratio, pitch, slope, crossing) + scaled(ratio * (length - crossing), HIGHEST_PITCH)
    } else {
        scaled(ratio * crossing, HIGHEST_PITCH) + closed(ratio, HIGHEST_PITCH, slope, length - crossing)
    };
    finite(sum)
}

/// The closed form of [`linear_increment`] where the pitch stays at or below
/// 1023 octaves.
fn closed(ratio: f64, pitch: f64, slope: f64, length: f64) -> f64 {
    let (rise, rise_lo) = two_product(slope, length);
    let mean = mean_of_falling(rise.abs());
    let correction = if rise > 0.0 { LN_2.mul_add(rise_lo, 1.0) } else { 1.0 };
    let (whole, part) = split(pitch);
    let (rise_whole, rise_part) = split(rise.max(0.0));
    let product = ratio * length * mean * exp2(part + rise_part) * correction;
    scaled(product, whole + rise_whole)
}

/// √(3/5), correctly rounded: the outer nodes of the 3-point Gauss–Legendre
/// rule lie at ±√(3/5) of the half-interval.
const NODE: f64 = f64::from_bits(0x3fe8_c97e_f43f_7248); // 0.7745966692414834

/// The three instants of a piece of `length` seconds, from its start, at which
/// [`gauss_increment`] takes the pitch: h/2·(1 − √(3/5)), h/2 and
/// h/2·(1 + √(3/5)). 1 − √(3/5) is exact (Sterbenz) and h/2 too; with the
/// constant's rounding, u/2 of it, and those of 1 + √(3/5) and of the products,
/// each instant is within 2u of the exact one relatively.
#[must_use]
pub fn gauss_instants(length: f64) -> [f64; 3] {
    let half = length / 2.0;
    [half * (1.0 - NODE), half, half * (1.0 + NODE)]
}

/// The increment over a piece of `length` seconds on which the pitch moves
/// smoothly, as a sine LFO moves it, times `ratio`, by the 3-point
/// Gauss–Legendre rule (MATH §2.5): r·h·(5·2^p₁ + 8·2^p₂ + 5·2^p₃)/18, from the
/// pitches at the [`gauss_instants`], each held at 1023 octaves.
///
/// The rule integrates a polynomial of degree 5 exactly, and errs by
/// h⁷/2016000 times a sixth derivative of the integrand at some instant of the
/// piece; it is for pieces short enough beside the pitch's movement that this
/// is far below binary64's resolution (MATH §16). Of the rounding: the pitches
/// are taken relative to the integer nearest the highest, each within
/// 0.37u of the sum at most, the integer's difference being exact where the
/// pitches lie within a factor of 2 of it and rounding a little otherwise;
/// exp2 within 1.86u; the sum of the three, every term positive, three
/// roundings, the outer terms' sum, the fused step and the division; the
/// products by r and h two; and the scaling exact in the normal range: within
/// 8u of the rule's exact value relatively, where the increment and r·h times
/// the weighed mean lie in the normal range, and r·h below binary64's largest
/// by a factor of 2.
#[must_use]
pub fn gauss_increment(ratio: f64, length: f64, pitches: [f64; 3]) -> f64 {
    let [first, middle, last] = pitches.map(|pitch| pitch.min(HIGHEST_PITCH));
    let (whole, _) = split(first.max(middle).max(last));
    let term = |pitch: f64| exp2(pitch - whole);
    let mean = 5.0_f64.mul_add(term(first) + term(last), 8.0 * term(middle)) / 18.0;
    scaled(ratio * length * mean, whole)
}

/// How a phase adds its increments (MATH §2.5, `osc.phase_accumulator`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Accumulator {
    /// One rounding a sample: within N·2⁻⁵³ turns of the exact phase after N
    /// samples of increments below 1 turn at most, and about √N·2⁻⁵³ as the
    /// roundings fall.
    Plain,
    /// Each rounding kept beside the phase, by two-sums: within 2⁻⁵³ turns of
    /// the exact phase, and N·2⁻¹⁰⁵ more after N samples at most.
    Compensated,
}

/// An operator's phase in turns, in [0, 1) (MATH §2.5), with the low part that
/// [`Accumulator::Compensated`] keeps, which is 0 under `Plain`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Phase {
    /// The phase, in [0, 1).
    turns: f64,
    /// What the compensated sum has rounded away, a few steps of 2⁻⁵³ at most.
    low: f64,
}

impl Phase {
    /// The phase 0, that of a key-on with OSC KEY SYNC on (MATH §2.5).
    pub const ZERO: Self = Self { turns: 0.0, low: 0.0 };

    /// A phase of so many turns, taken modulo 1: the nearest binary64 of
    /// [0, 1) on the circle, 1 itself being 0.
    #[must_use]
    pub fn new(turns: f64) -> Self {
        Self {
            turns: wrapped(turns - turns.floor()),
            low: 0.0,
        }
    }

    /// The phase in turns, in [0, 1).
    #[must_use]
    pub fn turns(self) -> f64 {
        self.turns
    }

    /// The phase after an increment of so many turns, the whole turns taken
    /// away: θ ← θ + Δ, then θ ← θ − ⌊θ⌋ (MATH §2.5).
    ///
    /// Under [`Accumulator::Plain`] θ + Δ rounds once, within (1 + |Δ|)·2⁻⁵³
    /// turns, and the whole turns go exactly where θ + Δ is at least 0
    /// (Sterbenz) and with one more rounding of 2⁻⁵⁴ below; a sum that rounds
    /// to 1 is 0, the nearer on the circle. Under `Compensated`, θ + Δ, the
    /// whole turns' removal and the low part are summed by two-sums, which lose
    /// only the roundings of the small parts' sums, of the order of 2⁻¹⁰⁶ a
    /// sample, and the low part is folded into the phase as far as binary64
    /// holds it. Where |Δ| reaches 2⁵² turns, a whole number of turns at
    /// binary64's precision, `Plain`'s sum rounds to an integer and the phase
    /// becomes 0, and `Compensated` keeps the phase as it was, exactly.
    pub fn advance(&mut self, increment: f64, accumulator: Accumulator) {
        match accumulator {
            Accumulator::Plain => {
                let sum = self.turns + increment;
                self.turns = wrapped(sum - sum.floor());
            }
            Accumulator::Compensated => {
                let (sum, error) = two_sum(self.turns, increment);
                let whole = sum.floor();
                let (rest, removal) = two_sum(sum, -whole);
                let (mut turns, mut low) = two_sum(rest, (error + removal) + self.low);
                if turns < 0.0 {
                    let (up, up_error) = two_sum(turns, 1.0);
                    turns = up;
                    low += up_error;
                }
                if turns >= 1.0 {
                    let (down, down_error) = two_sum(turns, -1.0);
                    turns = down;
                    low += down_error;
                }
                *self = Self { turns, low };
            }
        }
    }
}

/// A phase in [0, 1]: 1, which a rounding reaches from just below it, is 0.
fn wrapped(turns: f64) -> f64 {
    if turns >= 1.0 { 0.0 } else { turns }
}
