// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The operator graph (docs/MATH.md §8.4): any graph within its operators is
//! one, its edges and carriers held once and in their orders.

use keptone_model::graph::{Edge, Graph, Kind};

/// An edge between operators of these indices.
fn edge(from: usize, to: usize, kind: Kind) -> Edge {
    Edge { from, to, kind }
}

#[test]
fn a_graph_holds_its_edges_and_carriers_once_in_their_orders() {
    let (m, f) = (Kind::Modulation, Kind::Feedback);
    let given = [
        edge(3, 0, m),
        edge(1, 0, m),
        edge(3, 0, m),
        edge(0, 3, f),
        edge(2, 2, f),
    ];
    let graph = Graph::new(4, given, [3, 0, 3]).expect("within four operators");
    assert_eq!(graph.operators(), 4);
    assert_eq!(
        graph.edges(),
        [edge(1, 0, m), edge(3, 0, m), edge(2, 2, f), edge(0, 3, f)]
    );
    assert_eq!(graph.carriers(), [0, 3]);
    assert_eq!(graph.edges_to(0), [edge(1, 0, m), edge(3, 0, m)]);
    assert_eq!(graph.edges_to(1), []);
    assert_eq!(graph.edges_to(2), [edge(2, 2, f)]);
    assert_eq!(graph.edges_to(3), [edge(0, 3, f)]);
    assert_eq!(graph.edges_to(4), []);
    let reversed = Graph::new(4, graph.edges().iter().rev().copied(), [3, 0]);
    assert_eq!(reversed, Some(graph));
}

#[test]
fn a_graph_takes_any_edges_within_its_operators() {
    let (m, f) = (Kind::Modulation, Kind::Feedback);
    // Edges upward and downward, a loop of modulations, a loop through a
    // feedback edge, operators modulating themselves, a carrier that
    // modulates, an operator that is heard by no carrier: what a graph drawn
    // by hand may hold.
    let edges = [
        edge(0, 1, m),
        edge(1, 0, m),
        edge(0, 0, m),
        edge(2, 3, m),
        edge(3, 4, m),
        edge(4, 2, f),
        edge(5, 5, f),
        edge(6, 6, f),
        edge(5, 0, m),
    ];
    let graph = Graph::new(8, edges, [0, 4]).expect("within eight operators");
    assert_eq!(graph.edges().len(), edges.len());
    assert!(Graph::new(1, [edge(0, 0, m)], []).is_some());
    assert!(Graph::new(100, [], 0..100).is_some());
}

#[test]
fn a_graph_refuses_what_lies_beyond_it_and_an_edge_of_two_kinds() {
    let (m, f) = (Kind::Modulation, Kind::Feedback);
    assert_eq!(Graph::new(3, [edge(3, 0, m)], []), None);
    assert_eq!(Graph::new(3, [edge(0, 3, f)], []), None);
    assert_eq!(Graph::new(3, [], [3]), None);
    assert_eq!(Graph::new(0, [], [0]), None);
    assert_eq!(Graph::new(3, [edge(1, 0, m), edge(1, 0, f)], []), None);
    assert!(Graph::new(3, [edge(1, 0, m), edge(0, 1, f)], []).is_some());
}
