// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The operator envelope (docs/MATH.md §4) against the scenarios that
//! `scripts/envelope_references.py` ran in exact arithmetic
//! (`tests/data/envelope.txt`): at every event, end of a segment and level of
//! a scenario, in the order of its time, the envelope's ends at their instants
//! and its levels within the bounds of their errors that the script followed
//! from those that the engine's functions document; and the values that the
//! envelope gives exactly.

use keptone_engine::eg::RateLaw;
use keptone_engine::eg::envelope::{Envelope, Parameters, Settings, Stage};
use keptone_engine::time::Instant;

/// The bounds below are first order in u; their terms of the order of u² are
/// far below this share of them.
const SECOND_ORDER: f64 = 1.0 + 1e-6;

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

/// The time of an instant in samples, n + η.
fn time(instant: Instant) -> f64 {
    count(instant.sample()) + instant.fraction()
}

/// A scenario's lines: each a word and its numbers.
fn lines() -> Vec<(String, Vec<f64>)> {
    let number = |word: &str| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits"));
    include_str!("data/envelope.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let mut words = line.split(' ');
            let word = words.next().expect("a word").to_owned();
            (word, words.map(number).collect())
        })
        .collect()
}

/// An envelope's parameters from nine numbers: R1 to R4, L1 to L4 and KRS.
fn parameters(x: &[f64]) -> Parameters {
    Parameters {
        rates: [x[0], x[1], x[2], x[3]],
        levels: [x[4], x[5], x[6], x[7]],
        rate_scaling: x[8],
    }
}

/// A scenario's state as the test drives it.
struct Run {
    envelope: Envelope,
    settings: Settings,
    parameters: Parameters,
    key: f64,
    offset: f64,
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
            assert!(!pending, "an end at {} by {at}", time(end));
        }
    }
}

#[test]
fn the_envelope_runs_its_scenarios_as_the_references_do() {
    let (mut scenarios, mut ends, mut levels, mut worst) = (0, 0, 0, 0.0_f64);
    let mut run: Option<Run> = None;
    for (word, x) in lines() {
        if word == "scenario" {
            let settings = Settings {
                sample_rate: x[0],
                clock_hz: x[1],
                rate_law: if x[2] > 0.0 { RateLaw::Exponential } else { RateLaw::Dx7 },
                attack_jump: x[3] > 0.0,
                waits: x[4] > 0.0,
            };
            let parameters = parameters(&x[5..14]);
            let (key, offset) = (x[14], x[15]);
            let envelope = Envelope::new(&parameters, offset, Instant::at(0));
            run = Some(Run {
                envelope,
                settings,
                parameters,
                key,
                offset,
            });
            scenarios += 1;
            continue;
        }
        let state = run.as_mut().expect("a scenario first");
        match word.as_str() {
            "end" => {
                let end = state.envelope.end().expect("an end that the references have");
                let segment = u8::try_from(sample(x[0])).expect("a segment");
                assert_eq!(state.envelope.stage(), Stage::Segment(segment));
                // n − hi is exact where the two are close (Sterbenz), so that the
                // fraction meets the difference unrounded by n's magnitude.
                let error = (((count(end.sample()) - x[1]) + end.fraction()) - x[2]).abs();
                let limit = x[3] * SECOND_ORDER;
                assert!(
                    error <= limit,
                    "end of segment {segment} at {}, exactly {}: {error:e} > {limit:e}",
                    time(end),
                    x[1]
                );
                if limit > 0.0 {
                    worst = worst.max(error / limit);
                }
                state
                    .envelope
                    .next(&state.parameters, &state.settings, state.key, state.offset);
                ends += 1;
            }
            "key_on" | "key_off" | "offset" | "change" => {
                let at = sample(x[0]);
                state.nothing_ends(at, false);
                let instant = Instant::at(at);
                match word.as_str() {
                    "key_on" => {
                        state
                            .envelope
                            .key_on(instant, &state.parameters, &state.settings, state.key, state.offset);
                    }
                    "key_off" => {
                        state
                            .envelope
                            .key_off(instant, &state.parameters, &state.settings, state.key, state.offset);
                    }
                    "offset" => state.offset = x[1],
                    _ => {
                        state.parameters = parameters(&x[1..10]);
                        state.key = x[10];
                        state
                            .envelope
                            .change(instant, &state.parameters, &state.settings, state.key);
                    }
                }
            }
            "level" | "silent" => {
                let at = sample(x[0]);
                state.nothing_ends(at, true);
                let level = state.envelope.level(Instant::at(at), state.offset);
                if word == "silent" {
                    assert_eq!(level, None, "silent at {at}");
                } else {
                    let level = level.expect("a level that the references have");
                    let error = ((level - x[1]) - x[2]).abs();
                    let limit = x[3] * SECOND_ORDER;
                    assert!(
                        error <= limit,
                        "level at {at}: {level}, exactly {}: {error:e} > {limit:e}",
                        x[1]
                    );
                    if limit > 0.0 {
                        worst = worst.max(error / limit);
                    }
                }
                levels += 1;
            }
            other => panic!("a line of {other}"),
        }
    }
    assert!(
        scenarios >= 100 && ends > 200 && levels > 1000,
        "{scenarios} scenarios, {ends} ends, {levels} levels"
    );
    println!("{scenarios} scenarios: {ends} ends, {levels} levels, the worst error {worst:.3} of its bound");
}

#[test]
fn instants_count_their_samples() {
    let start = Instant::new(10, 0.25).expect("a fraction below 1");
    assert_eq!(Instant::new(10, 1.0), None);
    assert_eq!(start.after(2.5), Instant::new(12, 0.75));
    assert_eq!(start.after(0.75), Some(Instant::at(11)));
    assert_eq!(start.after(-3.0), Some(start));
    assert_eq!(Instant::at(12).samples_since(start), 1.75);
    assert_eq!(start.samples_since(Instant::at(12)), -1.75);
    assert_eq!(start.after(f64::INFINITY), None);
    assert_eq!(Instant::at(u64::MAX).after(1.0), None);
    assert!(Instant::at(3) < Instant::new(3, 0.5).expect("a fraction below 1"));
    assert!(Instant::new(3, 0.5).expect("a fraction below 1") < Instant::at(4));
}

#[test]
fn the_envelope_gives_these_exactly() {
    use keptone_engine::eg::{FLOOR, JUMP, WAIT, end_level, rate, speed};
    assert_eq!(FLOOR, 16.0);
    assert!((end_level(-200.0) + 4_920.135_922_911_648).abs() <= 1e-11);
    let settings = Settings {
        sample_rate: 48_000.0,
        clock_hz: 49_096.0,
        rate_law: RateLaw::Dx7,
        attack_jump: true,
        waits: true,
    };
    let parameters = Parameters {
        rates: [99.0, 40.0, 40.0, 60.0],
        levels: [99.0, 99.0, 99.0, 0.0],
        rate_scaling: 0.0,
    };
    // A voice that has had no note rests at L4's level, −∞ for L4 = 0.
    let mut envelope = Envelope::new(&parameters, 4064.0, Instant::at(0));
    assert_eq!(envelope.stage(), Stage::Resting);
    assert_eq!(envelope.end(), None);
    assert_eq!(envelope.level(Instant::at(0), 4064.0), None);
    // A key-on rises from the floor, jumping to 1716; a change of the offset
    // moves the level by its difference.
    envelope.key_on(Instant::at(0), &parameters, &settings, 60.0, 4064.0);
    assert_eq!(envelope.stage(), Stage::Segment(0));
    assert_eq!(envelope.level(Instant::at(0), 4064.0), Some(JUMP));
    assert_eq!(envelope.level(Instant::at(0), 4164.0), Some(JUMP + 100.0));
    // Segment 0 reaches full scale, 3840, and segment 1, which does not move,
    // waits 480 steps of its progress at its speed.
    let end = envelope.end().expect("segment 0 ends");
    envelope.next(&parameters, &settings, 60.0, 4064.0);
    assert_eq!(envelope.stage(), Stage::Segment(1));
    assert_eq!(envelope.level(end, 4064.0), Some(3840.0));
    let step = speed(rate(40.0, 0.0, 60.0), 49_096.0, RateLaw::Dx7) / 48_000.0;
    let wait = envelope.end().expect("segment 1 ends").samples_since(end);
    assert!((wait - WAIT / step).abs() <= 1e-9 * wait, "a wait of {wait} samples");
    // A key-off ends the wait: segment 3 falls from 3840 at its own speed, and
    // a second key-off changes nothing.
    envelope.key_off(Instant::at(7000), &parameters, &settings, 60.0, 4064.0);
    assert_eq!(envelope.stage(), Stage::Segment(3));
    let release = envelope;
    envelope.key_off(Instant::at(8000), &parameters, &settings, 60.0, 4064.0);
    assert_eq!(envelope, release);
    assert_eq!(envelope.level(Instant::at(7000), 4064.0), Some(3840.0));
    // It ends where the DX7's level reaches its floor, 3824 steps below, and
    // rests there, its own level falling on toward −∞.
    let end = envelope.end().expect("segment 3 ends");
    envelope.next(&parameters, &settings, 60.0, 4064.0);
    assert_eq!(envelope.stage(), Stage::Resting);
    let level = envelope.level(end, 4064.0).expect("a level");
    assert!((level - FLOOR).abs() <= 1e-9, "{level}");
    let later = envelope.level(Instant::at(1_000_000), 4064.0).expect("a level");
    assert!(later < -100_000.0, "{later}");
}
