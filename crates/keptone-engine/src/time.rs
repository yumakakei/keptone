// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! Instants (docs/MATH.md §0.1, §0.3): an engine sample and a binary64 fraction
//! of one, at which every law that runs in segments starts its segments, and
//! from which it counts their time.

/// An instant of MATH §0.1: an engine sample n and a fraction η of one, from 0
/// up to 1, at t = (n + η)/fₑ. Instants order by their samples and then by
/// their fractions, as their times do.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Instant {
    /// The engine sample n.
    sample: u64,
    /// The fraction η of a sample after it, 0 ≤ η < 1.
    fraction: f64,
}

/// A count of samples below 2⁵³ as a binary64, exactly; a larger one within u
/// of itself.
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "a u64 rounds to the nearest binary64, exactly below 2⁵³"
)]
fn samples(count: u64) -> f64 {
    count as f64
}

impl Instant {
    /// The instant of an engine sample itself, η = 0, at which the events of that
    /// sample act (MATH §0.3).
    #[must_use]
    pub const fn at(sample: u64) -> Self {
        Self { sample, fraction: 0.0 }
    }

    /// The instant a fraction of a sample after it, if the fraction lies from 0
    /// up to 1.
    #[must_use]
    pub fn new(sample: u64, fraction: f64) -> Option<Self> {
        (0.0..1.0).contains(&fraction).then_some(Self { sample, fraction })
    }

    /// The engine sample n.
    #[must_use]
    pub const fn sample(self) -> u64 {
        self.sample
    }

    /// The fraction η.
    #[must_use]
    pub const fn fraction(self) -> f64 {
        self.fraction
    }

    /// The samples from an earlier instant to this one, (n − n₀) + (η − η₀),
    /// below 0 where this one is the earlier.
    ///
    /// n − n₀ is exact below 2⁵³ samples, η − η₀ within u/2 of itself, the two
    /// fractions lying from 0 up to 1, and the sum within u of itself: within
    /// u(|Δ| + 1/2) of the exact count Δ below 2⁵³ samples, which last
    /// centuries at every rate.
    #[must_use]
    pub fn samples_since(self, earlier: Self) -> f64 {
        let whole = if self.sample >= earlier.sample {
            samples(self.sample - earlier.sample)
        } else {
            -samples(earlier.sample - self.sample)
        };
        whole + (self.fraction - earlier.fraction)
    }

    /// The instant a count of samples of 0 or more after this one, or none where
    /// it lies beyond the last sample that a u64 counts, or the count is not
    /// finite: a law that never ends there has no instant of its end.
    ///
    /// η + the count rounds within u of itself, and its floor and its part are
    /// exact: the instant lies within u(η + count) samples of the exact one.
    #[must_use]
    pub fn after(self, count: f64) -> Option<Self> {
        let total = self.fraction + count.max(0.0);
        let whole = total.floor();
        if !whole.is_finite() || whole >= 18_446_744_073_709_551_616.0 {
            return None;
        }
        let fraction = total - whole;
        whole_samples(whole)
            .and_then(|whole| self.sample.checked_add(whole))
            .map(|sample| Self { sample, fraction })
    }
}

/// A whole count of samples of 0 up to 2⁶⁴ as a u64.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole binary64 of [0, 2⁶⁴) converts exactly"
)]
fn whole_samples(whole: f64) -> Option<u64> {
    (whole >= 0.0).then_some(whole as u64)
}
