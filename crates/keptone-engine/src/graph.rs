// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The order in which a voice evaluates its operators (docs/MATH.md §8.2): a
//! graph's strongly connected components, each an operator on no loop, an
//! operator that modulates itself, or operators on loops among them, in an
//! order in which each comes after its operators' modulators.
//!
//! Any graph has them, whatever its operators and its loops (MATH §8.4); the
//! DX7's algorithms are graphs like any other (`keptone_model::graph`).

use std::collections::BinaryHeap;

use keptone_model::graph::Graph;

/// A strongly connected component of a graph (MATH §8.2): one operator on no
/// loop, one that modulates itself, or several on loops among them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Component {
    /// An operator on no loop, which its modulators' outputs give (MATH §10).
    Operator(usize),
    /// An operator that modulates itself and is on no loop with another,
    /// which MATH §9 solves.
    SelfLoop(usize),
    /// Operators on loops among them, ascending, which MATH §9 solves
    /// together; any of them may also modulate itself.
    Loop(Vec<usize>),
}

impl Component {
    /// Its operators' indices, ascending.
    #[must_use]
    pub fn operators(&self) -> &[usize] {
        match self {
            Self::Operator(k) | Self::SelfLoop(k) => core::slice::from_ref(k),
            Self::Loop(operators) => operators,
        }
    }
}

/// A graph's strongly connected components in an order of evaluation (MATH
/// §8.2): each after the components of its operators' modulators. Among the
/// components whose modulators all come before them, the one that holds the
/// highest operator comes first, as the DX7 computes OP6 first, so that the
/// graph of a DX7 algorithm is evaluated from OP6 down to OP1, a loop where
/// its highest operator would be.
///
/// The components come from Tarjan's algorithm, run without recursion, so
/// that no graph is too large for the thread's stack, and their order from
/// Kahn's; each takes a time and a memory linear in the operators and the
/// edges, but for the logarithm of the operators that the choice of the
/// highest costs. The kinds of the edges do not matter here.
#[must_use]
pub fn components(graph: &Graph) -> Vec<Component> {
    let (component, count) = strongly_connected(graph);
    let mut members = vec![Vec::new(); count];
    for (operator, &c) in component.iter().enumerate() {
        members[c].push(operator);
    }
    // What each component modulates, and how many edges from other components
    // each still waits for.
    let mut modulated = vec![Vec::new(); count];
    let mut waiting = vec![0_usize; count];
    for edge in graph.edges() {
        let (from, to) = (component[edge.from], component[edge.to]);
        if from != to {
            modulated[from].push(to);
            waiting[to] += 1;
        }
    }
    // A component's highest operator is its last, as members lists them in
    // ascending order, and none is empty.
    let highest = |c: usize| members[c].last().copied().unwrap_or_default();
    let mut ready: BinaryHeap<(usize, usize)> = (0..count)
        .filter(|&c| waiting[c] == 0)
        .map(|c| (highest(c), c))
        .collect();
    let mut order = Vec::with_capacity(count);
    while let Some((_, c)) = ready.pop() {
        order.push(c);
        for &d in &modulated[c] {
            waiting[d] -= 1;
            if waiting[d] == 0 {
                ready.push((highest(d), d));
            }
        }
    }
    order
        .into_iter()
        .map(|c| {
            let operators = core::mem::take(&mut members[c]);
            if let [k] = *operators.as_slice() {
                if graph.edges_to(k).iter().any(|edge| edge.from == k) {
                    Component::SelfLoop(k)
                } else {
                    Component::Operator(k)
                }
            } else {
                Component::Loop(operators)
            }
        })
        .collect()
}

/// Tarjan's algorithm for the strongly connected components, with a stack of
/// its own in place of recursion: each operator's component, numbered as the
/// algorithm closes them, and their count.
fn strongly_connected(graph: &Graph) -> (Vec<usize>, usize) {
    let operators = graph.operators();
    let mut successors = vec![Vec::new(); operators];
    for edge in graph.edges() {
        successors[edge.from].push(edge.to);
    }
    let unvisited = usize::MAX;
    let mut index = vec![unvisited; operators];
    let mut low = vec![0; operators];
    let mut on_stack = vec![false; operators];
    let mut stack = Vec::new();
    let mut component = vec![0; operators];
    let mut count = 0;
    let mut next = 0;
    // The operators whose successors are being visited, each with the position
    // of the next successor to visit.
    let mut frames: Vec<(usize, usize)> = Vec::new();
    for root in 0..operators {
        if index[root] != unvisited {
            continue;
        }
        index[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on_stack[root] = true;
        frames.push((root, 0));
        while let Some(frame) = frames.last_mut() {
            let v = frame.0;
            if let Some(&w) = successors[v].get(frame.1) {
                frame.1 += 1;
                if index[w] == unvisited {
                    index[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    frames.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
            } else {
                frames.pop();
                if let Some(&(parent, _)) = frames.last() {
                    low[parent] = low[parent].min(low[v]);
                }
                if low[v] == index[v] {
                    while let Some(w) = stack.pop() {
                        on_stack[w] = false;
                        component[w] = count;
                        if w == v {
                            break;
                        }
                    }
                    count += 1;
                }
            }
        }
    }
    (component, count)
}
