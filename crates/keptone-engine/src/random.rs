// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The random source (docs/MATH.md §6.3): `SplitMix64`, the generator kiral's
//! tests use, read at counters, so that a value needs no history and a part's
//! values depend on its own input alone. A stream seeded with x gives as its
//! value of index i `SplitMix64`'s i-th output from the state x, its finaliser
//! applied to x + iγ modulo 2⁶⁴; a part has three streams, of its LFO's
//! sample-and-hold values, of its voices' LFOs' and of its operators' random
//! phases, seeded from `lfo.seed`.

/// `SplitMix64`'s increment γ, 2⁶⁴ over the golden ratio, made odd.
pub const GAMMA: u64 = 0x9e37_79b9_7f4a_7c15;

/// `SplitMix64`'s finaliser.
const fn mix(state: u64) -> u64 {
    let z = (state ^ (state >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// The value of index i of the stream seeded with x (MATH §6.3): `SplitMix64`'s
/// i-th output from the state x, its finaliser applied to x + iγ modulo 2⁶⁴.
/// MATH counts the values from index 1.
#[must_use]
pub const fn value(seed: u64, index: u64) -> u64 {
    mix(seed.wrapping_add(index.wrapping_mul(GAMMA)))
}

/// The seeds of a part's three streams (MATH §6.3): of part j, 0 for the first,
/// the outputs 3j + 1, 3j + 2 and 3j + 3 of `SplitMix64` from `lfo.seed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Streams {
    /// The sample-and-hold values of the part's LFO.
    pub part: u64,
    /// Those of its voices' LFOs, under `lfo.scope = voice`.
    pub voices: u64,
    /// The random phases of its operators (MATH §2.5).
    pub phases: u64,
}

impl Streams {
    /// The streams of part j, from `lfo.seed`, the indices taken modulo 2⁶⁴.
    #[must_use]
    pub const fn of(seed: u64, part: u64) -> Self {
        let first = part.wrapping_mul(3);
        Self {
            part: value(seed, first.wrapping_add(1)),
            voices: value(seed, first.wrapping_add(2)),
            phases: value(seed, first.wrapping_add(3)),
        }
    }

    /// The index of the c-th sample-and-hold value of the part's LFO, from 0:
    /// c + 1.
    #[must_use]
    pub const fn part_index(draws: u64) -> u64 {
        draws.wrapping_add(1)
    }

    /// The index of the c-th sample-and-hold value of the LFO of the part's m-th
    /// note, both from 0: 2³²m + c + 1.
    #[must_use]
    pub const fn voice_index(note: u64, draws: u64) -> u64 {
        (note << 32).wrapping_add(draws).wrapping_add(1)
    }

    /// The index of the random phase of operator k of the part's m-th note,
    /// from 0: 8m + k, distinct for every note and operator while k lies from 1
    /// to 8.
    #[must_use]
    pub const fn phase_index(note: u64, operator: u64) -> u64 {
        note.wrapping_mul(8).wrapping_add(operator)
    }
}

/// An output's 53 high bits, ⌊x/2¹¹⌋, as a binary64, exactly.
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "an integer below 2⁵³ converts exactly"
)]
fn high_bits(output: u64) -> f64 {
    (output >> 11) as f64
}

/// 2⁻⁵³.
const UNIT: f64 = f64::from_bits((1023 - 53) << 52);

/// A sample-and-hold value of an output x (MATH §6.3): 2⌊x/2¹¹⌋·2⁻⁵³ − 1, in
/// [−1, 1), exactly.
#[must_use]
pub fn sample_hold(output: u64) -> f64 {
    (2.0 * high_bits(output)).mul_add(UNIT, -1.0)
}

/// A phase in turns of an output x (MATH §6.3): ⌊x/2¹¹⌋·2⁻⁵³, in [0, 1),
/// exactly.
#[must_use]
pub fn phase(output: u64) -> f64 {
    high_bits(output) * UNIT
}
