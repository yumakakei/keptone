// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The Kapteyn coefficients against mpmath (docs/MATH.md §16): for n up to
//! 2000 and β on a grid, within 10⁻¹⁵ absolute of aₙ(β) = 2Jₙ(nβ)/(nβ), as
//! `scripts/kapteyn_references.py` computes it (`tests/data/kapteyn.txt`). The
//! DFT is measured up to β = 0.95, and the table of the defaults
//! (`feedback.series_beta_max` = 0.8, `feedback.series_epsilon` = 2⁻⁵³) at
//! points of its domain between those it was built from and at its end. The
//! same references hold Kapteyn's inequality and the tail bound to account.

use std::collections::BTreeMap;

use keptone_math::kapteyn::{Table, coefficients, omega, tail_bound, terms};

/// ε = 2⁻⁵³, the default of `feedback.series_epsilon`.
const EPSILON: f64 = f64::EPSILON / 2.0;

/// The largest n of the references.
const LARGEST: usize = 2000;

/// The references by β: aₙ for n from 1, those below 10⁻²⁰ left out, as 0.
fn references() -> BTreeMap<u64, Vec<f64>> {
    let mut by_beta: BTreeMap<u64, Vec<f64>> = BTreeMap::new();
    let bits = |word: &str| u64::from_str_radix(word, 16).expect("hexadecimal bits");
    for line in include_str!("data/kapteyn.txt").lines().filter(|l| !l.starts_with('#')) {
        let words: Vec<&str> = line.split(' ').collect();
        let n: usize = words[1].parse().expect("n");
        let values = by_beta.entry(bits(words[0])).or_default();
        assert_eq!(values.len() + 1, n, "the references come in order of n");
        values.push(f64::from_bits(bits(words[2])));
    }
    by_beta
}

/// The largest |computed − reference| over n up to 2000, and the n.
fn worst(computed: &[f64], reference: &[f64]) -> (f64, usize) {
    let mut worst = (0.0_f64, 0);
    for n in 0..LARGEST {
        let expected = reference.get(n).copied().unwrap_or(0.0);
        let got = computed.get(n).copied().unwrap_or(0.0);
        let error = (got - expected).abs();
        if error > worst.0 {
            worst = (error, n + 1);
        }
    }
    worst
}

#[test]
fn dft_within_1e_15() {
    let count = u32::try_from(LARGEST).expect("small");
    for (bits, reference) in references() {
        let beta = f64::from_bits(bits);
        let needed = terms(beta, EPSILON).expect("β < 1");
        let points = u32::try_from(needed.max(2000) * 4).expect("small");
        let computed = coefficients(beta, count, points).expect("the DFT");
        let (error, n) = worst(&computed, &reference);
        println!("DFT at beta = {beta}: N_K {needed}, worst error {error:.3e} at n = {n}");
        assert!(error <= 1e-15, "beta = {beta}: {error:e} at n = {n}");
    }
}

#[test]
fn table_within_1e_15() {
    let table = Table::new(0.8, EPSILON).expect("the table of the defaults");
    println!(
        "the table of the defaults: {} coefficients, {} Chebyshev coefficients",
        table.len(),
        table.size()
    );
    let mut out = vec![0.0; LARGEST];
    for (bits, reference) in references() {
        let beta = f64::from_bits(bits);
        if beta > table.beta_max() {
            assert_eq!(table.coefficients(beta, &mut out), None, "beta = {beta} is outside");
            continue;
        }
        let held = table.coefficients(beta, &mut out).expect("in the domain");
        let (error, n) = worst(&out, &reference);
        println!("table at beta = {beta}: {held} coefficients, worst error {error:.3e} at n = {n}");
        assert!(error <= 1e-15, "beta = {beta}: {error:e} at n = {n}");
    }
}

#[test]
fn kapteyn_inequality_and_the_tail_bound() {
    for (bits, reference) in references() {
        let beta = f64::from_bits(bits);
        let bound_omega = omega(beta);
        let mut power = 1.0;
        for (k, &a) in reference.iter().enumerate() {
            power *= bound_omega;
            let n = f64::from(u32::try_from(k + 1).expect("small"));
            assert!(
                a.abs() <= 2.0 * power / (n * beta),
                "Kapteyn's inequality at beta = {beta}, n = {n}"
            );
        }
        // The tail beyond n, from the references; those left out add below 10⁻²⁰ each.
        let left_out = 1e-20 * f64::from(u32::try_from(LARGEST - reference.len()).expect("small"));
        for n in [0_usize, 1, 2, 5, 10, 50, 100, 300] {
            if n >= reference.len() {
                break;
            }
            let tail: f64 = reference[n..].iter().map(|a| a.abs()).sum::<f64>() + left_out;
            let bound = tail_bound(beta, u64::try_from(n).expect("small"));
            assert!(tail <= bound, "beta = {beta}, n = {n}: tail {tail:e} > bound {bound:e}");
        }
    }
    // N_K of the defaults: 366, as the same bound gives it at 40 digits in mpmath.
    assert_eq!(terms(0.8, EPSILON), Some(366));
}
