// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The LFO and the random source (docs/MATH.md §6) against the exact values of
//! their laws at binary64 inputs and the scenarios that
//! `scripts/lfo_references.py` ran in exact arithmetic (`tests/data/lfo.txt`):
//! the random source's values, a part's streams, and the sample-and-hold values
//! and phases of its outputs exactly; the frequency, the waves and the delay's
//! stages within the bounds that their functions document; and at every sample
//! of a scenario, in the order of its time, the phase and the fade within the
//! bounds of their errors that the script followed, the count of the values
//! drawn and the value that the sample and hold holds exactly, and each wave
//! its shape at the phase; and the values that the LFO gives exactly.

use std::collections::HashSet;

use keptone_engine::lfo::{Lfo, frequency, shape, stages};
use keptone_engine::random::{self, Streams};
use keptone_engine::time::Instant;
use keptone_model::voice::Wave;

/// u = 2⁻⁵³, the unit roundoff of binary64.
const U: f64 = f64::EPSILON / 2.0;

/// The bounds below are first order in u; their terms of the order of u² are
/// far below this share of them.
const SECOND_ORDER: f64 = 1.0 + 1e-6;

/// The least subnormal, 2⁻¹⁰⁷⁴, beside a relative bound, for a result in the
/// subnormal range.
const SUBNORMAL: f64 = f64::from_bits(1);

/// The rate F of the firmware's timer, 9.4265 MHz/(8·3165), within u.
const TICK: f64 = 9_426_500.0 / 25_320.0;

/// The waves in the order of the references.
const WAVES: [Wave; 6] = [
    Wave::Triangle,
    Wave::SawDown,
    Wave::SawUp,
    Wave::Square,
    Wave::Sine,
    Wave::SampleAndHold,
];

/// The lines of the references: each a word and its numbers' bits, u64 values
/// or binary64 ones as the file's header says.
fn lines() -> Vec<(String, Vec<u64>)> {
    let number = |word: &str| u64::from_str_radix(word, 16).expect("hexadecimal digits");
    include_str!("data/lfo.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let mut words = line.split(' ');
            let word = words.next().expect("a word").to_owned();
            (word, words.map(number).collect())
        })
        .collect()
}

/// The binary64 of these bits.
const fn float(bits: u64) -> f64 {
    f64::from_bits(bits)
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

/// The wave of an index of the references.
fn wave(index: f64) -> Wave {
    WAVES[usize::try_from(sample(index)).expect("an index")]
}

#[test]
fn the_random_source_is_splitmix64_read_at_counters() {
    let (mut values, mut streams, mut units) = (0, 0, 0);
    for (word, x) in lines() {
        match word.as_str() {
            "value" => {
                assert_eq!(random::value(x[0], x[1]), x[2], "value({:#x}, {})", x[0], x[1]);
                values += 1;
            }
            "streams" => {
                let expected = Streams {
                    part: x[2],
                    voices: x[3],
                    phases: x[4],
                };
                assert_eq!(Streams::of(x[0], x[1]), expected, "streams({:#x}, {})", x[0], x[1]);
                streams += 1;
            }
            "unit" => {
                assert_eq!(random::sample_hold(x[0]).to_bits(), x[1], "S&H of {:#x}", x[0]);
                assert_eq!(random::phase(x[0]).to_bits(), x[2], "phase of {:#x}", x[0]);
                units += 1;
            }
            _ => {}
        }
    }
    assert!(
        values > 200 && streams > 40 && units > 100,
        "{values} {streams} {units}"
    );
    // The indices of MATH §6.3: c + 1 for the part's LFO, 2³²m + c + 1 for the
    // LFO of its m-th note, and 8m + k for the random phases, distinct for
    // every note and every operator from 1 to 8.
    assert_eq!(Streams::part_index(0), 1);
    assert_eq!(Streams::part_index(41), 42);
    assert_eq!(Streams::voice_index(0, 0), 1);
    assert_eq!(Streams::voice_index(3, 41), (3 << 32) + 42);
    assert_eq!(Streams::phase_index(0, 1), 1);
    assert_eq!(Streams::phase_index(5, 6), 46);
    let mut indices = HashSet::new();
    for note in 0..1000 {
        for operator in 1..=8 {
            assert!(
                indices.insert(Streams::phase_index(note, operator)),
                "{note} {operator}"
            );
        }
    }
    // The extremes of the outputs: −1 and 1 − 2⁻⁵², 0 and 1 − 2⁻⁵³.
    assert_eq!(random::sample_hold(0), -1.0);
    assert_eq!(random::sample_hold(u64::MAX), 1.0 - 2.0 * U);
    assert_eq!(random::phase(0), 0.0);
    assert_eq!(random::phase(u64::MAX), 1.0 - U);
    println!("{values} values, {streams} streams, {units} outputs");
}

#[test]
fn the_frequency_the_waves_and_the_stages_within_their_bounds() {
    let (mut frequencies, mut shapes, mut delays, mut worst) = (0, 0, 0, 0.0_f64);
    for (word, x) in lines() {
        let x: Vec<f64> = x.into_iter().map(float).collect();
        let checks: Vec<(f64, f64, f64, f64)> = match word.as_str() {
            "frequency" => {
                frequencies += 1;
                vec![(frequency(x[0]), x[1], x[2], x[3] * x[1] + SUBNORMAL)]
            }
            "shape" => {
                shapes += 1;
                let wave = wave(x[0]);
                // The triangle within u, the saws within u/2, the square and
                // the held value exact, and the sine within 1.46u, or 0.96
                // ulp where it is subnormal.
                let limit = match wave {
                    Wave::Triangle => U,
                    Wave::SawDown | Wave::SawUp => U / 2.0,
                    Wave::Square | Wave::SampleAndHold => 0.0,
                    Wave::Sine => 1.46 * U * x[3].abs() + SUBNORMAL,
                };
                vec![(shape(wave, x[1], x[2]), x[3], x[4], limit)]
            }
            "stages" => {
                delays += 1;
                let (wait, rise) = stages(x[0]).expect("a delay");
                // 3u at whole DELAYs and 5u between them, and half the least
                // subnormal where a stage is subnormal.
                let relative = if x[0].fract() == 0.0 { 3.0 * U } else { 5.0 * U };
                vec![
                    (wait, x[1], x[2], relative * x[1] + SUBNORMAL),
                    (rise, x[3], x[4], relative * x[3] + SUBNORMAL),
                ]
            }
            "nodelay" => {
                delays += 1;
                assert_eq!(stages(x[0]), None, "stages({:e})", x[0]);
                continue;
            }
            _ => continue,
        };
        for (y, hi, lo, limit) in checks {
            let limit = limit * SECOND_ORDER;
            let e = error(y, hi, lo);
            assert!(
                e <= limit,
                "{word}({:e}, ...): {y:e}, exactly {hi:e}, error {e:e} > {limit:e}",
                x[0]
            );
            if limit > 0.0 {
                worst = worst.max(e / limit);
            }
        }
    }
    assert!(
        frequencies > 800 && shapes > 1500 && delays > 400,
        "{frequencies} {shapes} {delays}"
    );
    println!("{frequencies} frequencies, {shapes} shapes, {delays} delays, the worst error {worst:.3} of its bound");
}

/// A scenario's state as the test drives it.
struct Run {
    lfo: Lfo,
    rate: f64,
}

#[test]
fn the_lfo_runs_its_scenarios_as_the_references_do() {
    let (mut scenarios, mut samples, mut worst_phase, mut worst_fade) = (0, 0, 0.0_f64, 0.0_f64);
    let mut run: Option<Run> = None;
    for (word, bits) in lines() {
        let x: Vec<f64> = bits.iter().copied().map(float).collect();
        match word.as_str() {
            "value" | "streams" | "unit" | "frequency" | "shape" | "stages" | "nodelay" => {}
            "scenario" => {
                run = Some(Run {
                    lfo: Lfo::new(Instant::at(0), x[1], x[0], bits[2], bits[3]),
                    rate: x[0],
                });
                scenarios += 1;
            }
            "key_on" => {
                let state = run.as_mut().expect("a scenario first");
                let at = Instant::at(sample(x[0]));
                state.lfo.key_on(at, x[1] != 0.0, x[2], state.rate);
            }
            "speed" => {
                let state = run.as_mut().expect("a scenario first");
                state.lfo.set_speed(Instant::at(sample(x[0])), x[1], state.rate);
            }
            "delay" => {
                let state = run.as_mut().expect("a scenario first");
                state.lfo.set_delay(Instant::at(sample(x[0])), x[1], state.rate);
            }
            "sample" => {
                let state = run.as_ref().expect("a scenario first");
                let n = sample(x[0]);
                let at = Instant::at(n);
                let turns = state.lfo.turns(at);
                let e = error(turns, x[1], x[2]);
                let limit = x[3] * SECOND_ORDER;
                assert!(e <= limit, "phase at {n}: {turns}, exactly {}: {e:e} > {limit:e}", x[1]);
                worst_phase = worst_phase.max(e / limit);
                assert_eq!(state.lfo.draws(at), bits[4], "values drawn by {n}");
                assert_eq!(state.lfo.wave(at, Wave::SampleAndHold), x[5], "S&H at {n}");
                for wave in &WAVES[..5] {
                    assert_eq!(state.lfo.wave(at, *wave), shape(*wave, turns, 0.0), "{wave:?} at {n}");
                }
                let fade = state.lfo.fade(at);
                let e = error(fade, x[6], x[7]);
                let limit = x[8] * SECOND_ORDER;
                assert!(e <= limit, "fade at {n}: {fade}, exactly {}: {e:e} > {limit:e}", x[6]);
                if limit > 0.0 {
                    worst_fade = worst_fade.max(e / limit);
                }
                samples += 1;
            }
            other => panic!("a line of {other}"),
        }
    }
    assert!(
        scenarios >= 100 && samples > 3000,
        "{scenarios} scenarios, {samples} samples"
    );
    println!(
        "{scenarios} scenarios, {samples} samples: the worst error of the phase {worst_phase:.3} of its bound, of the fade {worst_fade:.3}"
    );
}

#[test]
fn the_lfo_gives_these_exactly() {
    // The frequency: 11 increments a tick at speed 0, a period of 16.0 s,
    // halving at each step below 0; 8670 at speed 99, 49.25 Hz; binary64's
    // largest finite number far beyond the scale.
    let zero = TICK * 11.0 / 65536.0;
    assert!((frequency(0.0) - zero).abs() <= 3.0 * U * zero);
    assert!((frequency(-1.0) - zero / 2.0).abs() <= 6.0 * U * zero);
    assert!((frequency(99.0) - TICK * 8670.0 / 65536.0).abs() <= 3.0 * U * frequency(99.0));
    assert!((frequency(1.5) - zero * 2.0 * 2.5_f64.sqrt()).abs() <= 8.0 * U * frequency(1.5));
    assert_eq!(frequency(1e300), f64::MAX);
    assert_eq!(frequency(f64::MAX), f64::MAX);
    // The waves at the quarters of a turn.
    for (wave, values) in [
        (Wave::Triangle, [1.0, 0.0, -1.0, 0.0]),
        (Wave::SawDown, [1.0, 0.5, 0.0, -0.5]),
        (Wave::SawUp, [-1.0, -0.5, 0.0, 0.5]),
        (Wave::Square, [-1.0, -1.0, 1.0, 1.0]),
        (Wave::Sine, [0.0, -1.0, 0.0, 1.0]),
    ] {
        for (quarter, value) in values.into_iter().enumerate() {
            let turns = [0.0, 0.25, 0.5, 0.75][quarter];
            assert_eq!(shape(wave, turns, 0.0), value, "{wave:?} at {turns}");
        }
    }
    assert_eq!(shape(Wave::Square, 0.5 - U, 0.0), -1.0);
    assert_eq!(shape(Wave::SampleAndHold, 0.3, -0.25), -0.25);
    // The delay: none at or below 0; DELAY 99 2.75 s and 0.69 s, DELAY 1 38 ms
    // each; t₁ at binary64's largest finite number from DELAY 16 461 on.
    assert_eq!(stages(0.0), None);
    assert_eq!(stages(-1.0), None);
    let (wait, rise) = stages(99.0).expect("a delay");
    assert!((wait - 1024.0 / TICK).abs() <= 4.0 * U * wait);
    assert!((rise - 256.0 / TICK).abs() <= 4.0 * U * rise);
    let (wait, rise) = stages(1.0).expect("a delay");
    assert!((wait - 65536.0 / (4608.0 * TICK)).abs() <= 4.0 * U * wait);
    assert!((rise - 256.0 / (18.0 * TICK)).abs() <= 4.0 * U * rise);
    assert_eq!(stages(16_461.0).expect("a delay").0, f64::MAX);
    assert!(stages(16_460.0).expect("a delay").0 < f64::MAX);
    // An LFO starts at phase 0, its first value drawn, at the full depth.
    let rate = 48_000.0;
    let seed = 0x5eed;
    let mut lfo = Lfo::new(Instant::at(0), 99.0, rate, seed, 0);
    assert_eq!(lfo.turns(Instant::at(0)), 0.0);
    assert_eq!(lfo.draws(Instant::at(0)), 0);
    let first = random::sample_hold(random::value(seed, 1));
    assert_eq!(lfo.wave(Instant::at(0), Wave::SampleAndHold), first);
    assert_eq!(lfo.fade(Instant::at(500)), 1.0);
    // A key-on with LFO SYNC resets its phase to 0, where it draws a new value,
    // and restarts its delay, with no depth at first.
    lfo.key_on(Instant::at(100), true, 99.0, rate);
    assert_eq!(lfo.turns(Instant::at(100)), 0.0);
    assert_eq!(lfo.draws(Instant::at(100)), 1);
    assert_eq!(lfo.fade(Instant::at(100)), 0.0);
    assert_eq!(lfo.fade(Instant::at(100 + 132_000)), 0.0);
    assert!(lfo.fade(Instant::at(100 + 132_024 + 16_506)) > 0.49);
    assert_eq!(lfo.fade(Instant::at(100 + 200_000)), 1.0);
    // A change to DELAY 0 ends the delay at once; a key-on with DELAY 0 has
    // none; a key-on without LFO SYNC leaves the phase as it was.
    lfo.key_on(Instant::at(300_000), false, 99.0, rate);
    assert_eq!(lfo.fade(Instant::at(300_001)), 0.0);
    lfo.set_delay(Instant::at(300_002), 0.0, rate);
    assert_eq!(lfo.fade(Instant::at(300_002)), 1.0);
    lfo.key_on(Instant::at(300_003), false, 0.0, rate);
    assert_eq!(lfo.fade(Instant::at(300_003)), 1.0);
    let synced = Lfo::new(Instant::at(100), 99.0, rate, seed, 0);
    assert_eq!(lfo.turns(Instant::at(300_004)), synced.turns(Instant::at(300_004)));
    // A change of speed carries the phase over without a jump.
    let before = lfo.turns(Instant::at(400_000));
    lfo.set_speed(Instant::at(400_000), 0.0, rate);
    assert_eq!(lfo.turns(Instant::at(400_000)), before);
    // Where a sample's advance is whole, from 2⁵² up to 2⁵³ turns, the phase
    // stays at 0 and the count is the advance times the samples, modulo 2⁶⁴,
    // exactly; from 2⁵³ turns on the phase stands still, and so do its wraps.
    let step = frequency(1.8e11) / rate;
    assert!((4_503_599_627_370_496.0..9_007_199_254_740_992.0).contains(&step));
    let moving = Lfo::new(Instant::at(0), 1.8e11, rate, seed, 0);
    for n in [1_u64, 3, (1 << 20) + 7, 1 << 40] {
        assert_eq!(moving.turns(Instant::at(n)), 0.0);
        let count = (whole(step) * u128::from(n)) & u128::from(u64::MAX);
        assert_eq!(u128::from(moving.draws(Instant::at(n))), count, "{n}");
    }
    for speed in [2.5e11, 1e300] {
        let step = frequency(speed) / rate;
        assert!(step >= 9_007_199_254_740_992.0);
        let still = Lfo::new(Instant::at(0), speed, rate, seed, 0);
        assert_eq!(still.turns(Instant::at(12_345)), 0.0);
        assert_eq!(still.draws(Instant::at(12_345)), 0);
    }
}

/// A whole binary64 of [0, 2¹²⁸) as an integer.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole binary64 of [0, 2¹²⁸) converts exactly"
)]
fn whole(value: f64) -> u128 {
    assert!(value >= 0.0 && value.fract() == 0.0, "a whole number: {value}");
    value as u128
}
