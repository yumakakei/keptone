// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The Kapteyn series of the feedback operator (docs/MATH.md §9.5): for β < 1,
//! the solution of Θ − β sin Θ = Φ has sin Θ = Σₙ aₙ(β) sin nΦ, n ≥ 1, with
//! aₙ(β) = 2Jₙ(nβ)/(nβ) \[KAPTEYN\], the operator's exact Fourier series.
//!
//! The coefficients come from a real DFT of sin Θ at equispaced Φ, Θ from
//! [`crate::kepler::solve`] \[TW-TRAP\]: every sample the DFT sums is at most 1
//! in size, where Bessel's integral at large n is ill-conditioned
//! \[BOOST-TRAP\]. Kapteyn's inequality, |Jₙ(nβ)| ≤ Ω(β)ⁿ \[DLMF-10.14
//! (10.14.8)\], bounds the series' tail, and so the number of terms to keep. A
//! [`Table`] holds each aₙ as a Chebyshev series in β on [0, βₘₐₓ], built at
//! activation and evaluated by Clenshaw's recurrence without allocating.

use core::f64::consts::LOG2_E;

use crate::eft::{two_product, two_sum};
use crate::exponential::exp2;
use crate::kepler::{DEFAULT_MAX_ITERATIONS, solve};
use crate::trig::{cos2pi, sin2pi};

/// u = 2⁻⁵³.
const U: f64 = f64::EPSILON / 2.0;

/// An upper bound of Ω(β) = β e^√(1 − β²)/(1 + √(1 − β²)) for β in [0, 1]:
/// Ω is in [0, 1) there and 1 at β = 1. The value is computed within a few u
/// and raised by 2⁻⁵⁰ of itself, so that it is not below Ω. Outside [0, 1], or
/// for NaN, the result is NaN.
#[must_use]
pub fn omega(beta: f64) -> f64 {
    if !(0.0..=1.0).contains(&beta) {
        return f64::NAN;
    }
    let s = (-beta).mul_add(beta, 1.0).sqrt();
    let value = beta * exp2(s * LOG2_E) / (1.0 + s);
    value.mul_add(2.0_f64.powi(-50), value)
}

/// An upper bound of the tail, the sum of |aₖ(β)| over k > n, for β in [0, 1):
/// by Kapteyn's inequality |aₖ| ≤ 2Ωᵏ/(kβ), whose sum over k > n is at most
/// 2Ωⁿ⁺¹/((n + 1)β(1 − Ω)). At β = 0 the series is sin Φ alone: the tail is 1
/// for n = 0 and 0 after. For β ≥ 1 the result is +∞; for NaN, NaN.
#[must_use]
pub fn tail_bound(beta: f64, n: u64) -> f64 {
    if beta == 0.0 {
        return if n == 0 { 1.0 } else { 0.0 };
    }
    let omega = omega(beta);
    if omega.is_nan() || omega >= 1.0 {
        return if omega.is_nan() { f64::NAN } else { f64::INFINITY };
    }
    let next = n.saturating_add(1);
    // The bound is raised by a few u of itself for the roundings below.
    let bound = 2.0 * power(omega, next) / (to_f64(next) * beta * (1.0 - omega));
    bound.mul_add(2.0_f64.powi(-48), bound)
}

/// The least number of terms N whose tail bound is below ε, `N_K` of MATH §9.5
/// (ε = `feedback.series_epsilon`), by doubling and bisection on
/// [`tail_bound`]; `None` for β ≥ 1, where the tail does not shrink, or for
/// ε ≤ 0 or NaN.
#[must_use]
pub fn terms(beta: f64, epsilon: f64) -> Option<u64> {
    if epsilon.is_nan() || epsilon <= 0.0 || !(0.0..1.0).contains(&beta) {
        return None;
    }
    if tail_bound(beta, 0) < epsilon {
        return Some(0);
    }
    let mut high = 1_u64;
    while tail_bound(beta, high) >= epsilon {
        high = high.checked_mul(2)?;
    }
    let mut low = high / 2;
    // tail_bound(low) ≥ ε > tail_bound(high).
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if tail_bound(beta, middle) < epsilon {
            high = middle;
        } else {
            low = middle;
        }
    }
    Some(high)
}

/// The first `count` coefficients, a₁, a₂, …, by the DFT of sin Θ at K
/// equispaced phases, K the least power of two that is at least `points`,
/// 4 · count (MATH §9.5) and 8: with Θ = 2πτ and Φ = 2πm/K,
///
/// aₙ = (4/K) Σ sin 2πτₘ · sin 2πnm/K, over m from 1 to K/2 − 1,
///
/// sin Θ being odd in Φ. The phases m/K and nm/K mod 1 are exact, and each sum
/// is kept in double-double, so that what remains is the error of each
/// sin 2πτₘ, a few u, which the sum averages, and the rounding of aₙ. The
/// aliased terms the DFT adds, of index K − n and beyond, are no larger than
/// the coefficient of index 3 · count, below ε³ when count is N of [`terms`]. It
/// allocates: an activation's work. `None` if β is not in [0, 1) or K would
/// exceed 2³¹.
#[must_use]
pub fn coefficients(beta: f64, count: u32, points: u32) -> Option<Vec<f64>> {
    if !(0.0..1.0).contains(&beta) {
        return None;
    }
    let k = points.max(count.checked_mul(4)?).max(8).checked_next_power_of_two()?;
    if k > 1 << 31 {
        return None;
    }
    let size = f64::from(k);
    let mask = k - 1;
    let sines: Vec<f64> = (0..k).map(|j| sin2pi(f64::from(j) / size)).collect();
    let half = k / 2;
    let samples: Vec<f64> = (0..half)
        .map(|m| sin2pi(solve(f64::from(m) / size, beta, DEFAULT_MAX_ITERATIONS).turns))
        .collect();
    let scale = 4.0 / size;
    let out = (1..=count)
        .map(|n| {
            let (mut hi, mut lo) = (0.0, 0.0);
            let mut index = 0_u32;
            for &sample in &samples[1..] {
                index = index.wrapping_add(n) & mask;
                let (p, p_lo) = two_product(sample, sines[usize_from(index)]);
                let (s, s_lo) = two_sum(hi, p);
                hi = s;
                lo += s_lo + p_lo;
            }
            (hi + lo) * scale
        })
        .collect();
    Some(out)
}

/// The first N Kapteyn coefficients as Chebyshev series in β on [0, βₘₐₓ]
/// (MATH §9.5), N = [`terms`] for βₘₐₓ = `feedback.series_beta_max` and
/// ε = `feedback.series_epsilon`.
///
/// [`Table::new`] takes the DFT of [`coefficients`] at the M + 1
/// Chebyshev–Lobatto points of [0, βₘₐₓ], βⱼ = βₘₐₓ (1 + cos πj/M)/2, M a
/// power of two from 16, doubled until, for every n, the upper half of aₙ's
/// Chebyshev coefficients lies below its floor; doubling keeps every point it
/// had. The floor is ε/M, ε shared out, or the noise of the values the series
/// interpolates, if that is larger: each value carries the DFT's average of the
/// samples' errors, about u each, so σ = 4u/√(2K) if they are independent, and
/// its own rounding, at most 0.6u max|aₙ| in standard deviation; a coefficient
/// carries 2σ/√M of it, and the floor is four times that. Each series is then
/// cut after its last coefficient above the floor. Coefficients beyond N are
/// below ε for every β up to βₘₐₓ (Ω grows with β), and
/// [`Table::coefficients`] gives 0 for them.
#[derive(Debug, Clone)]
pub struct Table {
    beta_max: f64,
    /// For each n from 1, the Chebyshev coefficients of aₙ in x = 2β/βₘₐₓ − 1,
    /// the first (and the M-th, if kept) halved.
    series: Vec<Vec<f64>>,
}

impl Table {
    /// The table for β in [0, βₘₐₓ] and ε, or `None` if βₘₐₓ is not in
    /// [0, 1), ε is not positive, or the sizes the table needs exceed what the
    /// DFT and the doubling allow (2³¹ phases, 2¹⁶ points). It allocates, and
    /// takes about M · (K/2) solves of Kepler's equation and M · N · K/2
    /// products: an activation's work, for the worker thread.
    #[must_use]
    pub fn new(beta_max: f64, epsilon: f64) -> Option<Self> {
        let count = u32::try_from(terms(beta_max, epsilon)?).ok()?.max(1);
        let points = count.checked_mul(4)?.max(8).checked_next_power_of_two()?;
        // The DFT's noise in each value, if the samples' errors are independent.
        let dft_noise = 4.0 * U / (2.0 * f64::from(points)).sqrt();
        let node = |j: u32, nodes: u32| {
            let x = cos2pi(f64::from(j) / (2.0 * f64::from(nodes)));
            coefficients(0.5 * beta_max * (1.0 + x), count, points)
        };
        let mut nodes = 16_u32;
        let mut values: Vec<Vec<f64>> = (0..=nodes).map(|j| node(j, nodes)).collect::<Option<_>>()?;
        loop {
            if let Some(series) = chebyshev(&values, nodes, epsilon, dft_noise) {
                return Some(Self { beta_max, series });
            }
            if nodes >= 1 << 16 {
                return None;
            }
            let doubled = nodes * 2;
            let mut old = values.into_iter();
            values = (0..=doubled)
                .map(|j| if j % 2 == 0 { old.next() } else { node(j, doubled) })
                .collect::<Option<_>>()?;
            nodes = doubled;
        }
    }

    /// βₘₐₓ, the end of the table's domain.
    #[must_use]
    pub fn beta_max(&self) -> f64 {
        self.beta_max
    }

    /// N: the number of coefficients the table holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.series.len()
    }

    /// Whether the table holds no coefficient, which it never does.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }

    /// The Chebyshev coefficients the table holds in all: its size, in binary64.
    #[must_use]
    pub fn size(&self) -> usize {
        self.series.iter().map(Vec::len).sum()
    }

    /// The first `out.len()` coefficients at β into `out`, those beyond N as 0,
    /// by Clenshaw's recurrence on each series, without allocating; the number
    /// of them the table held, or `None`, `out` untouched, if β is not in
    /// [0, βₘₐₓ].
    pub fn coefficients(&self, beta: f64, out: &mut [f64]) -> Option<usize> {
        if !(0.0..=self.beta_max).contains(&beta) {
            return None;
        }
        let x = if self.beta_max > 0.0 {
            2.0 * beta / self.beta_max - 1.0
        } else {
            0.0
        };
        let held = out.len().min(self.series.len());
        for (slot, series) in out.iter_mut().zip(&self.series) {
            *slot = clenshaw(series, x);
        }
        out[held..].fill(0.0);
        Some(held)
    }
}

/// The Chebyshev series of every aₙ from its values at the M + 1
/// Chebyshev–Lobatto points, by the DCT-I in double-double, each cut after its
/// last coefficient above its floor ([`Table`]); `None` while some upper half
/// is not below its floor yet.
fn chebyshev(values: &[Vec<f64>], nodes: u32, epsilon: f64, dft_noise: f64) -> Option<Vec<Vec<f64>>> {
    let size = f64::from(nodes);
    let period = 2 * nodes;
    let cosines: Vec<f64> = (0..period).map(|i| cos2pi(f64::from(i) / f64::from(period))).collect();
    let weight = |j: u32| if j == 0 || j == nodes { 0.5 } else { 1.0 };
    let mut series = Vec::with_capacity(values[0].len());
    for n in 0..values[0].len() {
        let scale = values.iter().map(|v| v[n].abs()).fold(0.0, f64::max);
        let noise = 2.0 * (0.6 * U).mul_add(scale, dft_noise) / size.sqrt();
        let floor = (epsilon / size).max(4.0 * noise);
        let mut c: Vec<f64> = (0..=nodes)
            .map(|k| {
                let (mut hi, mut lo) = (0.0, 0.0);
                for (j, v) in (0..=nodes).zip(values) {
                    let angle = (j * k) % period;
                    let (p, p_lo) = two_product(weight(j) * v[n], cosines[usize_from(angle)]);
                    let (s, s_lo) = two_sum(hi, p);
                    hi = s;
                    lo += s_lo + p_lo;
                }
                (hi + lo) * 2.0 / size * weight(k)
            })
            .collect();
        if c[usize_from(nodes / 2)..].iter().any(|x| x.abs() > floor) {
            return None;
        }
        let last = c.iter().rposition(|x| x.abs() > floor).map_or(1, |i| i + 1);
        c.truncate(last);
        series.push(c);
    }
    Some(series)
}

/// Σ cₖ Tₖ(x), c₀ already halved, by Clenshaw's recurrence.
fn clenshaw(c: &[f64], x: f64) -> f64 {
    let (mut next, mut after) = (0.0, 0.0);
    for &ck in c[1..].iter().rev() {
        let b = (2.0 * x).mul_add(next, ck) - after;
        after = next;
        next = b;
    }
    x.mul_add(next, c[0]) - after
}

/// xᵏ by squaring, for x in [0, 1], from basic operations alone.
fn power(x: f64, mut k: u64) -> f64 {
    let (mut result, mut base) = (1.0, x);
    while k > 0 {
        if k & 1 == 1 {
            result *= base;
        }
        base *= base;
        k >>= 1;
    }
    result
}

/// k as binary64: exact below 2⁵³, rounded above, where it serves a bound.
fn to_f64(k: u64) -> f64 {
    let high = u32::try_from(k >> 32).unwrap_or(u32::MAX);
    let low = u32::try_from(k & 0xffff_ffff).unwrap_or(u32::MAX);
    f64::from(high).mul_add(4_294_967_296.0, f64::from(low))
}

/// An index of u32 as usize, which holds it on every system Keptone runs on.
fn usize_from(i: u32) -> usize {
    usize::try_from(i).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::{clenshaw, omega, power, tail_bound, terms};

    #[test]
    fn bounds_at_the_ends() {
        assert_eq!(omega(0.0), 0.0);
        assert!((omega(1.0) - 1.0).abs() <= 4.0 * f64::EPSILON);
        assert!(omega(1.5).is_nan() && omega(-0.1).is_nan());
        assert_eq!(tail_bound(0.0, 0), 1.0);
        assert_eq!(tail_bound(0.0, 5), 0.0);
        assert_eq!(terms(0.0, 2.0_f64.powi(-53)), Some(1));
        assert_eq!(terms(1.0, 2.0_f64.powi(-53)), None);
        assert_eq!(terms(0.5, 0.0), None);
    }

    #[test]
    fn helpers() {
        assert_eq!(power(0.5, 10), 0.5_f64.powi(10));
        assert_eq!(power(0.9, 0), 1.0);
        // T0 + T1 + T2 at x: 1 + x + (2x² − 1).
        for x in [-1.0, -0.3, 0.0, 0.7, 1.0] {
            let expected = 1.0 + x + (2.0 * x * x - 1.0);
            assert!((clenshaw(&[0.5 * 2.0, 1.0, 1.0], x) - expected).abs() <= 1e-15, "{x}");
        }
    }
}
