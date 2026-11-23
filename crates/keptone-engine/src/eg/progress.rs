// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The progress of an operator envelope's segment over a piece of a sample on
//! which its rates' key moves (docs/MATH.md §4.7): ∫ρ dt as the key kᴿ moves
//! with the pitch envelope and the portamento, split where ρ has a corner as a
//! function of the key, in closed form between the corners, and the instant at
//! which it reaches a target, the progress left in the segment, where the
//! segment ends.
//!
//! A piece is the caller's: a sample split where kᴿ has a corner or a jump
//! (MATH §2.5), on which kᴿ is a line with a glide's law beside it. Between
//! two of ρ's corners a piece's stretch is a part of the budget of
//! `osc.max_pieces`, and under the exponential law with a glide each part of
//! §2.5's rule on it is one.
//!
//! Each function documents its error against the law's exact value at its
//! binary64 inputs; tests/progress.rs measures each against exact values
//! (`scripts/progress_references.py`).

use core::f64::consts::{LN_2, LOG2_E};

use keptone_math::eft::two_sum;
use keptone_math::{exp2, exp2m1, log2, log2p1};

use super::{RateLaw, rate, speed};
use crate::binary64::finite;
use crate::pieces::rule_parts;
use crate::pitch::phase::{gauss_increment, gauss_instants, linear_increment};

/// The whole keys between which q's group of keys moves (MATH §4.1): q holds
/// still below the first and above the last, where the group holds its
/// bounds.
const LOWEST: f64 = 23.0;
const HIGHEST: f64 = 114.0;

/// Below this the mean of e⁻ˣ over [0, x] is 1 − x/2 within x²/6, below u/700.
const SMALL: f64 = 1.0 / 1_073_741_824.0;

/// The most iterations of a bracketed search: each halves its bracket at
/// least, and 1100 halvings take any bracket of binary64 to one of its steps.
const SEARCH: u32 = 1100;

/// The rates' key kᴿ on a piece of a sample (MATH §2.6), τ the samples since
/// its start: k + bτ + G(τ), G a glide's offset from its target in keys,
/// d + (d + 3·sgn d)·(e^(−ρτ) − 1) from its offset d at the start, as a glide
/// in keys moves (MATH §7.3), up to its arrival, where the piece ends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    /// k at the start, the key without the glide's offset.
    pub key: f64,
    /// b, in keys a sample.
    pub slope: f64,
    /// d, the glide's offset in keys at the start; 0 where there is no glide.
    pub glide: f64,
    /// ρ = 4κ/fₑ, the rate of the glide's law a sample.
    pub rate: f64,
}

impl Key {
    /// A key that holds still.
    #[must_use]
    pub const fn still(key: f64) -> Self {
        Self {
            key,
            slope: 0.0,
            glide: 0.0,
            rate: 0.0,
        }
    }

    /// Whether a glide moves it.
    fn glides(&self) -> bool {
        self.glide != 0.0 && self.rate > 0.0
    }

    /// The glide's amplitude d + 3·sgn d, its distance from the glide's
    /// asymptote.
    fn amplitude(&self) -> f64 {
        finite(self.glide + 3.0_f64.copysign(self.glide))
    }

    /// kᴿ at τ samples into the piece.
    ///
    /// k + bτ rounds within u|k + bτ|, the glide's offset within u((|d| +
    /// 3)(2.24|m| + 3x·e^(−x)) + |G|), x = ρτ and m = e^(−x) − 1, as a glide's
    /// offset does (`crate::portamento`), 4.35u(|d| + 3) at most, and the sum
    /// within u|kᴿ|.
    #[must_use]
    pub fn at(&self, samples: f64) -> f64 {
        let line = self.slope.mul_add(samples, self.key);
        if self.glide == 0.0 {
            return line;
        }
        let from = self.glide.abs();
        let falling = exp2m1(-(self.rate * samples) * LOG2_E);
        let offset = (from + 3.0).mul_add(falling, from).max(0.0).copysign(self.glide);
        finite(line + offset)
    }

    /// The instant within (0, L) at which kᴿ′ = b − ρg·e^(−ρτ) vanishes and
    /// kᴿ turns, g its amplitude: e^(−ρτ) = b/(ρg), which only a slope and a
    /// glide of one sign reach, b below ρg; none elsewhere. The quotient, its
    /// logarithm and the product round within a few u, and the stretches on
    /// either side of the turn take its instant as their bound.
    fn turn(&self, length: f64) -> Option<f64> {
        if !self.glides() {
            return None;
        }
        let ratio = self.slope / finite(self.rate * self.amplitude());
        if !(ratio > 0.0 && ratio < 1.0) {
            return None;
        }
        let samples = -log2(ratio) * LN_2 / self.rate;
        (samples > 0.0 && samples < length).then_some(samples)
    }

    /// The glide's amplitude at τ, g·e^(−ρτ): what is left of it from there.
    fn amplitude_at(&self, samples: f64) -> f64 {
        finite(self.amplitude() * exp2(-(self.rate * samples) * LOG2_E))
    }
}

/// A segment's rates (MATH §4.1): its rate R and the operator's rate scaling
/// KRS, with `eg.clock_hz`, `eg.rate_law` and the engine's rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rates {
    /// R, of the segment's stage.
    pub rate: f64,
    /// KRS.
    pub sensitivity: f64,
    /// `eg.clock_hz`.
    pub clock_hz: f64,
    /// `eg.rate_law`.
    pub law: RateLaw,
    /// The engine's rate fₑ.
    pub sample_rate: f64,
}

impl Rates {
    /// q at a key, as [`super::rate`] gives it.
    fn q(&self, key: f64) -> f64 {
        rate(self.rate, self.sensitivity, key)
    }

    /// The progress a sample at a key, ρ(q)/fₑ: [`super::speed`]'s error, 2.5u
    /// relatively under the dx7 law and 2.86u under the exponential, a
    /// quarter of q's error more relatively, the speed's slope in q being at
    /// most a quarter of the speed, and u for the quotient.
    #[must_use]
    pub fn step(&self, key: f64) -> f64 {
        self.step_of(self.q(key))
    }

    /// The progress a sample at a q.
    fn step_of(&self, q: f64) -> f64 {
        speed(q, self.clock_hz, self.law) / self.sample_rate
    }

    /// Whether q's slope in the key changes at a whole key: its differences on
    /// either side differ.
    fn bends(&self, whole: f64) -> bool {
        let (below, at, above) = (self.q(whole - 1.0), self.q(whole), self.q(whole + 1.0));
        finite(finite(above - at) - finite(at - below)) != 0.0
    }

    /// The whole m whose 4m are corners of ρ under the dx7 law where ρ is
    /// finite and above 0 around them: from −1077 − log₂ C to 1025 − log₂ C, C
    /// = `eg.clock_hz`/4096, a step beyond the speeds that binary64 holds on
    /// either side, where ρ holds still at 0 or at its largest finite number.
    fn multiples(&self) -> (f64, f64) {
        let log = log2(self.clock_hz / 4096.0);
        ((-1077.0 - log).ceil(), (1025.0 - log).floor())
    }
}

/// The first key strictly beyond `start` toward `to`, on the ramp from the
/// whole key `ramp` to the next, at which q crosses a multiple of 4 (MATH
/// §4.7), where ρ is finite and above 0: 4m's key on the ramp, ramp + (4m −
/// q(ramp))/(q(ramp + 1) − q(ramp)), q being linear on it; none where q holds
/// still on it or crosses none before `to`.
fn quarter(rates: &Rates, ramp: f64, start: f64, to: f64) -> Option<f64> {
    let (low, high) = (rates.q(ramp), rates.q(ramp + 1.0));
    let slope = finite(high - low);
    if slope == 0.0 {
        return None;
    }
    let up = to > start;
    let end = if up { to.min(ramp + 1.0) } else { to.max(ramp) };
    let (from_q, to_q) = (rates.q(start), rates.q(end));
    let rising = to_q > from_q;
    let (least, most) = rates.multiples();
    let mut m = if rising {
        ((from_q / 4.0).floor() + 1.0).max(least)
    } else {
        ((from_q / 4.0).ceil() - 1.0).min(most)
    };
    // A start at a crossing that rounds before it takes the next multiple.
    for _ in 0..2 {
        if !(least..=most).contains(&m) || (rising && 4.0 * m >= to_q) || (!rising && 4.0 * m <= to_q) {
            return None;
        }
        let key = finite(ramp + finite(4.0 * m - low) / slope);
        if (up && key > start && key < to) || (!up && key < start && key > to) {
            return Some(key);
        }
        m += if rising { 1.0 } else { -1.0 };
    }
    None
}

/// The first key strictly beyond `from` toward `to`, strictly before `to`, at
/// which ρ has a corner as a function of the key (MATH §4.7): a whole key
/// from 23 to 114 at which q's slope changes, and, under the dx7 law, a key
/// at which q crosses a multiple of 4 while ρ is finite and above 0 around it;
/// none where there is none. q holds still below 23 and above 114, and a scan
/// passes at most the 92 ramps between them.
fn knot(rates: &Rates, from: f64, to: f64) -> Option<f64> {
    if to - from == 0.0 {
        return None;
    }
    let up = to > from;
    let (low, high) = if up { (from, to) } else { (to, from) };
    if high <= LOWEST || low >= HIGHEST {
        return None;
    }
    let quarters = rates.law == RateLaw::Dx7;
    let (mut start, mut ramp) = if up {
        if from < LOWEST - 1.0 {
            (LOWEST - 1.0, LOWEST - 1.0)
        } else {
            (from, from.floor())
        }
    } else if from > HIGHEST + 1.0 {
        (HIGHEST + 1.0, HIGHEST)
    } else {
        (from, from.ceil() - 1.0)
    };
    loop {
        if quarters
            && (LOWEST..HIGHEST).contains(&ramp)
            && let Some(key) = quarter(rates, ramp, start, to)
        {
            return Some(key);
        }
        let whole = if up { ramp + 1.0 } else { ramp };
        if (up && whole >= to) || (!up && whole <= to) {
            return None;
        }
        if (LOWEST..=HIGHEST).contains(&whole) && rates.bends(whole) {
            return Some(whole);
        }
        if (up && whole >= HIGHEST) || (!up && whole <= LOWEST) {
            return None;
        }
        start = whole;
        ramp += if up { 1.0 } else { -1.0 };
    }
}

/// The instant within [a, b] at which a monotone kᴿ reaches a key between its
/// values there: (K − kᴿ(0))/b where kᴿ is a line, and by Newton's
/// iteration within the bracket [a, b] where a glide moves it, bisecting where
/// a step leaves the bracket, until the bracket is one of binary64's steps or
/// kᴿ there rounds to the key. The line's instant rounds within u of the
/// difference and u of the quotient; the glide's lies within the bracket at
/// its end, where kᴿ lies within its own error of the key.
fn crossing(key: &Key, target: f64, a: f64, b: f64) -> f64 {
    if !key.glides() {
        let offset = if key.glide == 0.0 { 0.0 } else { key.glide };
        return (finite(target - finite(key.key + offset)) / key.slope).clamp(a, b);
    }
    let rising = key.at(b) > key.at(a);
    let (mut low, mut high) = (a, b);
    let mut samples = f64::midpoint(a, b);
    for _ in 0..SEARCH {
        let error = key.at(samples) - target;
        if error == 0.0 {
            return samples;
        }
        if (error < 0.0) == rising {
            low = samples;
        } else {
            high = samples;
        }
        let middle = f64::midpoint(low, high);
        if middle <= low || middle >= high {
            break;
        }
        let slope = finite(key.slope - finite(key.rate * key.amplitude_at(samples)));
        let next = samples - error / slope;
        samples = if next > low && next < high { next } else { middle };
    }
    f64::midpoint(low, high)
}

/// The mean of e^(−σ) over σ from 0 to x ≥ 0, F(x) = (1 − e⁻ˣ)/x, 1 at 0:
/// below 2⁻³⁰ 1 − x/2, within 1.002u, and above −exp2m1(−x log₂ e)/x, within
/// 4.24u relatively.
fn mean_of_falling(x: f64) -> f64 {
    if x < SMALL {
        (-0.5_f64).mul_add(x, 1.0)
    } else {
        -exp2m1(-x * LOG2_E) / x
    }
}

/// H(x) = (x − 1 + e⁻ˣ)/x² = (1 − F(x))/x for x ≥ 0, 1/2 at 0: below 1/2 its
/// series Σ (−x)ⁿ/(n + 2)! to n = 17, whose truncation lies below u/2⁴⁰ and
/// whose roundings, Horner's, within 2u relatively; above, (x + exp2m1(−x
/// log₂ e))/x², whose difference loses at most 3 bits of exp2m1's 0.62 ulp:
/// within 8u relatively.
fn second_mean(x: f64) -> f64 {
    if x < 0.5 {
        let mut sum = 0.0;
        let mut factorial = 1.0;
        let mut terms = [0.0; 18];
        for (n, term) in (2_u32..).zip(terms.iter_mut()) {
            factorial *= f64::from(n);
            *term = 1.0 / factorial;
        }
        for term in terms.iter().rev() {
            sum = (-x).mul_add(sum, *term);
        }
        sum
    } else {
        (x + exp2m1(-x * LOG2_E)) / (x * x)
    }
}

/// The mean place of kᴿ on a stretch [a, b] where a glide moves it, the
/// share m of its rise at which ρ is taken on average: ∫(kᴿ − kᴿ(a)) dτ
/// over the stretch divided by the stretch's length and kᴿ's change on it,
/// (b/2 − g₀ρH(x))/(b − g₀ρF(x)), x = ρ(b − a) and g₀ the glide's
/// amplitude at a, 1/2 without a glide; held from 0 to 1, as kᴿ is monotone
/// on the stretch, and 1/2 where kᴿ does not change. Each factor within a
/// few u relatively: m within 16u.
fn mean_place(key: &Key, a: f64, b: f64) -> f64 {
    if !key.glides() {
        return 0.5;
    }
    let x = key.rate * (b - a);
    let left = finite(key.rate * key.amplitude_at(a));
    let above = (-left).mul_add(second_mean(x), key.slope / 2.0);
    let below = (-left).mul_add(mean_of_falling(x), key.slope);
    let mean = above / below;
    if mean.is_finite() { mean.clamp(0.0, 1.0) } else { 0.5 }
}

/// Where a piece's progress reaches a target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reach {
    /// At so many samples into the piece.
    Within(f64),
    /// Not within the piece, whose whole progress is this, below the target.
    Beyond(f64),
}

/// A walk over a piece: its rates and key, its progress so far as a sum and
/// what the sums have rounded away, and the budget of parts it spends.
struct Walk<'a> {
    rates: &'a Rates,
    key: &'a Key,
    high: f64,
    low: f64,
    budget: &'a mut f64,
}

impl Walk<'_> {
    /// The bounds of the derivatives of ln ρ(kᴿ(τ)) on a stretch under the
    /// exponential law, where ln ρ is ln 2·q′·kᴿ/4 and its constant: the
    /// line's slope and the glide's ρᵏ·|g₀|, times ln 2·|q′|/4.
    fn bounds(&self, a: f64, slope: f64) -> [f64; 6] {
        let factor = finite(LN_2 * slope.abs() / 4.0);
        let mut bounds = [0.0; 6];
        let mut term = self.key.amplitude_at(a).abs();
        for bound in &mut bounds {
            term = finite(term * self.key.rate);
            *bound = term;
        }
        bounds[0] = finite(bounds[0] + self.key.slope.abs());
        bounds.map(|bound| finite(bound * factor))
    }

    /// q's slope in the key on a stretch between ρ's corners: q's difference
    /// across the ramp that holds the stretch's middle key, 0 outside 23 to
    /// 114, where q holds still.
    fn q_slope(&self, key_a: f64, key_b: f64) -> f64 {
        let middle = f64::midpoint(key_a, key_b);
        if !(LOWEST..HIGHEST).contains(&middle) {
            return 0.0;
        }
        let whole = middle.floor();
        finite(self.rates.q(whole + 1.0) - self.rates.q(whole))
    }

    /// The progress over the stretch [a, b] of the piece, on which ρ has no
    /// corner, from kᴿ's values at its ends, and the rule's parts that it
    /// takes, 1 where it is in closed form.
    ///
    /// Under the dx7 law ρ is linear in kᴿ there, and the progress is
    /// (b − a)·(ρ₀ + m·(ρ₁ − ρ₀)), m kᴿ's mean place ([`mean_place`]),
    /// 1/2 for a line: the trapezoid's. Under the exponential law it is
    /// ρ₀∫2^((q₁ − q₀)τ/(4(b − a))) dτ where kᴿ is a line, in closed form
    /// (`linear_increment`), and with a glide the rule on its parts at
    /// q′(kᴿ − kᴿ(a))/4 octaves of ρ.
    fn integral(&self, a: f64, b: f64, key_a: f64, key_b: f64) -> (f64, f64) {
        let length = b - a;
        let (rho_a, q_a) = {
            let q = self.rates.q(key_a);
            (self.rates.step_of(q), q)
        };
        match self.rates.law {
            RateLaw::Dx7 => {
                let rho_b = self.rates.step(key_b);
                let mean = mean_place(self.key, a, b);
                (finite(mean.mul_add(finite(rho_b - rho_a), rho_a) * length), 1.0)
            }
            RateLaw::Exponential if !self.key.glides() => {
                let q_b = self.rates.q(key_b);
                let slope = finite(q_b - q_a) / 4.0 / length;
                (linear_increment(rho_a, 0.0, slope, length), 1.0)
            }
            RateLaw::Exponential => {
                let q_slope = self.q_slope(key_a, key_b);
                let parts = rule_parts(self.bounds(a, q_slope), length);
                if parts > *self.budget {
                    return (0.0, parts);
                }
                (self.rule(a, b, key_a, q_slope, parts), parts)
            }
        }
    }

    /// The rule on `parts` equal parts of [a, b], ρ₀·2^(q′(kᴿ − kᴿ(a))/4)
    /// at its instants, the parts' boundaries a + (b − a)j/n and their
    /// differences, which tile the stretch exactly, as a piece's parts do
    /// (`crate::pieces`).
    fn rule(&self, a: f64, b: f64, key_a: f64, q_slope: f64, parts: f64) -> f64 {
        let length = b - a;
        let rho_a = self.rates.step(key_a);
        let count = whole(parts);
        let boundary = |index: u32| {
            if index >= count {
                length
            } else {
                length * f64::from(index) / f64::from(count)
            }
        };
        let (mut high, mut low) = (0.0, 0.0);
        for index in 0..count {
            let start = boundary(index);
            let part = boundary(index + 1) - start;
            let exponents =
                gauss_instants(part).map(|instant| q_slope * finite(self.key.at(a + start + instant) - key_a) / 4.0);
            let (sum, error) = two_sum(high, gauss_increment(rho_a, part, exponents));
            high = sum;
            low += error;
        }
        finite(high + low)
    }

    /// The samples into the stretch [a, b] at which its progress reaches a
    /// target from 0 to the stretch's whole: a quadratic's root where ρ is
    /// linear in time, 2t/(ρ₀ + √(ρ₀² + 2ρ′t)), a logarithm where it is
    /// exponential, (t/ρ₀)·ln(1 + y)/y with y = t·c·ln 2/ρ₀, and elsewhere
    /// Newton's iteration within the bracket [0, b − a] on the stretch's
    /// progress, bisecting where a step leaves it, until the bracket is one
    /// of binary64's steps or the progress rounds to the target.
    fn solve(&self, a: f64, b: f64, key_a: f64, key_b: f64, target: f64) -> f64 {
        let length = b - a;
        if target <= 0.0 {
            return 0.0;
        }
        let rho_a = self.rates.step(key_a);
        if !self.key.glides() {
            let root = match self.rates.law {
                RateLaw::Dx7 => {
                    let slope = finite(self.rates.step(key_b) - rho_a) / length;
                    let discriminant = finite(rho_a * rho_a + 2.0 * finite(slope * target)).max(0.0);
                    2.0 * target / (rho_a + discriminant.sqrt())
                }
                RateLaw::Exponential => {
                    let c = finite(self.rates.q(key_b) - self.rates.q(key_a)) / 4.0 / length;
                    let y = finite(target * c * LN_2) / rho_a;
                    let share = if y.abs() < SMALL {
                        (-0.5_f64).mul_add(y, 1.0)
                    } else {
                        log2p1(y) * LN_2 / y
                    };
                    target / rho_a * share
                }
            };
            return if root.is_finite() {
                root.clamp(0.0, length)
            } else {
                length
            };
        }
        let (mut low, mut high) = (0.0, length);
        let mut samples = (target / rho_a).clamp(0.0, length);
        for _ in 0..SEARCH {
            let at = a + samples;
            let (progress, _) = self.integral(a, at, key_a, self.key.at(at));
            let error = progress - target;
            if error == 0.0 {
                return samples;
            }
            if error < 0.0 {
                low = samples;
            } else {
                high = samples;
            }
            let middle = f64::midpoint(low, high);
            if middle <= low || middle >= high {
                break;
            }
            let next = samples - error / self.rates.step(self.key.at(at));
            samples = if next > low && next < high { next } else { middle };
        }
        f64::midpoint(low, high)
    }
}

/// A whole count of parts of at least 1 as a u32, at most `osc.max_pieces`.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole binary64 of 1 to u32::MAX converts exactly"
)]
fn whole(parts: f64) -> u32 {
    parts as u32
}

/// The progress of a segment over a piece of `length` samples on which its
/// rates' key moves as `key` says (MATH §4.7): ∫ρ(kᴿ(τ))/fₑ dτ over the
/// piece, or, given a target, the progress left in the segment, the instant
/// at which the progress reaches it, where the segment ends; none where the
/// piece would take more parts than the budget leaves, which it spends, each
/// stretch between two of ρ's corners one, and under the exponential law with
/// a glide each of the rule's parts on it one.
///
/// The piece is split at the instant at which a glide turns kᴿ ([`Key`]),
/// and each monotone stretch at the keys where ρ has a corner (MATH §4.7):
/// whole keys from 23 to 114 at which q's slope changes, and under the dx7 law
/// q's multiples of 4 while ρ is finite and above 0. The corners' instants
/// round within u of themselves, and as ρ is continuous there, a stretch's
/// bound moved by δ changes the progress by the second order of δ alone. On a
/// stretch, ρ at its ends lies within ε = 3.5u + δq/4 of itself relatively,
/// δq q's error ([`super::rate`]), and the progress within 2u + ε more of
/// the stretch's law at them, a line's trapezoid, and 16u·|ρ₁ − ρ₀|(b − a)
/// more where a glide moves kᴿ; the closed form of the exponential law within
/// 12u, and the rule within 8u and its truncation's 2⁻⁶⁰; and their sum,
/// kept by two-sums, within u of itself. A target's instant lies within the
/// progress's error over ρ there, beside the roundings of the root's formula,
/// a few u of it, or the search's bracket.
pub fn advance(rates: &Rates, key: &Key, length: f64, target: Option<f64>, budget: &mut f64) -> Option<Reach> {
    let mut walk = Walk {
        rates,
        key,
        high: 0.0,
        low: 0.0,
        budget,
    };
    if length.is_nan() || length <= 0.0 {
        return Some(Reach::Beyond(0.0));
    }
    let turn = key.turn(length);
    let halves = [(0.0, turn.unwrap_or(length)), (turn.unwrap_or(length), length)];
    for (a, b) in halves {
        if b <= a {
            continue;
        }
        let (mut start, mut key_start) = (a, key.at(a));
        let key_end = key.at(b);
        loop {
            let corner = knot(rates, key_start, key_end);
            let (end, key_stop) = match corner {
                Some(corner) => (crossing(key, corner, start, b), corner),
                None => (b, key_end),
            };
            if end > start {
                if *walk.budget < 1.0 {
                    return None;
                }
                let (progress, parts) = walk.integral(start, end, key_start, key_stop);
                if parts > *walk.budget {
                    return None;
                }
                *walk.budget -= parts;
                let done = finite(walk.high + walk.low);
                if let Some(target) = target
                    && finite(done + progress) >= target
                {
                    let left = finite(target - done).max(0.0);
                    let samples = walk.solve(start, end, key_start, key_stop, left);
                    return Some(Reach::Within(finite(start + samples).min(length)));
                }
                let (sum, error) = two_sum(walk.high, progress);
                walk.high = sum;
                walk.low += error;
            }
            if corner.is_none() {
                break;
            }
            start = end;
            key_start = key_stop;
        }
    }
    Some(Reach::Beyond(finite(walk.high + walk.low)))
}
