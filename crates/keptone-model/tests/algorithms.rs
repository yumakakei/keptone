// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The DX7's 32 algorithms (docs/MATH.md §8.1): the table, which transcribes
//! the chart printed on the instruments' panels, against two encodings made
//! apart from it, \[MSFA\]'s flags and the OPS's algorithm ROM as
//! \[SHIRRIFF-ALG\] reads it, which `scripts/algorithm_sources.py` decodes
//! into `tests/data/algorithms.txt` (docs/VERIFICATION.md V16); and the form that
//! the chart gives every algorithm.

use std::collections::{BTreeMap, BTreeSet};

use keptone_model::graph::{Graph, Kind};
use keptone_model::voice::Algorithm;

/// The two encodings in the canonical form, one line an algorithm of each.
const SOURCES: &str = include_str!("data/algorithms.txt");

/// An algorithm in the canonical form of MATH §8.1, by the operators'
/// numbers: its modulations and its feedback edges, each (from, to), and its
/// carriers.
#[derive(Debug, PartialEq, Eq)]
struct Canonical {
    modulations: BTreeSet<(usize, usize)>,
    feedback: BTreeSet<(usize, usize)>,
    carriers: BTreeSet<usize>,
}

/// The table's algorithm of a number, with its feedback edge.
fn table(number: u8) -> Canonical {
    let graph = Graph::of_algorithm(Algorithm::new(number).expect("1 to 32"), true);
    let edges = |kind| {
        graph
            .edges()
            .iter()
            .filter(|edge| edge.kind == kind)
            .map(|edge| (edge.from + 1, edge.to + 1))
            .collect()
    };
    Canonical {
        modulations: edges(Kind::Modulation),
        feedback: edges(Kind::Feedback),
        carriers: graph.carriers().iter().map(|carrier| carrier + 1).collect(),
    }
}

/// A list of edges written j>k, or "-" for none.
fn edges(list: &str) -> BTreeSet<(usize, usize)> {
    if list == "-" {
        return BTreeSet::new();
    }
    list.split(' ')
        .map(|edge| {
            let (from, to) = edge.split_once('>').expect("an edge as j>k");
            (from.parse().expect("a number"), to.parse().expect("a number"))
        })
        .collect()
}

#[test]
fn the_table_agrees_with_msfa_and_the_ops_rom() {
    let mut seen: BTreeMap<&str, Vec<u8>> = BTreeMap::new();
    for line in SOURCES.lines().filter(|line| !line.starts_with('#')) {
        let (head, lists) = line
            .split_once(": ")
            .expect("the source and the algorithm, then the lists");
        let (source, number) = head.split_once(' ').expect("the source, then the algorithm");
        let number: u8 = number.parse().expect("the algorithm's number");
        let [modulations, feedback, carriers] = *lists.split(" | ").collect::<Vec<_>>() else {
            panic!("not three lists: {line}");
        };
        let decoded = Canonical {
            modulations: edges(modulations),
            feedback: edges(feedback),
            carriers: carriers
                .split(' ')
                .map(|carrier| carrier.parse().expect("a number"))
                .collect(),
        };
        assert_eq!(decoded, table(number), "{source}, algorithm {number}");
        seen.entry(source).or_default().push(number);
    }
    let all: Vec<u8> = (1..=32).collect();
    assert_eq!(seen, BTreeMap::from([("msfa", all.clone()), ("ops", all)]));
}

#[test]
fn every_algorithm_has_the_form_of_the_chart() {
    for number in 1..=32 {
        let algorithm = Algorithm::new(number).expect("1 to 32");
        let graph = Graph::of_algorithm(algorithm, true);
        assert_eq!(graph.operators(), 6);
        // The table holds each edge and carrier once, within six operators,
        // and two operators joined by one edge at most: the same edges and
        // carriers in another order make the same graph.
        let again = Graph::new(
            6,
            graph.edges().iter().rev().copied(),
            graph.carriers().iter().rev().copied(),
        );
        assert_eq!(again.as_ref(), Some(&graph), "algorithm {number}");
        // The modulations go down, as the DX7 computes OP6 first. The feedback
        // edge is a self-loop, but for the loops of algorithms 4 and 6.
        let feedback: Vec<_> = graph
            .edges()
            .iter()
            .filter(|edge| edge.kind == Kind::Feedback)
            .collect();
        assert_eq!(feedback.len(), 1, "algorithm {number}");
        let expected = match number {
            4 => (3, 5),
            6 => (4, 5),
            _ => (feedback[0].to, feedback[0].to),
        };
        assert_eq!((feedback[0].from, feedback[0].to), expected, "algorithm {number}");
        for edge in graph.edges().iter().filter(|edge| edge.kind == Kind::Modulation) {
            assert!(edge.from > edge.to, "algorithm {number}: {edge:?}");
        }
        // Each operator is a carrier or modulates others, never both but
        // along the feedback edge.
        for operator in 0..6 {
            let carrier = graph.carriers().contains(&operator);
            let modulates = graph
                .edges()
                .iter()
                .any(|edge| edge.from == operator && edge.kind == Kind::Modulation);
            assert!(carrier != modulates, "algorithm {number}, OP{}", operator + 1);
        }
        // Without its feedback edge, the algorithm is the same graph less it.
        let without = Graph::of_algorithm(algorithm, false);
        let rest: Vec<_> = graph
            .edges()
            .iter()
            .filter(|edge| edge.kind == Kind::Modulation)
            .copied()
            .collect();
        assert_eq!(without.edges(), rest, "algorithm {number}");
        assert_eq!(without.carriers(), graph.carriers(), "algorithm {number}");
    }
}
