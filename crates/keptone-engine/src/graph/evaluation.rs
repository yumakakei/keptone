// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! A voice's operators at an engine sample (docs/MATH.md §1, §8.2, §9): its
//! graph's strongly connected components in their order of evaluation, an
//! operator on no loop by its point sample y = g sin 2π(θ + μ), as milestone
//! M3 computes no antiderivative antialiasing (§10), and a self-loop and a
//! loop through several operators by the model that `feedback.model` names
//! (§9.6, §9.7); and the sum of the carriers' outputs.
//!
//! A sample is evaluated in two steps. [`Plan::integrate`] takes the lags of
//! the relaxation over the sample, by the laws that held over it, θ and g at
//! the instants of its substeps; then the sample's events act (MATH §0.3);
//! and [`Plan::output`] gives the operators' outputs at the sample's instant,
//! θ, g and κ taken there. What the feedback keeps from sample to sample is
//! a [`Memory`], which a voice takes when the engine is prepared; neither
//! step allocates.
//!
//! An operator's μ adds its modulators' outputs in the ascending order of
//! their operators, and the carriers' sum adds theirs in that order too, so
//! that each sum's order is fixed (MATH §8.2); a feedback edge's term is κ
//! times the value that its model feeds back. tests/evaluation.rs measures the
//! outputs of every algorithm against their exact values at binary64 inputs
//! (`scripts/evaluation_references.py`), within the bounds that the functions
//! here and in `crate::feedback` document.

use std::f64::consts::TAU;

use keptone_math::{cos2pi, sin2pi};
use keptone_model::graph::{Graph, Kind};

use super::Component;
use crate::binary64::finite;
use crate::compile::FeedbackEdge;
use crate::configuration::{Feedback, FeedbackModel};
use crate::feedback::{self, Fed};

/// 2⁻⁵⁰⁰, below which a state is flushed to 0 (MATH §13.4).
const TINY: f64 = f64::from_bits((1023 - 500) << 52);

/// A modulator of an operator: the operator whose output adds to its phase,
/// and whether the edge is its stage's feedback edge, whose term the stage's
/// model gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Modulator {
    /// The operator that modulates.
    from: usize,
    /// Whether the edge is the feedback edge of the stage that it lies in.
    feedback: bool,
}

/// A step of a sample's evaluation: one strongly connected component.
#[derive(Debug, Clone, PartialEq)]
enum Stage {
    /// An operator on no loop.
    Operator(usize),
    /// A self-loop, or a loop through several operators closed by one
    /// feedback edge (MATH §9.7).
    Feedback(Loop),
    /// Operators on loops that no model here solves, a component closed by
    /// more than one feedback edge or by none, which none of the DX7's
    /// algorithms has (MATH §8.1): they give nothing until user graphs come
    /// (DESIGN §16, M8b).
    Silent(Vec<usize>),
}

/// A feedback stage: its operators in an order in which each comes after
/// its modulators but for the feedback edge, the operator that the edge
/// enters, which indexes the stage's memory, the one that it leaves, whose
/// output yᵇ feeds back, and the edge's κ.
#[derive(Debug, Clone, PartialEq)]
struct Loop {
    /// The stage's operators in their order of evaluation.
    order: Vec<usize>,
    /// The operator that the feedback edge enters.
    to: usize,
    /// The operator that it leaves.
    from: usize,
    /// The edge's κ.
    kappa: f64,
}

/// What a voice evaluates at each engine sample (MATH §8.2): its graph's
/// strongly connected components in their order, each operator's modulators
/// and the carriers.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The components, in their order of evaluation.
    stages: Vec<Stage>,
    /// Each operator's modulators, ascending.
    modulators: Vec<Vec<Modulator>>,
    /// The carriers, ascending.
    carriers: Vec<usize>,
}

impl Plan {
    /// The plan of a graph, its components in their order of evaluation
    /// (`super::components`) and its feedback edges with their κ (MATH §9.1):
    /// a self-loop is a stage of one operator, and a loop through several
    /// operators a stage where one feedback edge closes it and its other edges
    /// leave no loop; another loop is silent. A self-loop's edge that is no
    /// feedback edge, which no algorithm has, is one of κ = 1, a modulation of
    /// unit gain.
    #[must_use]
    pub fn new(graph: &Graph, components: &[Component], feedback: &[FeedbackEdge]) -> Self {
        let kappa = |from: usize, to: usize| {
            feedback
                .iter()
                .find(|edge| edge.from == from && edge.to == to)
                .map_or(1.0, |edge| edge.kappa)
        };
        let mut stages = Vec::with_capacity(components.len());
        let mut closing = Vec::new();
        for component in components {
            stages.push(match component {
                Component::Operator(k) => Stage::Operator(*k),
                Component::SelfLoop(k) => {
                    closing.push((*k, *k));
                    Stage::Feedback(Loop {
                        order: vec![*k],
                        to: *k,
                        from: *k,
                        kappa: kappa(*k, *k),
                    })
                }
                Component::Loop(operators) => match loop_order(graph, operators) {
                    Some((order, from, to)) => {
                        closing.push((from, to));
                        Stage::Feedback(Loop {
                            order,
                            to,
                            from,
                            kappa: kappa(from, to),
                        })
                    }
                    None => Stage::Silent(operators.clone()),
                },
            });
        }
        let modulators = (0..graph.operators())
            .map(|k| {
                graph
                    .edges_to(k)
                    .iter()
                    .map(|edge| Modulator {
                        from: edge.from,
                        feedback: closing.contains(&(edge.from, edge.to)),
                    })
                    .collect()
            })
            .collect();
        Self {
            stages,
            modulators,
            carriers: graph.carriers().to_vec(),
        }
    }

    /// The number of its operators, which a voice's outputs and its memory
    /// hold.
    #[must_use]
    pub fn operators(&self) -> usize {
        self.modulators.len()
    }

    /// The lags of the relaxation over an engine sample (MATH §9.6, §9.7),
    /// by the laws that held over it: `values(τ, out)` writes every
    /// operator's θ in turns and its gain g at τ samples into the sample, from
    /// 0, the instant before, to 1, the sample's own, as those laws give them
    /// there, into `out`, by the operators' indices; it is asked once for each
    /// instant that a stage takes. `before` holds the operators' outputs at
    /// the instant before, at which a stage's modulators outside it, which
    /// none of the DX7's algorithms has (§8.1), are held over the substeps;
    /// user graphs (M8b) will take them at the substeps' instants. Nothing
    /// under the other models, which keep no lag, nor for a renewed memory,
    /// whose voice starts at the next output's instant, nor where the memory
    /// or `before` holds fewer operators than the plan.
    ///
    /// τ and the substeps are taken at the larger of the loop gain's
    /// magnitudes at the sample's two ends, B of §9.7, β for a self-loop
    /// ([`feedback::lag`], [`feedback::substeps`]), and each substep's root by
    /// `crate::feedback`'s Newton iteration within its bracket.
    pub fn integrate(
        &self,
        memory: &mut Memory,
        mut values: impl FnMut(f64, &mut [(f64, f64)]),
        before: &[f64],
        settings: &Feedback,
        sample_rate: f64,
    ) {
        let count = self.operators();
        if settings.model != FeedbackModel::Relaxation || !memory.holds(count) || before.len() < count {
            return;
        }
        let Memory {
            lagged,
            fresh,
            lagging,
            scratch,
            slopes,
            start,
            end,
            from,
            to,
            ..
        } = memory;
        let mut ends_taken = false;
        for stage in &self.stages {
            let Stage::Feedback(stage) = stage else {
                continue;
            };
            let entered = stage.to;
            if fresh[entered] {
                continue;
            }
            if !ends_taken {
                values(0.0, &mut start[..count]);
                values(1.0, &mut end[..count]);
                ends_taken = true;
            }
            let ends = [&*start, &*end].map(|at| self.gain(stage, |k| at[k].1).abs());
            let largest = ends[0].max(ends[1]);
            let tau = feedback::lag(largest, settings.relaxation_onset, settings.tau_s);
            lagging[entered] = tau > 0.0;
            if tau <= 0.0 {
                continue;
            }
            let steps = feedback::substeps(largest, tau, sample_rate, settings.max_substeps).max(1);
            let a = finite(1.0 / (2.0 * tau * sample_rate * f64::from(steps)));
            scratch[..count].copy_from_slice(&before[..count]);
            from[..count].copy_from_slice(&start[..count]);
            let mut value = lagged[entered];
            for j in 1..=steps {
                if j >= steps {
                    to[..count].copy_from_slice(&end[..count]);
                } else {
                    values(f64::from(j) / f64::from(steps), &mut to[..count]);
                }
                let yielded = self.chain(stage, stage.kappa, value, |k| from[k], scratch, slopes);
                let bound = to[stage.from].1;
                value = feedback::trapezoid(
                    value,
                    yielded.value,
                    a,
                    bound,
                    stage.kappa,
                    settings.max_iterations,
                    |fed| self.chain(stage, stage.kappa, fed, |k| to[k], scratch, slopes),
                );
                core::mem::swap(from, to);
            }
            lagged[entered] = value;
        }
    }

    /// The operators' outputs at an engine sample's instant, written into
    /// `outputs` by their indices, and the carriers' sum (MATH §1, §8.2):
    /// `at(k)` gives operator k's θ in turns and its gain g there, after the
    /// sample's events. Each operator on no loop is g·sin2pi(θ + μ); a
    /// feedback stage is its model's:
    ///
    /// - `relaxation`: where its lag over the sample was 0, the delay-free
    ///   solution, to which Y is set; elsewhere its operators with κY fed back.
    ///   A renewed memory takes its lag at the instant alone, from Y = 0.
    /// - `dx7-discrete`: its operators with κ times the mean of yᵇ's two
    ///   outputs before, both 0 for a renewed memory.
    /// - `clamp`: the delay-free solution with κ scaled so that the loop gain
    ///   is at most `feedback.beta_clamp`.
    ///
    /// A self-loop's delay-free solution is Kepler's equation's
    /// ([`feedback::point`], [`feedback::clamped`]); a loop's is
    /// `crate::feedback`'s Newton iteration from the previous sample's yᵇ, 0
    /// for a renewed memory. Where `outputs` or the memory holds fewer
    /// operators than the plan, nothing is written and the sum is 0.
    ///
    /// θ + μ rounds within u|θ + μ| turns, μ's sum within (m − 1)u of its m
    /// terms' magnitudes and κ's product within u, sin2pi within 1.46u and the
    /// product with g within u: an operator's output lies within
    /// 2π|g cos 2π(θ + μ)|·(u|θ + μ| + δμ) + 2.46u|y| of its exact value at its
    /// binary64 θ and g, δμ the error that its modulators' outputs and the sum
    /// give μ; the carriers' sum adds (c − 1)u of its c terms' magnitudes.
    pub fn output(
        &self,
        memory: &mut Memory,
        at: impl Fn(usize) -> (f64, f64),
        settings: &Feedback,
        outputs: &mut [f64],
    ) -> f64 {
        let count = self.operators();
        if !memory.holds(count) || outputs.len() < count {
            return 0.0;
        }
        for stage in &self.stages {
            match stage {
                Stage::Operator(k) => {
                    let (phase, gain) = at(*k);
                    let turns = phase + self.modulation(*k, outputs, 0.0, 0.0);
                    outputs[*k] = finite(gain * sin2pi(turns));
                }
                Stage::Silent(operators) => {
                    for &k in operators {
                        outputs[k] = 0.0;
                    }
                }
                Stage::Feedback(stage) => self.feedback(stage, memory, &at, settings, outputs),
            }
        }
        self.carriers
            .iter()
            .fold(0.0, |sum, &carrier| finite(sum + outputs[carrier]))
    }

    /// A feedback stage's outputs at the sample's instant (see
    /// [`Plan::output`]).
    fn feedback(
        &self,
        stage: &Loop,
        memory: &mut Memory,
        at: &impl Fn(usize) -> (f64, f64),
        settings: &Feedback,
        outputs: &mut [f64],
    ) {
        let count = self.operators();
        let Memory {
            lagged,
            before,
            fresh,
            lagging,
            scratch,
            slopes,
            ..
        } = memory;
        let to = stage.to;
        let single = stage.order.len() == 1;
        let loop_gain = self.gain(stage, |k| at(k).1);
        scratch[..count].copy_from_slice(&outputs[..count]);
        match settings.model {
            FeedbackModel::Relaxation => {
                // A renewed memory's Y is 0 ([`Memory::renew`]).
                if fresh[to] {
                    let tau = feedback::lag(loop_gain, settings.relaxation_onset, settings.tau_s);
                    lagging[to] = tau > 0.0;
                }
                if lagging[to] {
                    self.feed(stage, stage.kappa, lagged[to], at, scratch, slopes, outputs);
                } else if single {
                    lagged[to] = self.kepler(stage, at, settings, None, outputs);
                } else {
                    lagged[to] = self.solve(stage, stage.kappa, lagged[to], at, settings, scratch, slopes);
                    self.feed(stage, stage.kappa, lagged[to], at, scratch, slopes, outputs);
                }
            }
            FeedbackModel::Dx7Discrete => {
                let mean = f64::midpoint(lagged[to], before[to]);
                self.feed(stage, stage.kappa, mean, at, scratch, slopes, outputs);
                before[to] = lagged[to];
                lagged[to] = outputs[stage.from];
            }
            FeedbackModel::Clamp => {
                if single {
                    lagged[to] = self.kepler(stage, at, settings, Some(settings.beta_clamp), outputs);
                } else {
                    let magnitude = loop_gain.abs();
                    let kappa = if magnitude > settings.beta_clamp {
                        finite(stage.kappa * (settings.beta_clamp / magnitude))
                    } else {
                        stage.kappa
                    };
                    lagged[to] = self.solve(stage, kappa, lagged[to], at, settings, scratch, slopes);
                    self.feed(stage, kappa, lagged[to], at, scratch, slopes, outputs);
                }
            }
        }
        fresh[to] = false;
    }

    /// A self-loop's delay-free solution at the instant, Kepler's equation's
    /// at θ + μ, its modulators' outputs but its own in μ, with |β| clamped
    /// where a clamp is given, written into outputs and given.
    fn kepler(
        &self,
        stage: &Loop,
        at: &impl Fn(usize) -> (f64, f64),
        settings: &Feedback,
        clamp: Option<f64>,
        outputs: &mut [f64],
    ) -> f64 {
        let to = stage.to;
        let (phase, gain) = at(to);
        let turns = phase + self.modulation(to, outputs, 0.0, 0.0);
        let beta = feedback::beta(stage.kappa, gain);
        let output = match clamp {
            Some(clamp) => feedback::clamped(turns, gain, beta, clamp, settings.max_iterations),
            None => feedback::point(turns, gain, beta, settings.max_iterations),
        };
        outputs[to] = output;
        output
    }

    /// A loop's delay-free solution yᵇ at the instant, at a κ, from a start,
    /// by `crate::feedback`'s Newton iteration within [−|gᵇ|, |gᵇ|], the
    /// operators outside the stage taken from the scratch.
    #[expect(
        clippy::too_many_arguments,
        reason = "a loop's solution takes all that it depends on"
    )]
    fn solve(
        &self,
        stage: &Loop,
        kappa: f64,
        start: f64,
        at: &impl Fn(usize) -> (f64, f64),
        settings: &Feedback,
        scratch: &mut [f64],
        slopes: &mut [f64],
    ) -> f64 {
        let bound = at(stage.from).1;
        feedback::fixed_point(start, bound, kappa, settings.max_iterations, |fed| {
            self.chain(stage, kappa, fed, at, scratch, slopes)
        })
    }

    /// A feedback stage's operators at the instant with a value fed back at a
    /// κ, from the operators outside the stage in the scratch, written into
    /// outputs.
    #[expect(
        clippy::too_many_arguments,
        reason = "a stage's evaluation takes all that it depends on"
    )]
    fn feed(
        &self,
        stage: &Loop,
        kappa: f64,
        fed: f64,
        at: &impl Fn(usize) -> (f64, f64),
        scratch: &mut [f64],
        slopes: &mut [f64],
        outputs: &mut [f64],
    ) {
        self.chain(stage, kappa, fed, at, scratch, slopes);
        for &k in &stage.order {
            outputs[k] = scratch[k];
        }
    }

    /// The loop gain of a feedback stage at its operators' gains, B of MATH
    /// §9.7 and β of §9.1 for a self-loop: 2πκ times the gain of the operator
    /// that the edge enters, times 2π times each other's, each product
    /// rounding within u, saturated at binary64's largest finite number.
    fn gain(&self, stage: &Loop, gains: impl Fn(usize) -> f64) -> f64 {
        let _ = self;
        stage.order.iter().fold(1.0, |product, &k| {
            let factor = if k == stage.to {
                feedback::beta(stage.kappa, gains(k))
            } else {
                finite(TAU * gains(k))
            };
            finite(product * factor)
        })
    }

    /// μ of operator k, its modulators' outputs added in their ascending
    /// order, a feedback edge's term being κ times `fed`.
    fn modulation(&self, k: usize, outputs: &[f64], kappa: f64, fed: f64) -> f64 {
        self.modulators[k].iter().fold(0.0, |sum, modulator| {
            let term = if modulator.feedback {
                finite(kappa * fed)
            } else {
                outputs[modulator.from]
            };
            finite(sum + term)
        })
    }

    /// A feedback stage's operators evaluated in their order with a value
    /// `fed` fed back at a κ, into the scratch, whose entries outside the stage hold
    /// its modulators' outputs: each operator's output and its derivative in
    /// the fed-back value, forward from κ where the feedback edge enters; G of
    /// the fed-back value and its slope.
    fn chain(
        &self,
        stage: &Loop,
        kappa: f64,
        fed: f64,
        values: impl Fn(usize) -> (f64, f64),
        scratch: &mut [f64],
        slopes: &mut [f64],
    ) -> Fed {
        let mut entering = 0.0;
        for &k in &stage.order {
            let (phase, gain) = values(k);
            let turns = phase + self.modulation(k, scratch, kappa, fed);
            let slope = self.modulators[k].iter().fold(0.0, |sum, modulator| {
                let term = if modulator.feedback {
                    kappa
                } else if stage.order.contains(&modulator.from) {
                    slopes[modulator.from]
                } else {
                    0.0
                };
                finite(sum + term)
            });
            scratch[k] = finite(gain * sin2pi(turns));
            slopes[k] = finite(finite(gain * TAU * cos2pi(turns)) * slope);
            if k == stage.to {
                entering = turns;
            }
        }
        Fed {
            turns: entering,
            value: scratch[stage.from],
            slope: slopes[stage.from],
        }
    }
}

/// The operators that a loop's operators run through, in an order in which
/// each comes after its modulators but the feedback edge's, with the feedback
/// edge's ends: none where the loop is closed by more than one feedback edge,
/// or by none, or where its other edges leave a loop.
fn loop_order(graph: &Graph, operators: &[usize]) -> Option<(Vec<usize>, usize, usize)> {
    let inside = |k: usize| operators.binary_search(&k).is_ok();
    let mut closing = graph
        .edges()
        .iter()
        .filter(|edge| edge.kind == Kind::Feedback && inside(edge.from) && inside(edge.to));
    let edge = closing.next()?;
    if closing.next().is_some() {
        return None;
    }
    let (from, to) = (edge.from, edge.to);
    // Kahn's order over the loop's other edges, the highest operator first
    // among those whose modulators are all placed, as the components are.
    let mut order = Vec::with_capacity(operators.len());
    let mut placed = vec![false; operators.len()];
    while order.len() < operators.len() {
        let (index, &k) = operators.iter().enumerate().rev().find(|&(index, &k)| {
            !placed[index]
                && graph
                    .edges_to(k)
                    .iter()
                    .filter(|modulator| inside(modulator.from) && (modulator.from, modulator.to) != (from, to))
                    .all(|modulator| order.contains(&modulator.from))
        })?;
        placed[index] = true;
        order.push(k);
    }
    Some((order, from, to))
}

/// What a voice's feedback keeps from sample to sample (MATH §9.6, §9.7),
/// for each feedback stage by the operator that its edge enters: the
/// relaxation's lagged Y and whether the sample's lag was above 0, the DX7's
/// recurrence's two outputs before, the delay-free solution's last yᵇ, from
/// which the next starts, and whether its voice starts at the next output's
/// instant; and the room in which a stage is evaluated.
#[derive(Debug, Clone, PartialEq)]
pub struct Memory {
    /// Y, yᵇ\[n − 1\] or the last yᵇ.
    lagged: Vec<f64>,
    /// yᵇ\[n − 2\].
    before: Vec<f64>,
    /// Whether the stage's voice starts at the next output's instant.
    fresh: Vec<bool>,
    /// Whether the relaxation's lag over the sample was above 0.
    lagging: Vec<bool>,
    /// The outputs in which a stage is evaluated.
    scratch: Vec<f64>,
    /// Their derivatives in the fed-back value.
    slopes: Vec<f64>,
    /// The operators' θ and g at the sample's start.
    start: Vec<(f64, f64)>,
    /// At its end.
    end: Vec<(f64, f64)>,
    /// At a substep's start.
    from: Vec<(f64, f64)>,
    /// At its end.
    to: Vec<(f64, f64)>,
}

impl Memory {
    /// A memory for a voice of so many operators, renewed, or none where the
    /// memory cannot be had.
    #[must_use]
    pub fn new(operators: usize) -> Option<Self> {
        fn filled<T: Clone>(operators: usize, value: T) -> Option<Vec<T>> {
            let mut values = Vec::new();
            values.try_reserve_exact(operators).ok()?;
            values.resize(operators, value);
            Some(values)
        }
        Some(Self {
            lagged: filled(operators, 0.0)?,
            before: filled(operators, 0.0)?,
            fresh: filled(operators, true)?,
            lagging: filled(operators, false)?,
            scratch: filled(operators, 0.0)?,
            slopes: filled(operators, 0.0)?,
            start: filled(operators, (0.0, 0.0))?,
            end: filled(operators, (0.0, 0.0))?,
            from: filled(operators, (0.0, 0.0))?,
            to: filled(operators, (0.0, 0.0))?,
        })
    }

    /// Whether it holds so many operators.
    fn holds(&self, operators: usize) -> bool {
        self.lagged.len() >= operators
    }

    /// Renews every stage's memory, as a new voice's: its voice starts at the
    /// next output's instant (MATH §9.6).
    pub fn renew(&mut self) {
        self.lagged.fill(0.0);
        self.before.fill(0.0);
        self.fresh.fill(true);
    }

    /// Renews the memory of each stage of a new plan whose component, its
    /// operators and its feedback edge, the old plan does not have, and keeps
    /// the others' (MATH §8.3).
    pub fn renew_changed(&mut self, old: &Plan, new: &Plan) {
        for stage in &new.stages {
            let Stage::Feedback(stage) = stage else {
                continue;
            };
            let kept = old.stages.iter().any(|before| {
                matches!(before, Stage::Feedback(before)
                    if before.to == stage.to && before.from == stage.from && same_operators(&before.order, &stage.order))
            });
            if !kept && stage.to < self.lagged.len() {
                self.lagged[stage.to] = 0.0;
                self.before[stage.to] = 0.0;
                self.fresh[stage.to] = true;
            }
        }
    }

    /// Sets every stage's state to 0, its voice going on, as MATH §13.4 does
    /// once a voice has been silent for `engine.idle_s`.
    pub fn zero(&mut self) {
        self.lagged.fill(0.0);
        self.before.fill(0.0);
    }

    /// Sets each state whose magnitude lies below 2⁻⁵⁰⁰ to 0, as MATH §13.4
    /// does at every 64th engine sample.
    pub fn flush(&mut self) {
        for state in self.lagged.iter_mut().chain(self.before.iter_mut()) {
            if state.abs() < TINY {
                *state = 0.0;
            }
        }
    }
}

/// Whether two stages' orders hold the same operators.
fn same_operators(a: &[usize], b: &[usize]) -> bool {
    a.len() == b.len() && a.iter().all(|k| b.contains(k))
}
