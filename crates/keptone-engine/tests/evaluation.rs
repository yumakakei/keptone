// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! A voice's operators at engine samples (docs/MATH.md §1, §8.2, §9): the
//! outputs of the DX7's 32 algorithms with and without their feedback, under
//! the three models of `feedback.model`, through sequences from a new voice in
//! which θ and g move and may jump where a sample's events would act, against
//! their exact values at binary64 inputs, which
//! `scripts/evaluation_references.py` computed with mpmath
//! (`tests/data/evaluation.txt`), within the bounds of their errors that the
//! script follows from the documented ones; a loop that no model solves; the
//! memory's renewal; and the relaxation's order of convergence on the loops of
//! algorithms 4 and 6.

use keptone_engine::compile::FeedbackEdge;
use keptone_engine::configuration::{Feedback, FeedbackModel};
use keptone_engine::graph::components;
use keptone_engine::graph::evaluation::{Memory, Plan};
use keptone_model::graph::{Edge, Graph, Kind};
use keptone_model::voice::Algorithm;

/// The references: each line's kind and its numbers.
fn references() -> Vec<(String, Vec<f64>)> {
    let number = |word: &str| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits"));
    include_str!("data/evaluation.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let mut words = line.split(' ');
            let kind = words.next().expect("a kind").to_owned();
            (kind, words.map(number).collect())
        })
        .collect()
}

/// An operator's θ and g as lines in time, which may jump at a sample: the
/// primed law in effect from the jump's sample on, at its instant and over the
/// samples after it.
#[derive(Debug, Clone, Copy)]
struct Law {
    /// θ, its speed, g and its slope before the jump.
    before: [f64; 4],
    /// The jump's sample, 2⁶⁰ where there is none.
    jump: f64,
    /// θ, its speed, g and its slope from the jump on.
    after: [f64; 4],
}

impl Law {
    /// θ and g at t samples by the law in effect from sample `since` on, as
    /// the script computes them in binary64.
    fn at(&self, t: f64, since: f64) -> (f64, f64) {
        let [theta, speed, gain, slope] = if self.jump <= since { self.after } else { self.before };
        (theta + speed * t, gain + slope * t)
    }
}

/// A case's head: its graph's plan, its settings, the engine's rate, its
/// samples and the operators' laws.
struct Case {
    /// The algorithm's number.
    algorithm: u8,
    /// Its plan.
    plan: Plan,
    /// The feedback's settings.
    settings: Feedback,
    /// The engine's rate.
    rate: f64,
    /// The operators' laws.
    laws: Vec<Law>,
}

impl Case {
    /// The case of a head line.
    fn of(x: &[f64]) -> Self {
        let [
            algorithm,
            feedback,
            model,
            kappa,
            tau,
            onset,
            clamp,
            iterations,
            substeps,
            rate,
            _,
        ] = x[..11]
        else {
            panic!("a case's head")
        };
        let algorithm = (1..=32_u8)
            .find(|number| f64::from(*number).to_bits() == algorithm.to_bits())
            .expect("an algorithm");
        let graph = Graph::of_algorithm(Algorithm::new(algorithm).expect("1 to 32"), feedback != 0.0);
        let plan = plan_of(&graph, kappa);
        let model = match model {
            0.0 => FeedbackModel::Relaxation,
            1.0 => FeedbackModel::Dx7Discrete,
            _ => FeedbackModel::Clamp,
        };
        let count = |value: f64| {
            (1..=1024_u32)
                .find(|n| f64::from(*n).to_bits() == value.to_bits())
                .expect("a count")
        };
        let laws = x[11..]
            .as_chunks::<9>()
            .0
            .iter()
            .map(|law| Law {
                before: [law[0], law[1], law[2], law[3]],
                jump: law[4],
                after: [law[5], law[6], law[7], law[8]],
            })
            .collect();
        Self {
            algorithm,
            plan,
            settings: Feedback {
                model,
                tau_s: tau,
                beta_clamp: clamp,
                max_iterations: count(iterations),
                relaxation_onset: onset,
                max_substeps: count(substeps),
            },
            rate,
            laws,
        }
    }
}

/// A graph's plan, its feedback edges at a κ.
fn plan_of(graph: &Graph, kappa: f64) -> Plan {
    let feedback: Vec<FeedbackEdge> = graph
        .edges()
        .iter()
        .filter(|edge| edge.kind == Kind::Feedback)
        .map(|edge| FeedbackEdge {
            from: edge.from,
            to: edge.to,
            kappa,
        })
        .collect();
    Plan::new(graph, &components(graph), &feedback)
}

/// |y − (hi + lo)|.
fn error(y: f64, hi: f64, lo: f64) -> f64 {
    ((y - hi) - lo).abs()
}

#[test]
fn outputs_within_their_bound() {
    let lines = references();
    let mut index = 0;
    let (mut cases, mut samples, mut worst, mut models) = (0, 0, 0.0_f64, [0; 3]);
    while index < lines.len() {
        let (kind, head) = &lines[index];
        assert_eq!(kind, "case");
        index += 1;
        let case = Case::of(head);
        models[match case.settings.model {
            FeedbackModel::Relaxation => 0,
            FeedbackModel::Dx7Discrete => 1,
            FeedbackModel::Clamp => 2,
        }] += 1;
        let mut memory = Memory::new(6).expect("memory");
        let (mut outputs, mut before) = ([0.0; 6], [0.0; 6]);
        while index < lines.len() && lines[index].0 == "sample" {
            let row = &lines[index].1;
            index += 1;
            let n = row[0];
            if n > 0.0 {
                case.plan.integrate(
                    &mut memory,
                    |s, out: &mut [(f64, f64)]| {
                        for (k, slot) in out.iter_mut().enumerate() {
                            *slot = case.laws[k].at((n - 1.0) + s, n - 1.0);
                        }
                    },
                    &before,
                    &case.settings,
                    case.rate,
                );
            }
            let sum = case
                .plan
                .output(&mut memory, |k| case.laws[k].at(n, n), &case.settings, &mut outputs);
            for (k, output) in outputs.iter().enumerate() {
                let [hi, lo, bound] = row[1 + 3 * k..4 + 3 * k] else {
                    panic!("an output")
                };
                let e = error(*output, hi, lo);
                assert!(
                    e <= bound,
                    "algorithm {}, {:?}, sample {n}, OP{}: {output:e}, exactly {hi:e}, error {e:e} > {bound:e}",
                    case.algorithm,
                    case.settings.model,
                    k + 1
                );
                if bound > 0.0 {
                    worst = worst.max(e / bound);
                }
            }
            let [hi, lo, bound] = row[19..22] else { panic!("a sum") };
            let e = error(sum, hi, lo);
            assert!(
                e <= bound,
                "algorithm {}, sample {n}: the sum {sum:e}, exactly {hi:e}",
                case.algorithm
            );
            before = outputs;
            samples += 1;
        }
        cases += 1;
    }
    assert!(
        cases >= 384 && models.iter().all(|&count| count > 100),
        "{cases} cases, {models:?}"
    );
    println!("evaluation: {cases} cases, {samples} samples, the worst error {worst:.3} of its bound");
}

#[test]
fn a_loop_that_no_model_solves_gives_nothing() {
    // Two feedback edges close one loop of OP1 and OP2, which no algorithm has:
    // its operators give 0 and the carrier above them takes nothing from them.
    let edges = [
        Edge {
            from: 0,
            to: 1,
            kind: Kind::Feedback,
        },
        Edge {
            from: 1,
            to: 0,
            kind: Kind::Feedback,
        },
        Edge {
            from: 1,
            to: 2,
            kind: Kind::Modulation,
        },
    ];
    let graph = Graph::new(3, edges, [2]).expect("a graph");
    let plan = plan_of(&graph, 0.25);
    let settings = Feedback {
        model: FeedbackModel::Clamp,
        tau_s: 1.5 / 49096.0,
        beta_clamp: 0.999,
        max_iterations: 16,
        relaxation_onset: 0.9,
        max_substeps: 64,
    };
    let mut memory = Memory::new(3).expect("memory");
    let mut outputs = [1.0; 3];
    let sum = plan.output(&mut memory, |_| (0.125, 1.5), &settings, &mut outputs);
    let bits = |values: &[f64]| values.iter().map(|value| value.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(&outputs[..2]), [0, 0]);
    assert_eq!(sum.to_bits(), outputs[2].to_bits());
    assert_eq!(sum.to_bits(), (1.5 * keptone_math::sin2pi(0.125)).to_bits());
    // Outputs or a memory that hold fewer operators than the plan give nothing.
    let mut short = [7.0; 2];
    let given = plan.output(&mut memory, |_| (0.125, 1.5), &settings, &mut short);
    assert_eq!((given.to_bits(), bits(&short)), (0, bits(&[7.0; 2])));
    let mut small = Memory::new(2).expect("memory");
    let given = plan.output(&mut small, |_| (0.125, 1.5), &settings, &mut outputs);
    assert_eq!(given.to_bits(), 0);
}

/// The feedback's default settings but its model and τ.
fn settings(model: FeedbackModel) -> Feedback {
    Feedback {
        model,
        tau_s: 1.5 / 49096.0,
        beta_clamp: 0.999,
        max_iterations: 16,
        relaxation_onset: 0.9,
        max_substeps: 64,
    }
}

#[test]
fn a_memory_is_renewed_where_its_component_changes() {
    // Under the DX7's recurrence a stage's output depends on its two outputs
    // before: a memory renewed gives a new voice's first output again, and one
    // kept where the component stays goes on with its sequence.
    let settings = settings(FeedbackModel::Dx7Discrete);
    let one = plan_of(&Graph::of_algorithm(Algorithm::new(1).expect("1"), true), 0.5);
    let two = plan_of(&Graph::of_algorithm(Algorithm::new(2).expect("2"), true), 0.5);
    let at = |k: usize| (0.1 + 0.07 * f64::from(u32::try_from(k).expect("small")), 1.25);
    let run = |plan: &Plan, memory: &mut Memory| {
        let mut outputs = [0.0; 6];
        plan.output(memory, at, &settings, &mut outputs);
        outputs.map(f64::to_bits)
    };
    let mut fresh = Memory::new(6).expect("memory");
    let first = run(&one, &mut fresh);
    let second = run(&one, &mut fresh);
    assert_ne!(first[5], second[5], "the recurrence moves");
    let first_two = run(&two, &mut Memory::new(6).expect("memory"));
    // Algorithm 1 kept: OP6's self-loop stays, and so does its memory.
    let mut kept = Memory::new(6).expect("memory");
    run(&one, &mut kept);
    kept.renew_changed(&one, &one);
    assert_eq!(run(&one, &mut kept)[5], second[5]);
    // Algorithm 2 twice, then 1, whose OP6 memory is renewed, then 2 again,
    // whose OP2 memory, which algorithm 1 has no stage for, is renewed too.
    let mut moved = Memory::new(6).expect("memory");
    run(&two, &mut moved);
    run(&two, &mut moved);
    moved.renew_changed(&two, &one);
    assert_eq!(run(&one, &mut moved)[5], first[5]);
    let mut unrenewed = moved.clone();
    moved.renew_changed(&one, &two);
    assert_eq!(run(&two, &mut moved)[1], first_two[1]);
    assert_ne!(
        run(&two, &mut unrenewed)[1],
        first_two[1],
        "without the renewal the sequence goes on"
    );
    // renew renews every stage.
    let mut renewed = Memory::new(6).expect("memory");
    run(&one, &mut renewed);
    renewed.renew();
    assert_eq!(run(&one, &mut renewed)[5], first[5]);
    // Under the relaxation, at a loop gain of 3.9 where it lags, a renewed
    // memory's voice starts at the next output's instant: the sample before it
    // is not integrated, and its first output is a new voice's.
    let relaxation = self::settings(FeedbackModel::Relaxation);
    let lagging = |plan: &Plan, memory: &mut Memory, integrate: bool| {
        let mut outputs = [0.0; 6];
        if integrate {
            plan.integrate(
                memory,
                |_, out: &mut [(f64, f64)]| {
                    for (k, slot) in out.iter_mut().enumerate() {
                        *slot = at(k);
                    }
                },
                &[0.0; 6],
                &relaxation,
                48_000.0,
            );
        }
        plan.output(memory, at, &relaxation, &mut outputs);
        outputs.map(f64::to_bits)
    };
    let started = lagging(&one, &mut Memory::new(6).expect("memory"), false);
    let mut going = Memory::new(6).expect("memory");
    lagging(&one, &mut going, false);
    let goes_on = lagging(&one, &mut going, true);
    assert_ne!(goes_on[5], started[5], "the lag moves");
    going.renew();
    assert_eq!(lagging(&one, &mut going, true)[5], started[5]);
    // flush sets states below 2⁻⁵⁰⁰ to 0 and keeps the others.
    let mut tiny = Memory::new(6).expect("memory");
    let quiet = |_: usize| (0.25, 1e-200);
    let mut outputs = [0.0; 6];
    one.output(&mut tiny, quiet, &settings, &mut outputs);
    one.output(&mut tiny, quiet, &settings, &mut outputs);
    let mut flushed = tiny.clone();
    flushed.flush();
    assert_ne!(flushed, tiny);
    let mut loud = Memory::new(6).expect("memory");
    one.output(&mut loud, at, &settings, &mut outputs);
    let mut unchanged = loud.clone();
    unchanged.flush();
    assert_eq!(unchanged, loud);
}

#[test]
fn the_relaxation_of_a_loop_converges_at_order_two() {
    // The loops of algorithms 4 and 6 at κ = 1/2 and loop gains near 2.5,
    // 124·g₆g₅g₄ and 19.7·g₆g₅, where τ is feedback.tau_s throughout and one
    // substep a sample keeps each equation's root unique at 48 kHz and above,
    // B lying below 1 + 2τfₑ = 3.9: the samples' length halves as the rate
    // doubles, θ and g following the same lines in seconds, and the output of
    // the operator that the feedback leaves at 2 ms converges at order 2.
    for (algorithm, gains) in [(4_u8, [0.5, 0.25, 0.16]), (6, [0.5, 0.25, 0.0])] {
        let graph = Graph::of_algorithm(Algorithm::new(algorithm).expect("an algorithm"), true);
        let plan = plan_of(&graph, 0.5);
        let settings = settings(FeedbackModel::Relaxation);
        let at_rate = |rate: f64| {
            let law = |k: usize, seconds: f64| {
                let index = f64::from(u32::try_from(k).expect("small"));
                let theta = 0.11 * index + (180.0 + 37.0 * index) * seconds;
                let gain = match k {
                    5 => gains[0] * (1.0 - 20.0 * seconds),
                    4 => gains[1] * (1.0 + 30.0 * seconds),
                    3 => gains[2],
                    _ => 0.5,
                };
                (theta, gain)
            };
            let mut memory = Memory::new(6).expect("memory");
            let (mut outputs, mut before) = ([0.0; 6], [0.0; 6]);
            let samples = (0..100_000_u32)
                .find(|n| f64::from(*n) >= 0.002 * rate)
                .expect("a few hundred samples");
            for n in 0..=samples {
                let n = f64::from(n);
                if n > 0.0 {
                    plan.integrate(
                        &mut memory,
                        |s, out: &mut [(f64, f64)]| {
                            for (k, slot) in out.iter_mut().enumerate() {
                                *slot = law(k, ((n - 1.0) + s) / rate);
                            }
                        },
                        &before,
                        &settings,
                        rate,
                    );
                }
                plan.output(&mut memory, |k| law(k, n / rate), &settings, &mut outputs);
                before = outputs;
            }
            let leaving = if algorithm == 4 { 3 } else { 4 };
            outputs[leaving]
        };
        let values: Vec<f64> = [48_000.0, 96_000.0, 192_000.0, 384_000.0]
            .into_iter()
            .map(at_rate)
            .collect();
        let differences: Vec<f64> = values.windows(2).map(|pair| (pair[0] - pair[1]).abs()).collect();
        for pair in differences.windows(2) {
            let ratio = pair[0] / pair[1];
            assert!(
                (3.4..4.6).contains(&ratio),
                "algorithm {algorithm}: {values:?}, ratio {ratio}"
            );
        }
        println!("algorithm {algorithm}: differences {differences:?}");
    }
}
