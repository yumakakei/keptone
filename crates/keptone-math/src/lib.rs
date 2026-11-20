// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Keptone's reference functions, scalar and in binary64.
//!
//! The numerical methods that docs/MATH.md specifies, each a function in plain
//! Rust that the engine, the editor and the tests share: those of milestones M1
//! and M3 (docs/DESIGN.md §16). The elementary functions of MATH §12, which the
//! engine and the tools take in place of the system's libm:
//!
//! - [`sin2pi`] and [`cos2pi`], the sine and cosine of a phase in turns, within
//!   1 ulp of the correctly rounded result;
//! - [`sinc`], within 2 ulp, and its derivative [`sinc_prime`], within 2 ulp
//!   where |u| < 1/2 and within an absolute 9 · 2⁻⁵³ elsewhere;
//! - [`exp2`], the exponential of base 2, and [`exp2m1`], 2^z − 1 without the
//!   cancellation near 0, each within 1 ulp;
//! - [`log2`], the logarithm of base 2, and [`log2p1`], log₂(1 + x) without the
//!   rounding of 1 + x, each within 1 ulp;
//!
//! the closed forms of antiderivative antialiasing of MATH §10, in [`adaa`]:
//! [`adaa::first_order`], within 9 · 2⁻⁵³ absolute, and
//! [`adaa::second_order`], within 5.5 · 10⁻¹³ absolute for the default of
//! `adaa.tau2`; and the feedback operator of MATH §9: the solver of Kepler's
//! equation, [`kepler::solve`], whose residual is within 4u max(1, |Φ|), and
//! the coefficients of its Fourier series, the Kapteyn series, by a DFT and in
//! tables of Chebyshev series in β, with the bounds of their tail, in
//! [`kapteyn`]; and the error-free transformations that these are built on and
//! that the engine shares, in [`eft`].
//!
//! These are the portable forms, which every vector form in `keptone-kernels`
//! must agree with to within its accuracy contract (MATH §0.2). They use
//! `f64::mul_add` where a fused operation is the more accurate, and fixed orders
//! of summation, and they never change the floating-point environment. Their
//! polynomials were designed by Sollya with binary64 coefficients and their
//! approximation errors certified ([`coefficients`] says by how much), so that
//! what remains of each contract is the rounding of the evaluation, which each
//! function's documentation bounds to first order in u = 2⁻⁵³ (Horner's scheme
//! with fused steps as in \[HIGHAM\] §5.1), with the constants that
//! `scripts/rounding_bounds.py` computes from the coefficients. The tests
//! compare them with CORE-MATH's correctly rounded functions and with references
//! computed by mpmath.
#![forbid(unsafe_code)]

pub mod adaa;
pub mod coefficients;
pub mod eft;
mod exponential;
pub mod kapteyn;
pub mod kepler;
mod logarithm;
mod sinc;
mod trig;

pub use exponential::{exp2, exp2m1};
pub use logarithm::{log2, log2p1};
pub use sinc::{sinc, sinc_prime};
pub use trig::{cos2pi, sin2pi};
