// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! sinc and its derivative against references that mpmath computed well past
//! binary64 (`tests/data/sinc.txt`, written by `scripts/sinc_references.py`):
//! sinc within 2 ulp of the correctly rounded result, sinc′ within 2 ulp of it
//! where |u| < 1/2 and within an absolute 9 · 2⁻⁵³ of its value elsewhere
//! (docs/MATH.md §12.2, §16).
//!
//! Each reference is a double-double whose high part is the correctly rounded
//! result, so that an error in ulp is measured as `oracle.rs` measures it
//! against CORE-MATH, and an absolute error against the value itself. Where the
//! function is exactly 0, at the integers for sinc and at 0 for sinc′, the
//! result must be exactly 0, and +0 for sinc, as it documents.

mod common;

use common::ulp_error;
use keptone_math::{sinc, sinc_prime};

/// One line of the references: u, sinc u and sinc′ u, each a double-double.
struct Reference {
    u: f64,
    sinc: (f64, f64),
    sinc_prime: (f64, f64),
}

fn references() -> Vec<Reference> {
    let text = include_str!("data/sinc.txt");
    let number = |word: &str| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits"));
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let words: Vec<f64> = line.split(' ').map(number).collect();
            Reference {
                u: words[0],
                sinc: (words[1], words[2]),
                sinc_prime: (words[3], words[4]),
            }
        })
        .collect()
}

/// |y − (hi + lo)|: y − hi is exact where the two are close (Sterbenz's lemma),
/// and the subtraction of lo rounds at the order of u² of the result.
fn absolute_error(y: f64, (hi, lo): (f64, f64)) -> f64 {
    ((y - hi) - lo).abs()
}

/// The largest error found, and the input it was found at.
#[derive(Debug, Default)]
struct Worst {
    error: f64,
    input: f64,
}

impl Worst {
    fn take(&mut self, error: f64, input: f64) {
        if error > self.error {
            *self = Self { error, input };
        }
    }
}

#[test]
fn sinc_within_2_ulp() {
    let references = references();
    let mut worst = Worst::default();
    for reference in &references {
        let y = sinc(reference.u);
        if reference.sinc.0 == 0.0 {
            assert_eq!(y.to_bits(), 0, "sinc({:e}) = {y:e}, not +0", reference.u);
        } else {
            worst.take(ulp_error(y, reference.sinc.0), reference.u);
        }
    }
    println!(
        "sinc: worst {:.4} ulp at {:e}, over {} references",
        worst.error,
        worst.input,
        references.len()
    );
    assert!(worst.error <= 2.0, "sinc: {worst:?}");
}

#[test]
fn sinc_prime_within_its_bounds() {
    let references = references();
    let (mut below, mut above) = (Worst::default(), Worst::default());
    for reference in &references {
        let y = sinc_prime(reference.u);
        if reference.u.abs() >= 0.5 {
            above.take(absolute_error(y, reference.sinc_prime), reference.u);
        } else if reference.sinc_prime.0 == 0.0 {
            assert!(y == 0.0, "sinc'({:e}) = {y:e}, not 0", reference.u);
        } else {
            below.take(ulp_error(y, reference.sinc_prime.0), reference.u);
        }
    }
    let unit = 2.0_f64.powi(-53);
    println!(
        "sinc': worst {:.4} ulp below 1/2 at {:e}, worst {:.4} * 2^-53 absolute from 1/2 at {:e}, over {} references",
        below.error,
        below.input,
        above.error / unit,
        above.input,
        references.len()
    );
    assert!(below.error <= 2.0, "sinc' below 1/2: {below:?}");
    assert!(above.error <= 9.0 * unit, "sinc' from 1/2: {above:?}");
}
