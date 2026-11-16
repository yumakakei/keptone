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
//! Rust that the engine, the editor and the tests share:
//!
//! - the sine and cosine of a phase in turns, sinc and its derivative, and the
//!   exponential of base 2 (MATH §12), each within a stated bound of the
//!   correctly rounded result;
//! - the closed forms of first- and second-order antiderivative antialiasing for
//!   the sine (MATH §10);
//! - the solver of Kepler's equation for the feedback operator, and the tables of
//!   its Kapteyn series (MATH §9).
//!
//! These are the portable forms, which every vector form in `keptone-kernels`
//! must agree with to within its accuracy contract (MATH §0.2). They use
//! `f64::mul_add` where a fused operation is the more accurate, and fixed orders
//! of summation, and they never change the floating-point environment.
//!
//! Nothing is here yet: the crate is filled in milestone M1 (docs/DESIGN.md
//! §16).
#![forbid(unsafe_code)]
