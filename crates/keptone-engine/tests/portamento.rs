// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The portamento (docs/MATH.md §7.3) against the exact values of its laws at
//! binary64 inputs and the scenarios that `scripts/portamento_references.py`
//! ran in exact arithmetic (`tests/data/portamento.txt`): its speed within the
//! bound that its function documents; and at every event, end and sample of a
//! scenario, in the order of its time, the instants at which a glide reaches
//! its target and a glissando's key heard changes, the offsets between them
//! and a glissando's glide, within the bounds of their errors that the script
//! followed; and the values that it gives exactly.

use std::f64::consts::LN_2;

use keptone_engine::portamento::{Glide, Glissando, Unit, speed};
use keptone_engine::time::Instant;
use keptone_math::log2;

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
    include_str!("data/portamento.txt")
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

/// The speed of a scenario's TIME, which its TIMEs all have.
fn kappa(time: f64) -> f64 {
    speed(time).expect("a TIME above 0")
}

/// A voice's glide, with glissando or without.
enum Motion {
    /// Without glissando.
    Glide(Glide),
    /// With glissando.
    Glissando(Glissando),
}

impl Motion {
    /// The instant of its next change.
    fn end(&self) -> Option<Instant> {
        match self {
            Self::Glide(glide) => glide.end(),
            Self::Glissando(glissando) => glissando.end(),
        }
    }

    /// Makes its next change.
    fn next(&mut self) {
        match self {
            Self::Glide(glide) => glide.next(),
            Self::Glissando(glissando) => glissando.next(),
        }
    }

    /// The offset heard at an instant.
    fn offset(&self, at: Instant) -> f64 {
        match self {
            Self::Glide(glide) => glide.offset(at),
            Self::Glissando(glissando) => glissando.offset(),
        }
    }
}

/// A scenario's state as the test drives it.
struct Run {
    motion: Motion,
    rate: f64,
    unit: Unit,
    time: f64,
}

impl Run {
    /// Asserts that the motion has no change before a sample, or at it too,
    /// which the references would have made before the events or the reading
    /// there.
    fn nothing_ends(&self, at: u64, at_it_too: bool) {
        if let Some(end) = self.motion.end() {
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
fn the_speed_within_its_bound() {
    let (mut speeds, mut worst) = (0, 0.0_f64);
    for (word, x) in lines() {
        match word.as_str() {
            "speed" => {
                let y = speed(x[0]).expect("a speed");
                let limit = x[3] * SECOND_ORDER + SUBNORMAL;
                let e = error(y, x[1], x[2]);
                assert!(
                    e <= limit,
                    "speed({:e}): {y:e}, exactly {:e}: {e:e} > {limit:e}",
                    x[0],
                    x[1]
                );
                worst = worst.max(e / limit);
                speeds += 1;
            }
            "nospeed" => assert_eq!(speed(x[0]), None, "speed({:e})", x[0]),
            _ => {}
        }
    }
    assert!(speeds > 600, "{speeds} speeds");
    println!("{speeds} speeds, the worst error {worst:.3} of its bound");
}

#[test]
fn the_portamento_runs_its_scenarios_as_the_references_do() {
    let (mut scenarios, mut ends, mut samples, mut glides) = (0, 0, 0, 0);
    let (mut worst_end, mut worst_offset, mut worst_glide) = (0.0_f64, 0.0_f64, 0.0_f64);
    let mut run: Option<Run> = None;
    for (word, x) in lines() {
        match word.as_str() {
            "speed" | "nospeed" => {}
            "scenario" => {
                let (rate, unit, time) = (x[0], if x[1] == 0.0 { Unit::Octaves } else { Unit::Keys }, x[4]);
                let start = Instant::at(0);
                let motion = if x[2] == 0.0 {
                    Motion::Glide(Glide::new(start, x[3], kappa(time), unit, rate))
                } else {
                    Motion::Glissando(Glissando::new(start, x[3], kappa(time), rate))
                };
                run = Some(Run {
                    motion,
                    rate,
                    unit,
                    time,
                });
                scenarios += 1;
            }
            "end" => {
                let state = run.as_mut().expect("a scenario first");
                let end = state.motion.end().expect("an end that the references have");
                // n − hi is exact where the two are close (Sterbenz).
                let e = (((count(end.sample()) - x[0]) + end.fraction()) - x[1]).abs();
                let limit = x[2] * SECOND_ORDER;
                assert!(e <= limit, "end, exactly {}: {e:e} > {limit:e}", x[0]);
                if limit > 0.0 {
                    worst_end = worst_end.max(e / limit);
                }
                state.motion.next();
                ends += 1;
            }
            "time" | "target" | "switch" => {
                let state = run.as_mut().expect("a scenario first");
                let n = sample(x[0]);
                state.nothing_ends(n, false);
                let at = Instant::at(n);
                match (word.as_str(), &mut state.motion) {
                    ("time", Motion::Glide(glide)) => glide.set_speed(at, kappa(x[1]), state.rate),
                    ("time", Motion::Glissando(glissando)) => glissando.set_speed(at, kappa(x[1]), state.rate),
                    ("target", Motion::Glide(glide)) => glide.retarget(at, x[1]),
                    ("target", Motion::Glissando(glissando)) => glissando.retarget(at, x[1]),
                    (_, Motion::Glide(glide)) => {
                        assert_eq!(state.unit, Unit::Keys, "a glissando in keys");
                        let offset = glide.offset(at);
                        state.motion = Motion::Glissando(Glissando::new(at, offset, kappa(state.time), state.rate));
                    }
                    (_, Motion::Glissando(glissando)) => {
                        let offset = glissando.offset();
                        let glide = Glide::new(at, offset, kappa(state.time), Unit::Keys, state.rate);
                        state.motion = Motion::Glide(glide);
                    }
                }
                if word == "time" {
                    state.time = x[1];
                }
            }
            "sample" => {
                let state = run.as_ref().expect("a scenario first");
                let n = sample(x[0]);
                state.nothing_ends(n, true);
                let y = state.motion.offset(Instant::at(n));
                let e = error(y, x[1], x[2]);
                let limit = x[3] * SECOND_ORDER;
                assert!(e <= limit, "offset at {n}: {y}, exactly {}: {e:e} > {limit:e}", x[1]);
                if limit > 0.0 {
                    worst_offset = worst_offset.max(e / limit);
                }
                samples += 1;
            }
            "glide" => {
                let state = run.as_ref().expect("a scenario first");
                let Motion::Glissando(glissando) = &state.motion else {
                    panic!("a glissando's glide");
                };
                let n = sample(x[0]);
                let y = glissando.glide(Instant::at(n));
                let e = error(y, x[1], x[2]);
                let limit = x[3] * SECOND_ORDER;
                assert!(e <= limit, "glide at {n}: {y}, exactly {}: {e:e} > {limit:e}", x[1]);
                if limit > 0.0 {
                    worst_glide = worst_glide.max(e / limit);
                }
                glides += 1;
            }
            other => panic!("a line of {other}"),
        }
    }
    assert!(
        scenarios >= 150 && ends > 500 && samples > 3000 && glides > 1000,
        "{scenarios} scenarios, {ends} ends, {samples} samples, {glides} glides"
    );
    println!(
        "{scenarios} scenarios: {ends} ends, {samples} samples, {glides} glides; the worst errors {worst_end:.3}, {worst_offset:.3} and {worst_glide:.3} of their bounds"
    );
}

#[test]
fn the_portamento_gives_these_exactly() {
    // The speed: none at or below 0; at TIME 99 the pitch envelope's of rate 0,
    // F/8192; from TIME 1 down to 0 its time scale falling linearly to 0.
    assert_eq!(speed(0.0), None);
    assert_eq!(speed(-1.0), None);
    let tick = 9_426_500.0 / (8.0 * 3165.0 * 8192.0);
    let least = speed(99.0).expect("a speed");
    assert!((least - tick).abs() <= 4.0 * U * tick);
    let one = speed(1.0).expect("a speed");
    assert_eq!(speed(0.5), Some(2.0 * one));
    assert_eq!(speed(0.25), Some(4.0 * one));
    assert!(speed(100.0).expect("a speed") < least);
    // A glide falls from its offset to its target, slowing toward it, and
    // reaches it at ln(1 + d₀/q)/(4κ): an octave at κ = 1 in 12 000·ln 5
    // samples at 48 kHz, and 12 keys in as many.
    let rate = 48_000.0;
    let mut glide = Glide::new(Instant::at(0), 1.0, 1.0, Unit::Octaves, rate);
    assert_eq!(glide.offset(Instant::at(0)), 1.0);
    let arrival = 12_000.0 * (LN_2 * log2(5.0));
    let end = glide.end().expect("an arrival");
    assert!((count(end.sample()) + end.fraction() - arrival).abs() <= 1e-9);
    let keys = Glide::new(Instant::at(0), -12.0, 1.0, Unit::Keys, rate)
        .end()
        .expect("an arrival");
    assert!((count(keys.sample()) + keys.fraction() - arrival).abs() <= 1e-9);
    let halfway = glide.offset(Instant::at(1000));
    assert!(0.0 < halfway && halfway < 1.0);
    glide.next();
    assert_eq!(glide.offset(Instant::at(19_400)), 0.0);
    assert_eq!(glide.end(), None);
    assert_eq!(Glide::new(Instant::at(7), 0.0, 1.0, Unit::Keys, rate).end(), None);
    // A glissando from 3 keys below its target sounds them a whole key at a
    // time, 36κ keys a second, and the target when its glide reaches it.
    let step = 36.0 / rate;
    let mut glissando = Glissando::new(Instant::at(0), -3.0, 1.0, rate);
    assert_eq!(glissando.offset(), -3.0);
    for (heard, keys) in [(-2.0, 1.0), (-1.0, 2.0), (0.0, 3.0)] {
        let end = glissando.end().expect("a change");
        assert!((count(end.sample()) + end.fraction() - keys / step).abs() <= 1e-9);
        glissando.next();
        assert_eq!(glissando.offset(), heard);
    }
    assert_eq!(glissando.end(), None);
    // From 3.7 keys above, the keys heard keep its part; the target comes when
    // the glide reaches it, 3.7 keys on.
    let mut glissando = Glissando::new(Instant::at(0), 3.7, 1.0, rate);
    for heard in [2.7, 1.7, 0.7, 0.0] {
        glissando.next();
        assert!((glissando.offset() - heard).abs() <= 4.0 * U, "{}", glissando.offset());
    }
    // A change of speed keeps the glide between keys and the key heard; a
    // target moved past the glide turns the glide where it lies, and the key
    // heard, now below it, steps when the glide lies a whole key below it.
    let mut glissando = Glissando::new(Instant::at(0), -2.0, 1.0, rate);
    let at = Instant::at(1000);
    let between = glissando.glide(at);
    assert!((between - (-2.0 + 1000.0 * step)).abs() <= 4.0 * U);
    glissando.set_speed(at, 2.0, rate);
    assert_eq!(glissando.offset(), -2.0);
    assert!((glissando.glide(at) - between).abs() <= 4.0 * U);
    glissando.retarget(at, 5.0);
    assert_eq!(glissando.offset(), 3.0);
    let end = glissando.end().expect("a step");
    let expected = 1000.0 + ((between + 5.0) - 2.0) / (2.0 * step);
    assert!((count(end.sample()) + end.fraction() - expected).abs() <= 1e-9);
    glissando.next();
    assert_eq!(glissando.offset(), 2.0);
}
