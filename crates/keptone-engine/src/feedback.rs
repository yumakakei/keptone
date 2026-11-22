// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The feedback of docs/MATH.md §9 for an operator whose own output feeds back
//! to it: a step of FB's scale κ, the loop gain β, and the operator's output by
//! the delay-free point solution of Kepler's equation where |β| lies below 1
//! (§9.4), or by the models of §9.6 that `feedback.model` names: relaxation,
//! which lags the fed-back signal and integrates the lag by the trapezoidal
//! rule in substeps; dx7-discrete, the DX7's recurrence at the engine rate;
//! and clamp, the point solution with |β| held below 1. Each is a function of
//! its inputs at an engine sample and, for the relaxation, at the instants of
//! its substeps, which the voice gives (MATH §0.3). Loops through several
//! operators (§9.7) are the voice's, which composes them.
//!
//! Each function documents its error against the law's exact value at its
//! binary64 inputs; tests/feedback.rs measures each against exact values
//! (`scripts/feedback_references.py`).

use std::f64::consts::TAU;

use keptone_math::kepler;
use keptone_math::{cos2pi, sin2pi};

use crate::binary64::{finite, scaled};
use crate::steps::between;

/// 2⁻⁷, κ at FB 1, the slope of its first step.
const FIRST_STEP: f64 = 1.0 / 128.0;

/// u = 2⁻⁵³, the unit roundoff of binary64.
const U: f64 = f64::EPSILON / 2.0;

/// 1/(2π): where |Θ| = 1 in turns, below which the stopping rule is absolute.
const ONE_RADIAN: f64 = 1.0 / TAU;

/// 2⁻⁵⁰⁰, below which a state is flushed to 0 (MATH §13.4).
const TINY: f64 = f64::from_bits((1023 - 500) << 52);

/// κ of MATH §9.1 in turns per unit of output at a FB: 2^(FB − 8) at FB 1 to 7,
/// and at the whole FBs above as that formula gives them; linear between
/// whole FBs (§0.6), from 0 at FB 0; and along its first step below 0, where κ
/// and the feedback are inverted. FB exactly 0 has no feedback edge (§8.1),
/// which the graph says.
///
/// Exact at whole FBs, binary64's largest finite number where 2^(FB − 8) lies
/// beyond it, and below 0, where it is FB·2⁻⁷, but where it is subnormal;
/// between whole FBs the steps and their difference are exact, and the fused
/// step rounds within u|κ|.
#[must_use]
pub fn kappa(fb: f64) -> f64 {
    if fb < 0.0 {
        return fb * FIRST_STEP;
    }
    between(fb, |whole| if whole <= 0.0 { 0.0 } else { scaled(1.0, whole - 8.0) })
}

/// β of MATH §9.1 in radians, the loop gain 2πκg for κ and the operator's
/// gain g, which moves with its envelope during a note: within 3u relatively,
/// 2π's rounding and the two products'.
#[must_use]
pub fn beta(kappa: f64, gain: f64) -> f64 {
    finite(TAU * kappa * gain)
}

/// The operator's output y = g sin Θ of MATH §9.4, Θ − β sin Θ = Φ, at its
/// phase θ = Φ/(2π) in turns, its gain g and a loop gain β of magnitude below
/// 1, by Kepler's equation's solver (`keptone_math::kepler`), stopped after
/// `feedback.max_iterations`: a negative β is solved half a turn away,
/// Θ = Ψ + π with Ψ − |β| sin Ψ = Φ − π, so that y = −g sin Ψ.
///
/// The solver's residual is within 4u·max(1, |Φ|) of 0, Φ reduced to a half
/// turn, so that Θ lies within 4u·max(1, |Φ|)/(1 − |β| cos Θ) of the root; φ −
/// 1/2 for a negative β within u/2 of itself, a turn's half-ulp; sin2pi within
/// 1.46u relatively and the product with g within u: y within
/// g·(|cos Θ|·δΘ + 1.46u|sin Θ|) + u|y|.
#[must_use]
pub fn point(phase: f64, gain: f64, beta: f64, max_iterations: u32) -> f64 {
    if beta < 0.0 {
        let solution = kepler::solve(phase - 0.5, -beta, max_iterations);
        finite(-gain * sin2pi(solution.turns))
    } else {
        finite(gain * sin2pi(kepler::solve(phase, beta, max_iterations).turns))
    }
}

/// `feedback.model = clamp` (MATH §9.6): [`point`] with β's magnitude at most
/// `feedback.beta_clamp`, at every β.
#[must_use]
pub fn clamped(phase: f64, gain: f64, beta: f64, beta_clamp: f64, max_iterations: u32) -> f64 {
    point(phase, gain, beta.clamp(-beta_clamp, beta_clamp), max_iterations)
}

/// `feedback.model = dx7-discrete` (MATH §9.6), the DX7's recurrence of §9.1 at
/// the engine rate, at every β: y\[n\] = g sin(2π(θ\[n\] + κ(y\[n−1\] + y\[n−2\])/2)),
/// from the operator's two outputs before, both 0 for a new voice.
///
/// The mean of the two is within u of itself, θ + κ·mean in a fused step within
/// u|θ + κ·mean| turns, sin2pi within 1.46u and the product within u: y within
/// g·2π|cos|·u(|κ·mean| + |θ + κ·mean|) + 2.46u|y| of its exact value at the
/// binary64 outputs before.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Discrete {
    /// y\[n − 1\].
    previous: f64,
    /// y\[n − 2\].
    before: f64,
}

impl Discrete {
    /// A new voice's: both outputs before 0.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            previous: 0.0,
            before: 0.0,
        }
    }

    /// The output at an engine sample at its phase θ in turns, its gain and κ,
    /// which becomes the output before the next.
    pub fn output(&mut self, phase: f64, gain: f64, kappa: f64) -> f64 {
        let mean = f64::midpoint(self.previous, self.before);
        let output = finite(gain * sin2pi(kappa.mul_add(mean, phase)));
        self.before = self.previous;
        self.previous = output;
        output
    }

    /// Sets each output before whose magnitude lies below 2⁻⁵⁰⁰ to 0, as MATH
    /// §13.4 does at every 64th engine sample.
    pub fn flush(&mut self) {
        for state in [&mut self.previous, &mut self.before] {
            if state.abs() < TINY {
                *state = 0.0;
            }
        }
    }
}

/// τ of MATH §9.6's relaxation at a loop gain β, in seconds: 0 up to
/// |β| = `feedback.relaxation_onset`, where the operator is [`point`]'s,
/// rising linearly to `feedback.tau_s` at |β| = 1, and staying there beyond.
/// Within 2u of τ relatively where the onset lies from 1/2 on, its two
/// differences being exact there, and within 4u elsewhere.
#[must_use]
pub fn lag(beta: f64, onset: f64, tau: f64) -> f64 {
    let magnitude = beta.abs();
    if magnitude <= onset {
        0.0
    } else if magnitude >= 1.0 {
        tau
    } else {
        finite(tau * ((magnitude - onset) / (1.0 - onset)))
    }
}

/// The substeps of an engine sample for the relaxation (MATH §9.6): the fewest
/// equal ones each shorter than 2τ/(β − 1) at the sample's largest |β|, one
/// where it is at most 1, where every step's root is unique, and at most
/// `feedback.max_substeps`, beyond which Newton from the previous value picks
/// the root. (β − 1)/(2τfₑ) rounds, which can take the count one off where it
/// lies within its rounding of a whole number.
#[must_use]
pub fn substeps(beta_max: f64, tau: f64, sample_rate: f64, max_substeps: u32) -> u32 {
    let excess = beta_max.abs() - 1.0;
    if excess <= 0.0 || tau <= 0.0 {
        return 1;
    }
    let needed = (excess / (2.0 * tau * sample_rate)).floor() + 1.0;
    if needed >= f64::from(max_substeps) {
        max_substeps
    } else {
        // needed lies below max_substeps, a u32.
        whole_count(needed)
    }
}

/// A whole binary64 of [1, 2³²) as a count.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole binary64 of [1, 2³²) converts exactly"
)]
fn whole_count(value: f64) -> u32 {
    value as u32
}

/// x − round(x), exactly: a phase in turns reduced to a half turn.
fn reduced(turns: f64) -> f64 {
    turns - turns.round_ties_even()
}

/// The relaxation of MATH §9.6, `feedback.model = relaxation`: the fed-back
/// signal lagged, τẎ = −Y + y with y = g sin(2π(θ + κY)), the operator's
/// output taking the lagged Y in place of itself; Y is 0 for a new voice.
///
/// Each substep of h = 1/(m·fₑ) solves the trapezoidal rule's equation
/// F(Y) = (1 + a)Y − (1 − a)Yₖ − a·yₖ − a·g sin(2π(θ + κY)) = 0, a = h/(2τ),
/// by Newton from Yₖ inside its bracket, Y within a·g/(1 + a) of
/// ((1 − a)Yₖ + a·yₖ)/(1 + a), bisecting where a step leaves it, and stopping
/// where a step moves θ + κY by at most 2u·max(1/(2π), |θ + κY|) turns, θ + κY
/// reduced to a half turn, or after `feedback.max_iterations`. F's evaluation
/// rounds within u(3|(1 + a)Y| + 4|(1 − a)Yₖ| + 5.5a|yₖ| + 3.5a·g), yₖ's own
/// rounding among them, each sine's argument within u|θ + κY| turns, and a,
/// within 3u of itself, moves it by 3u|Y − Yₖ|: the step's Y lies within that
/// over |F′(Y)| = |1 + a − a·β cos 2π(θ + κY)|, and the stopping rule's step,
/// of the root of its equation at its binary64 inputs and Yₖ.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Relaxation {
    /// Y, the lagged fed-back signal, in the unit of the output.
    lagged: f64,
}

impl Relaxation {
    /// A new voice's: Y = 0.
    #[must_use]
    pub const fn new() -> Self {
        Self { lagged: 0.0 }
    }

    /// Y.
    #[must_use]
    pub const fn lagged(&self) -> f64 {
        self.lagged
    }

    /// Advances Y over an engine sample and gives the output at the sample's
    /// instant, y = g sin 2π(θ + κY): in `substeps` equal steps by the
    /// trapezoidal rule, θ and g at the substeps' instants being `at(j)` for j
    /// from 0, the sample before, to `substeps`, the sample's own; with τ = 0,
    /// [`point`]'s output at the sample, to which Y is set.
    pub fn output(
        &mut self,
        at: impl Fn(u32) -> (f64, f64),
        substeps: u32,
        kappa: f64,
        tau: f64,
        sample_rate: f64,
        max_iterations: u32,
    ) -> f64 {
        let steps = substeps.max(1);
        if tau <= 0.0 {
            let (phase, gain) = at(steps);
            let output = point(phase, gain, beta(kappa, gain), max_iterations);
            self.lagged = output;
            return output;
        }
        let a = finite(1.0 / (2.0 * tau * sample_rate * f64::from(steps)));
        let (mut phase, mut gain) = at(0);
        for j in 1..=steps {
            let start = self.lagged;
            let before = finite(gain * sin2pi(kappa.mul_add(start, phase)));
            (phase, gain) = at(j);
            self.lagged = solve(start, before, phase, gain, kappa, a, max_iterations);
        }
        finite(gain * sin2pi(kappa.mul_add(self.lagged, phase)))
    }

    /// Sets Y to 0 where its magnitude lies below 2⁻⁵⁰⁰, as MATH §13.4 does at
    /// every 64th engine sample.
    pub fn flush(&mut self) {
        if self.lagged.abs() < TINY {
            self.lagged = 0.0;
        }
    }
}

/// The root of one trapezoidal substep's equation (see [`Relaxation`]), from
/// the lagged value and the output at its start, θ and g at its end, κ and a.
fn solve(start: f64, before: f64, phase: f64, gain: f64, kappa: f64, a: f64, max_iterations: u32) -> f64 {
    let centre = finite(finite((1.0 - a) * start + a * before) / (1.0 + a));
    let reach = finite(a * gain.abs() / (1.0 + a));
    let (mut low, mut high) = (centre - reach, centre + reach);
    let mut lagged = start.max(low).min(high);
    for _ in 0..max_iterations {
        let turns = kappa.mul_add(lagged, phase);
        let residual =
            finite((1.0 + a).mul_add(lagged, -finite((1.0 - a) * start)) - a * before - a * gain * sin2pi(turns));
        if residual > 0.0 {
            high = lagged;
        } else if residual < 0.0 {
            low = lagged;
        } else {
            break;
        }
        let slope = finite(1.0 + a - a * gain * TAU * kappa * cos2pi(turns));
        let step = -residual / slope;
        let next = lagged + step;
        if (step * kappa).abs() <= 2.0 * U * reduced(turns).abs().max(ONE_RADIAN) {
            return next.max(low).min(high);
        }
        lagged = if low < next && next < high {
            next
        } else {
            low + (high - low) / 2.0
        };
    }
    lagged
}
