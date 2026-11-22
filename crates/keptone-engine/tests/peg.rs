// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The pitch envelope (docs/MATH.md §5) against the exact values of its laws at
//! binary64 inputs and the scenarios that `scripts/peg_references.py` ran in
//! exact arithmetic (`tests/data/peg.txt`): its levels and speeds within the
//! bounds that their functions document, and at every event, end of a segment
//! and sample of a scenario, in the order of its time, its ends at their
//! instants and its levels within the bounds of their errors that the script
//! followed; and the values that it gives exactly.

use keptone_engine::eg::envelope::Stage;
use keptone_engine::peg::{Parameters, PitchEnvelope, level, speed};
use keptone_engine::time::Instant;

/// u = 2⁻⁵³, the unit roundoff of binary64.
const U: f64 = f64::EPSILON / 2.0;

/// The bounds below are first order in u; their terms of the order of u² are
/// far below this share of them.
const SECOND_ORDER: f64 = 1.0 + 1e-6;

/// Two steps of the subnormals, beside a relative bound, for a speed below the
/// normal range.
const SUBNORMAL: f64 = f64::from_bits(2);

/// The lines of the references: each a word and its numbers.
fn lines() -> Vec<(String, Vec<f64>)> {
    let number = |word: &str| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits"));
    include_str!("data/peg.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let mut words = line.split(' ');
            let word = words.next().expect("a word").to_owned();
            (word, words.map(number).collect())
        })
        .collect()
}

/// |y − (hi + lo)|: y − hi is exact where the two are close (Sterbenz), and
/// the subtraction of lo rounds at the order of u² of the result.
fn error(y: f64, hi: f64, lo: f64) -> f64 {
    ((y - hi) - lo).abs()
}

/// A whole sample of the references, which they write as a binary64, as a
/// count.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole binary64 of [0, 2⁵³) converts exactly"
)]
fn sample(value: f64) -> u64 {
    assert!(
        (0.0..9_007_199_254_740_992.0).contains(&value) && value.fract() == 0.0,
        "a whole sample: {value}"
    );
    value as u64
}

/// A count of samples below 2⁵³ as a binary64, exactly.
#[expect(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    reason = "a count below 2⁵³ converts exactly"
)]
fn count(value: u64) -> f64 {
    value as f64
}

/// A voice's pitch envelope parameters from eight numbers: R1 to R4 and L1 to
/// L4.
fn parameters(x: &[f64]) -> Parameters {
    Parameters {
        rates: [x[0], x[1], x[2], x[3]],
        levels: [x[4], x[5], x[6], x[7]],
    }
}

#[test]
fn the_levels_and_the_speeds_within_their_bounds() {
    let (mut levels, mut speeds, mut worst) = (0, 0, 0.0_f64);
    for (word, x) in lines() {
        let (y, limit) = match word.as_str() {
            "level" => {
                levels += 1;
                // u|pt|/32, and 12u|L − 99|/32 more above 99.
                let beyond = if x[0] > 99.0 {
                    12.0 * U * (x[0] - 99.0) / 32.0
                } else {
                    0.0
                };
                (level(x[0]), U * x[1].abs() + beyond)
            }
            "speed" => {
                speeds += 1;
                // 3u from 0 to 99, 3.86u below 0, and u|R − 99|/22.004 more
                // above 99.
                let relative = if x[0] < 0.0 { 3.86 * U } else { 3.0 * U };
                let beyond = if x[0] > 99.0 { U * (x[0] - 99.0) / 22.0 } else { 0.0 };
                (speed(x[0]), relative * x[1] + beyond + SUBNORMAL)
            }
            _ => continue,
        };
        let limit = limit * SECOND_ORDER;
        let e = error(y, x[1], x[2]);
        assert!(
            e <= limit,
            "{word}({:e}): {y:e}, exactly {:e}, error {e:e} > {limit:e}",
            x[0],
            x[1]
        );
        if limit > 0.0 {
            worst = worst.max(e / limit);
        }
    }
    assert!(levels > 500 && speeds > 500, "{levels} levels, {speeds} speeds");
    println!("{levels} levels, {speeds} speeds, the worst error {worst:.3} of its bound");
}

/// A scenario's state as the test drives it.
struct Run {
    envelope: PitchEnvelope,
    parameters: Parameters,
    rate: f64,
}

impl Run {
    /// Asserts that the envelope has no end before a sample, or at it too, which
    /// the references would have ended before the events or the level there.
    fn nothing_ends(&self, at: u64, at_it_too: bool) {
        if let Some(end) = self.envelope.end() {
            let pending = if at_it_too {
                end <= Instant::at(at)
            } else {
                end < Instant::at(at)
            };
            assert!(!pending, "an end at {}+{} by {at}", end.sample(), end.fraction());
        }
    }
}

#[test]
fn the_pitch_envelope_runs_its_scenarios_as_the_references_do() {
    let (mut scenarios, mut ends, mut samples, mut worst) = (0, 0, 0, 0.0_f64);
    let mut run: Option<Run> = None;
    for (word, x) in lines() {
        match word.as_str() {
            "level" | "speed" => {}
            "scenario" => {
                let parameters = parameters(&x[1..9]);
                run = Some(Run {
                    envelope: PitchEnvelope::new(&parameters, Instant::at(0)),
                    parameters,
                    rate: x[0],
                });
                scenarios += 1;
            }
            "end" => {
                let state = run.as_mut().expect("a scenario first");
                let end = state.envelope.end().expect("an end that the references have");
                let segment = u8::try_from(sample(x[0])).expect("a segment");
                assert_eq!(state.envelope.stage(), Stage::Segment(segment));
                // n − hi is exact where the two are close (Sterbenz).
                let e = (((count(end.sample()) - x[1]) + end.fraction()) - x[2]).abs();
                let limit = x[3] * SECOND_ORDER;
                assert!(
                    e <= limit,
                    "end of segment {segment}, exactly {}: {e:e} > {limit:e}",
                    x[1]
                );
                if limit > 0.0 {
                    worst = worst.max(e / limit);
                }
                state.envelope.next(&state.parameters, state.rate);
                ends += 1;
            }
            "key_on" | "key_off" | "change" => {
                let state = run.as_mut().expect("a scenario first");
                let at = sample(x[0]);
                state.nothing_ends(at, false);
                let instant = Instant::at(at);
                match word.as_str() {
                    "key_on" => state.envelope.key_on(instant, &state.parameters, state.rate),
                    "key_off" => state.envelope.key_off(instant, &state.parameters, state.rate),
                    _ => {
                        state.parameters = parameters(&x[1..9]);
                        state.envelope.change(instant, &state.parameters, state.rate);
                    }
                }
            }
            "sample" => {
                let state = run.as_mut().expect("a scenario first");
                let at = sample(x[0]);
                state.nothing_ends(at, true);
                let y = state.envelope.level(Instant::at(at));
                let e = error(y, x[1], x[2]);
                let limit = x[3] * SECOND_ORDER;
                assert!(e <= limit, "level at {at}: {y}, exactly {}: {e:e} > {limit:e}", x[1]);
                if limit > 0.0 {
                    worst = worst.max(e / limit);
                }
                samples += 1;
            }
            other => panic!("a line of {other}"),
        }
    }
    assert!(
        scenarios >= 100 && ends > 200 && samples > 1000,
        "{scenarios} scenarios, {ends} ends, {samples} samples"
    );
    println!("{scenarios} scenarios: {ends} ends, {samples} samples, the worst error {worst:.3} of its bound");
}

#[test]
fn the_pitch_envelope_gives_these_exactly() {
    // The levels: 0 at L = 50, the table's ends ±4 octaves within a step, and
    // its end steps beyond the scale; the speed of rate 99, 255/22.004.
    assert_eq!(level(50.0), 0.0);
    assert_eq!(level(0.0), -4.0);
    assert_eq!(level(99.0), 127.0 / 32.0);
    assert_eq!(level(-1.0), -140.0 / 32.0);
    assert_eq!(level(100.0), 139.0 / 32.0);
    assert_eq!(level(49.5), -0.5 / 32.0);
    let tick = 9_426_500.0 / (8.0 * 3165.0 * 8192.0);
    assert!((speed(99.0) - 255.0 * tick).abs() <= 4.0 * U * speed(99.0));
    assert!((speed(0.0) - tick).abs() <= 4.0 * U * tick);
    assert!((speed(-1.0) - tick / 2.0).abs() <= 5.0 * U * tick);
    // A key-on starts at L4 and moves to L1; a segment whose target is its
    // start ends at once; past segment 2 it holds at L3, and a key-off takes
    // it to L4, a second one changing nothing.
    let parameters = Parameters {
        rates: [99.0, 99.0, 99.0, 99.0],
        levels: [99.0, 99.0, 75.0, 50.0],
    };
    let mut envelope = PitchEnvelope::new(&parameters, Instant::at(0));
    assert_eq!(envelope.stage(), Stage::Resting);
    assert_eq!(envelope.level(Instant::at(0)), 0.0);
    envelope.key_on(Instant::at(10), &parameters, 48_000.0);
    assert_eq!(envelope.level(Instant::at(10)), 0.0);
    let end = envelope.end().expect("segment 0 ends");
    envelope.next(&parameters, 48_000.0);
    assert_eq!(envelope.level(end), 127.0 / 32.0);
    assert_eq!(envelope.end(), Some(end), "segment 1 starts at its target");
    envelope.next(&parameters, 48_000.0);
    let end = envelope.end().expect("segment 2 ends");
    envelope.next(&parameters, 48_000.0);
    assert_eq!(envelope.stage(), Stage::Held);
    assert_eq!(envelope.end(), None);
    assert_eq!(envelope.level(Instant::at(100_000)), level(75.0));
    assert!(end < Instant::at(100_000));
    envelope.key_off(Instant::at(100_000), &parameters, 48_000.0);
    let release = envelope;
    envelope.key_off(Instant::at(100_001), &parameters, 48_000.0);
    assert_eq!(envelope, release);
    let end = envelope.end().expect("segment 3 ends");
    envelope.next(&parameters, 48_000.0);
    assert_eq!(envelope.stage(), Stage::Resting);
    assert_eq!(envelope.level(end), 0.0);
}
