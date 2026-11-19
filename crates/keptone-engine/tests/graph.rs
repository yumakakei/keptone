// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The order of evaluation (docs/MATH.md §8.2): the components of the DX7's
//! algorithms, those of random graphs against what reachability makes of
//! them, and graphs too deep for a recursion on the thread's stack.

use keptone_engine::graph::{Component, components};
use keptone_model::graph::{Edge, Graph, Kind};
use keptone_model::voice::Algorithm;

/// `SplitMix64`, the generator kiral's tests use: a 64-bit state advanced by a
/// constant and mixed, so that a seed gives the same sequence on every system.
#[derive(Debug, Clone)]
struct SplitMix64(u64);

impl SplitMix64 {
    /// The next 64 bits.
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A uniform integer of [0, n), n not 0.
    fn below(&mut self, n: usize) -> usize {
        let n = u64::try_from(n).expect("a usize of 64 bits at most");
        usize::try_from(self.next_u64() % n).expect("below a usize")
    }
}

#[test]
fn the_algorithms_go_from_op6_down_with_the_loops_of_4_and_6() {
    for number in 1..=32 {
        let algorithm = Algorithm::new(number).expect("1 to 32");
        let without = components(&Graph::of_algorithm(algorithm, false));
        let operators: Vec<_> = (0..6).rev().map(Component::Operator).collect();
        assert_eq!(without, operators, "algorithm {number} without its feedback");
        let graph = Graph::of_algorithm(algorithm, true);
        let feedback = graph
            .edges()
            .iter()
            .find(|edge| edge.kind == Kind::Feedback)
            .expect("a feedback edge");
        let expected = match number {
            4 => vec![
                Component::Loop(vec![3, 4, 5]),
                Component::Operator(2),
                Component::Operator(1),
                Component::Operator(0),
            ],
            6 => vec![
                Component::Loop(vec![4, 5]),
                Component::Operator(3),
                Component::Operator(2),
                Component::Operator(1),
                Component::Operator(0),
            ],
            _ => (0..6)
                .rev()
                .map(|k| {
                    if k == feedback.to {
                        Component::SelfLoop(k)
                    } else {
                        Component::Operator(k)
                    }
                })
                .collect(),
        };
        assert_eq!(components(&graph), expected, "algorithm {number}");
    }
}

#[test]
fn components_are_what_reachability_makes_of_random_graphs() {
    let mut random = SplitMix64(0x6772_6170_6873);
    for _ in 0..4000 {
        let operators = 1 + random.below(12);
        // An edge's chance runs from 1/(4N) to 1/2, sparse graphs to dense.
        let chance = 1 + random.below(2 * operators);
        let mut edges = Vec::new();
        for from in 0..operators {
            for to in 0..operators {
                if random.below(4 * operators) < chance {
                    let kind = if random.below(4) == 0 {
                        Kind::Feedback
                    } else {
                        Kind::Modulation
                    };
                    edges.push(Edge { from, to, kind });
                }
            }
        }
        let carriers: Vec<_> = (0..operators).filter(|_| random.below(2) == 0).collect();
        check(&Graph::new(operators, edges, carriers).expect("within the graph"));
    }
}

/// Checks the components of a graph against its reachability, by Warshall's
/// closure: that they part the operators, that two operators share one exactly
/// when each reaches the other, that each is of the kind its edges make it,
/// that every edge goes forward in their order, and that each comes first of
/// those whose modulators all came before it by its highest operator.
fn check(graph: &Graph) {
    let n = graph.operators();
    let mut reach = vec![vec![false; n]; n];
    for edge in graph.edges() {
        reach[edge.from][edge.to] = true;
    }
    for k in 0..n {
        for i in 0..n {
            if reach[i][k] {
                for j in 0..n {
                    if reach[k][j] {
                        reach[i][j] = true;
                    }
                }
            }
        }
    }
    let order = components(graph);
    let mut place = vec![None; n];
    for (position, component) in order.iter().enumerate() {
        let operators = component.operators();
        assert!(operators.windows(2).all(|pair| pair[0] < pair[1]), "{component:?}");
        for &k in operators {
            assert_eq!(place[k].replace(position), None, "operator {k} in two components");
        }
        match component {
            Component::Operator(k) => assert!(!reach[*k][*k], "{component:?}"),
            Component::SelfLoop(k) => assert!(graph.edges_to(*k).iter().any(|edge| edge.from == *k)),
            Component::Loop(operators) => assert!(operators.len() > 1),
        }
    }
    let place: Vec<usize> = place
        .into_iter()
        .map(|p| p.expect("every operator in a component"))
        .collect();
    for a in 0..n {
        for b in 0..n {
            if a != b {
                assert_eq!(
                    place[a] == place[b],
                    reach[a][b] && reach[b][a],
                    "operators {a} and {b}"
                );
            }
        }
    }
    for edge in graph.edges() {
        assert!(place[edge.from] <= place[edge.to], "{edge:?}");
    }
    let highest = |component: &Component| component.operators().last().copied().expect("not empty");
    for (position, chosen) in order.iter().enumerate() {
        for later in &order[position + 1..] {
            let ready = later.operators().iter().all(|&k| {
                graph
                    .edges_to(k)
                    .iter()
                    .all(|edge| place[edge.from] < position || later.operators().contains(&edge.from))
            });
            if ready {
                assert!(
                    highest(later) < highest(chosen),
                    "{later:?} was ready before {chosen:?}"
                );
            }
        }
    }
}

#[test]
fn a_long_chain_and_a_long_loop_need_no_deep_stack() {
    // Each operator modulates the next: a search from the first goes as deep
    // as the chain is long, which a recursion would take from the stack.
    let n = 200_000;
    let modulation = |from, to| Edge {
        from,
        to,
        kind: Kind::Modulation,
    };
    let chain = Graph::new(n, (1..n).map(|k| modulation(k - 1, k)), [n - 1]).expect("within the graph");
    let order = components(&chain);
    assert_eq!(order.len(), n);
    assert!(
        order
            .iter()
            .zip(0..n)
            .all(|(component, k)| *component == Component::Operator(k))
    );
    let ring = Graph::new(n, (0..n).map(|k| modulation(k, (k + 1) % n)), [0]).expect("within the graph");
    assert_eq!(components(&ring), [Component::Loop((0..n).collect())]);
}
