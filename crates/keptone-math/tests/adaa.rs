// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! ADAA-1 and ADAA-2 against references that mpmath computed from the inputs
//! taken as exact (`tests/data/adaa1.txt` and `adaa2.txt`, written by
//! `scripts/adaa_references.py`): ADAA-1 within 9 · 2⁻⁵³ absolute, and ADAA-2
//! within the bound its documentation derives, for the thresholds 2⁻¹², 2⁻¹⁰
//! (the default of `adaa.tau2`) and 2⁻⁸ (docs/MATH.md §10, §16). The inputs
//! include h = Δ + Δ′ on either side of each threshold, so that both branches
//! of ADAA-2 are measured where they meet.

use keptone_math::adaa::{DEFAULT_TAU2, first_order, second_order};

/// u = 2⁻⁵³.
const U: f64 = 1.0 / 9_007_199_254_740_992.0;

/// The lines of a reference file, each number from its hexadecimal bits.
fn rows(text: &str) -> Vec<Vec<f64>> {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            line.split(' ')
                .map(|word| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits")))
                .collect()
        })
        .collect()
}

/// |y − (hi + lo)|: y − hi is exact where the two are close (Sterbenz's lemma).
fn absolute_error(y: f64, hi: f64, lo: f64) -> f64 {
    ((y - hi) - lo).abs()
}

/// The bound of `second_order`'s documentation for a threshold τ₂.
fn bound(tau2: f64) -> f64 {
    let quotient = 4.81 * U / tau2 + 4.35 * U;
    let expansion = 0.271 * tau2.powi(4) + 14.0 * U;
    quotient.max(expansion)
}

#[test]
fn first_order_within_9_units() {
    let rows = rows(include_str!("data/adaa1.txt"));
    let (mut worst, mut at) = (0.0_f64, (0.0, 0.0));
    for row in &rows {
        let error = absolute_error(first_order(row[0], row[1]), row[2], row[3]);
        if error > worst {
            (worst, at) = (error, (row[0], row[1]));
        }
    }
    println!(
        "ADAA-1: worst {:.3} * 2^-53 at x = {:e}, delta = {:e}, over {} references",
        worst / U,
        at.0,
        at.1,
        rows.len()
    );
    assert!(worst <= 9.0 * U, "ADAA-1: {worst:e} at {at:?}");
}

#[test]
fn second_order_within_its_bound() {
    let rows = rows(include_str!("data/adaa2.txt"));
    for tau2 in [2.0_f64.powi(-12), DEFAULT_TAU2, 2.0_f64.powi(-8)] {
        // The worst error of each branch, and where it is.
        let mut worst = [(0.0_f64, [0.0; 3]); 2];
        for row in &rows {
            let (x, delta, previous) = (row[0], row[1], row[2]);
            let error = absolute_error(second_order(x, delta, previous, tau2), row[3], row[4]);
            let h = delta + previous;
            let branch = usize::from(h.abs() < tau2 || h == 0.0);
            if error > worst[branch].0 {
                worst[branch] = (error, [x, delta, previous]);
            }
        }
        let [quotient, expansion] = worst;
        println!(
            "ADAA-2, tau2 = 2^{}: quotient worst {:.3e} at {:?}, expansion worst {:.3e} at {:?}, bound {:.3e}",
            tau2.log2(),
            quotient.0,
            quotient.1,
            expansion.0,
            expansion.1,
            bound(tau2)
        );
        assert!(
            quotient.0 <= bound(tau2),
            "ADAA-2 quotient, tau2 = {tau2:e}: {quotient:?}"
        );
        assert!(
            expansion.0 <= bound(tau2),
            "ADAA-2 expansion, tau2 = {tau2:e}: {expansion:?}"
        );
    }
}
