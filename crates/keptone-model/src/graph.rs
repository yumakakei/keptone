// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The operator graph (docs/MATH.md §8): a graph of any number of operators,
//! the edges along which they modulate one another and the carriers, whose
//! outputs a voice adds, and the DX7's 32 algorithms as graphs of six.
//!
//! Any directed graph is one (MATH §8.4), so that the 32 algorithms are 32 of
//! its values and nothing in it is bound to them: an edge joins any operator
//! to any other, in either direction, or to itself; loops of any length, and
//! any number of them, are allowed; and any of the operators are carriers. A
//! graph names its operators by their indices, 0 for OP1, as
//! [`crate::voice::Voice`] holds them; the table of the algorithms gives them
//! by their numbers, as the chart it transcribes does.

use crate::voice::Algorithm;

/// What sets an edge's gain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A modulation, of unit gain (MATH §8.1).
    Modulation,
    /// A feedback edge, of the gain that its FB sets (MATH §9.1).
    Feedback,
}

/// An edge of a graph: operator `from` modulates operator `to`, both by their
/// indices, with the gain that its kind says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edge {
    /// The index of the operator that modulates.
    pub from: usize,
    /// The index of the operator modulated.
    pub to: usize,
    /// What sets its gain.
    pub kind: Kind,
}

/// A graph of operators (MATH §8.4): operators 0 to N − 1, the edges along
/// which they modulate one another, and the carriers.
///
/// It holds each edge and each carrier once, two operators joined in one
/// direction by one edge at most, the edges in ascending order of the operator
/// modulated and then of the one that modulates and the carriers in ascending
/// order: the orders of the sums of MATH §8.2, so that two graphs of the same
/// edges and carriers are equal however they were given.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Graph {
    /// N.
    operators: usize,
    /// The edges, in their order.
    edges: Vec<Edge>,
    /// The carriers' indices, ascending.
    carriers: Vec<usize>,
}

impl Graph {
    /// The graph of `operators` operators with these edges and carriers, in
    /// any order and each any number of times: `None` if one names an operator
    /// beyond the graph, or if two edges of different kinds join the same
    /// operators in the same direction.
    #[must_use]
    pub fn new(
        operators: usize,
        edges: impl IntoIterator<Item = Edge>,
        carriers: impl IntoIterator<Item = usize>,
    ) -> Option<Self> {
        let graph = Self::ordered(operators, edges.into_iter().collect(), carriers.into_iter().collect());
        let within = |operator: usize| operator < operators;
        let unique = graph
            .edges
            .windows(2)
            .all(|pair| (pair[0].to, pair[0].from) != (pair[1].to, pair[1].from));
        (unique
            && graph.edges.iter().all(|edge| within(edge.from) && within(edge.to))
            && graph.carriers.iter().all(|&carrier| within(carrier)))
        .then_some(graph)
    }

    /// The graph of six operators of one of the DX7's algorithms, as the chart
    /// printed on the instruments' panels draws it \[DX7-PANEL\], with its
    /// feedback edge where `feedback` is true: a feedback edge whose FB is 0 is
    /// not in the graph (MATH §8.1). The feedback edge is a self-loop, but in
    /// algorithms 4 and 6, whose edges from OP4 and from OP5 to OP6 close loops
    /// of three operators and of two.
    #[must_use]
    pub fn of_algorithm(algorithm: Algorithm, feedback: bool) -> Self {
        let (modulations, (from, to), carriers) = ALGORITHMS[usize::from(algorithm.step())];
        let edges = modulations
            .iter()
            .map(|&(from, to)| edge(from, to, Kind::Modulation))
            .chain(feedback.then_some(edge(from, to, Kind::Feedback)));
        Self::ordered(
            6,
            edges.collect(),
            carriers.iter().map(|&number| index(number)).collect(),
        )
    }

    /// The graph of these edges and carriers, put in their orders, each once.
    fn ordered(operators: usize, mut edges: Vec<Edge>, mut carriers: Vec<usize>) -> Self {
        edges.sort_unstable_by_key(|edge| (edge.to, edge.from, edge.kind));
        edges.dedup();
        carriers.sort_unstable();
        carriers.dedup();
        Self {
            operators,
            edges,
            carriers,
        }
    }

    /// N, the number of operators.
    #[must_use]
    pub fn operators(&self) -> usize {
        self.operators
    }

    /// The edges, in ascending order of the operator modulated and then of the
    /// one that modulates.
    #[must_use]
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// The edges into an operator, in ascending order of the operator that
    /// modulates, the order in which its modulation adds them (MATH §8.2): an
    /// edge from itself among them where it modulates itself, and none for an
    /// index beyond the graph.
    #[must_use]
    pub fn edges_to(&self, operator: usize) -> &[Edge] {
        let start = self.edges.partition_point(|edge| edge.to < operator);
        let end = self.edges.partition_point(|edge| edge.to <= operator);
        &self.edges[start..end]
    }

    /// The carriers' indices, ascending, the order in which the voice adds
    /// their outputs (MATH §8.2).
    #[must_use]
    pub fn carriers(&self) -> &[usize] {
        &self.carriers
    }
}

/// The edge between two operators given by their numbers.
fn edge(from: u8, to: u8, kind: Kind) -> Edge {
    Edge {
        from: index(from),
        to: index(to),
        kind,
    }
}

/// The index of an operator given by its number, 1 or more.
fn index(number: u8) -> usize {
    usize::from(number) - 1
}

/// A row of the table of the algorithms: the modulations, each (from, to) by
/// the operators' numbers, in ascending order of the operator modulated and
/// then of the one that modulates; the feedback edge; and the carriers'
/// numbers, ascending.
type Row = (&'static [(u8, u8)], (u8, u8), &'static [u8]);

/// The 32 algorithms: the chart printed on the panels of the DX7 and of the DX7II
/// \[DX7-PANEL\], read from a photograph of a DX7II-D's: boxes numbered by
/// their operators, a line from a modulator down to each operator it
/// modulates, a loop from an operator's output back to an input for the
/// feedback, and the carriers joined by the line beneath them. Two encodings
/// made apart from the chart agree with it in all 32, \[MSFA\]'s flags and the
/// OPS's algorithm ROM as \[SHIRRIFF-ALG\] reads it from the die, which
/// tests/algorithms.rs checks (docs/VERIFICATION.md V16).
static ALGORITHMS: [Row; 32] = [
    (&[(2, 1), (4, 3), (5, 4), (6, 5)], (6, 6), &[1, 3]),
    (&[(2, 1), (4, 3), (5, 4), (6, 5)], (2, 2), &[1, 3]),
    (&[(2, 1), (3, 2), (5, 4), (6, 5)], (6, 6), &[1, 4]),
    (&[(2, 1), (3, 2), (5, 4), (6, 5)], (4, 6), &[1, 4]),
    (&[(2, 1), (4, 3), (6, 5)], (6, 6), &[1, 3, 5]),
    (&[(2, 1), (4, 3), (6, 5)], (5, 6), &[1, 3, 5]),
    (&[(2, 1), (4, 3), (5, 3), (6, 5)], (6, 6), &[1, 3]),
    (&[(2, 1), (4, 3), (5, 3), (6, 5)], (4, 4), &[1, 3]),
    (&[(2, 1), (4, 3), (5, 3), (6, 5)], (2, 2), &[1, 3]),
    (&[(2, 1), (3, 2), (5, 4), (6, 4)], (3, 3), &[1, 4]),
    (&[(2, 1), (3, 2), (5, 4), (6, 4)], (6, 6), &[1, 4]),
    (&[(2, 1), (4, 3), (5, 3), (6, 3)], (2, 2), &[1, 3]),
    (&[(2, 1), (4, 3), (5, 3), (6, 3)], (6, 6), &[1, 3]),
    (&[(2, 1), (4, 3), (5, 4), (6, 4)], (6, 6), &[1, 3]),
    (&[(2, 1), (4, 3), (5, 4), (6, 4)], (2, 2), &[1, 3]),
    (&[(2, 1), (3, 1), (5, 1), (4, 3), (6, 5)], (6, 6), &[1]),
    (&[(2, 1), (3, 1), (5, 1), (4, 3), (6, 5)], (2, 2), &[1]),
    (&[(2, 1), (3, 1), (4, 1), (5, 4), (6, 5)], (3, 3), &[1]),
    (&[(2, 1), (3, 2), (6, 4), (6, 5)], (6, 6), &[1, 4, 5]),
    (&[(3, 1), (3, 2), (5, 4), (6, 4)], (3, 3), &[1, 2, 4]),
    (&[(3, 1), (3, 2), (6, 4), (6, 5)], (3, 3), &[1, 2, 4, 5]),
    (&[(2, 1), (6, 3), (6, 4), (6, 5)], (6, 6), &[1, 3, 4, 5]),
    (&[(3, 2), (6, 4), (6, 5)], (6, 6), &[1, 2, 4, 5]),
    (&[(6, 3), (6, 4), (6, 5)], (6, 6), &[1, 2, 3, 4, 5]),
    (&[(6, 4), (6, 5)], (6, 6), &[1, 2, 3, 4, 5]),
    (&[(3, 2), (5, 4), (6, 4)], (6, 6), &[1, 2, 4]),
    (&[(3, 2), (5, 4), (6, 4)], (3, 3), &[1, 2, 4]),
    (&[(2, 1), (4, 3), (5, 4)], (5, 5), &[1, 3, 6]),
    (&[(4, 3), (6, 5)], (6, 6), &[1, 2, 3, 5]),
    (&[(4, 3), (5, 4)], (5, 5), &[1, 2, 3, 6]),
    (&[(6, 5)], (6, 6), &[1, 2, 3, 4, 5]),
    (&[], (6, 6), &[1, 2, 3, 4, 5, 6]),
];
