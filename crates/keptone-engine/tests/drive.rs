// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The operator envelope driven over samples by its rates' key (docs/MATH.md
//! §4.7): scenarios in which the pitch envelope moves the key, run in exact
//! arithmetic by `scripts/drive_references.py` (`tests/data/drive.txt`), their
//! levels at every sample and the instants at which segments end within the
//! bounds of their errors that the script follows; a key that holds still,
//! on which the closed form goes on; a moving key at which the speed holds
//! still, which drives the envelope as the closed form runs it; the rates'
//! key over a sample against the pitch envelope's and the portamento's own
//! laws; and samples past the budget, of the key's pieces or of the
//! progress's stretches, which take the rule on equal parts from the sample's
//! start.

use keptone_engine::eg::RateLaw;
use keptone_engine::eg::envelope::{Envelope, Parameters, Settings, Stage};
use keptone_engine::eg::progress::Rates;
use keptone_engine::peg::{self, PitchEnvelope};
use keptone_engine::pieces::{Keys, Portamento, advance};
use keptone_engine::pitch::phase::gauss_instants;
use keptone_engine::portamento::{self, Glide, Glissando, Unit};
use keptone_engine::time::Instant;

/// u = 2⁻⁵³, the unit roundoff of binary64.
const U: f64 = f64::EPSILON / 2.0;

/// The references: each line's kind and its numbers.
fn references() -> Vec<(String, Vec<f64>)> {
    let number = |word: &str| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits"));
    include_str!("data/drive.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let mut words = line.split(' ');
            let kind = words.next().expect("a kind").to_owned();
            (kind, words.map(number).collect())
        })
        .collect()
}

/// A whole count of samples below 2³² as a u64.
fn whole(value: f64) -> u64 {
    let mut count = 0_u64;
    while f64::from(u32::try_from(count).expect("a short scenario")) < value {
        count += 1;
    }
    count
}

/// The samples since sample 0 at an instant.
fn samples(at: Instant) -> f64 {
    f64::from(u32::try_from(at.sample()).expect("a short scenario")) + at.fraction()
}

/// A scenario's head: its settings, the envelope's and the pitch envelope's
/// parameters, the key, the offset, the samples and the key-off's sample.
struct Scenario {
    settings: Settings,
    envelope: Parameters,
    pitch: peg::Parameters,
    key: f64,
    offset: f64,
    samples: u64,
    release: u64,
}

impl Scenario {
    /// The head of a scenario line.
    fn of(x: &[f64]) -> Self {
        let [rate, clock_hz, law, jump, waits, key, offset, samples, release] = x[..9] else {
            panic!("a scenario's head")
        };
        let four = |from: usize| [x[from], x[from + 1], x[from + 2], x[from + 3]];
        Self {
            settings: Settings {
                sample_rate: rate,
                clock_hz,
                rate_law: if law == 0.0 { RateLaw::Dx7 } else { RateLaw::Exponential },
                attack_jump: jump != 0.0,
                waits: waits != 0.0,
            },
            envelope: Parameters {
                rates: four(9),
                levels: four(13),
                rate_scaling: x[17],
            },
            pitch: peg::Parameters {
                rates: four(18),
                levels: four(22),
            },
            key,
            offset,
            samples: whole(samples),
            release: whole(release),
        }
    }
}

#[test]
fn driven_envelopes_within_their_bound() {
    let lines = references();
    let mut index = 0;
    let (mut scenarios, mut levels, mut ends, mut worst, mut steady) = (0, 0, 0, 0.0_f64, 0);
    let mut keys = Keys::new(256).expect("memory");
    while index < lines.len() {
        let (kind, head) = &lines[index];
        assert_eq!(kind, "scenario");
        index += 1;
        let scenario = Scenario::of(head);
        let (settings, rate) = (scenario.settings, scenario.settings.sample_rate);
        let start = Instant::at(0);
        let mut pitch = PitchEnvelope::new(&scenario.pitch, start);
        pitch.key_on(start, &scenario.pitch, rate);
        let mut envelope = Envelope::new(&scenario.envelope, scenario.offset, start);
        let key_now = |pitch: &PitchEnvelope, at: Instant| 12.0_f64.mul_add(pitch.level(at), scenario.key);
        envelope.key_on(
            start,
            &scenario.envelope,
            &settings,
            key_now(&pitch, start),
            scenario.offset,
        );
        let mut still = Portamento::Still;
        for n in 1..=scenario.samples {
            let from = Instant::at(n - 1);
            keys.walk(from, scenario.key, pitch, &scenario.pitch, still, rate);
            if keys.still().is_some() {
                steady += 1;
            }
            let mut found = Vec::new();
            envelope.run(
                from,
                &keys,
                &scenario.envelope,
                &settings,
                |_| scenario.offset,
                |at| found.push(at),
            );
            for at in found {
                let (kind, x) = &lines[index];
                assert_eq!(kind, "end", "an end at sample {n}");
                index += 1;
                let error = (samples(at) - x[0]).abs();
                assert!(
                    error <= x[1],
                    "an end at {}, exactly {}, error {error:e} > {:e}",
                    samples(at),
                    x[0],
                    x[1]
                );
                worst = worst.max(error / x[1]);
                ends += 1;
            }
            let to = Instant::at(n);
            advance(&mut pitch, &scenario.pitch, &mut still, to, rate);
            let (kind, x) = &lines[index];
            assert_eq!(kind, "level", "the level at sample {n}, not another end");
            index += 1;
            let level = envelope.level(to, scenario.offset);
            if x[2] < 0.0 {
                assert_eq!(level, None, "silent at sample {n}");
            } else {
                let level = level.expect("a level");
                let error = (level - x[1]).abs();
                assert!(
                    error <= x[2],
                    "at sample {n}: {level}, exactly {}, error {error:e} > {:e}",
                    x[1],
                    x[2]
                );
                worst = worst.max(error / x[2]);
            }
            levels += 1;
            if n == scenario.release {
                pitch.key_off(to, &scenario.pitch, rate);
                envelope.key_off(to, &scenario.envelope, &settings, key_now(&pitch, to), scenario.offset);
            }
        }
        scenarios += 1;
    }
    assert!(
        scenarios >= 60 && ends > 150 && steady > 5000,
        "{scenarios} scenarios, {ends} ends, {steady} samples of a still key"
    );
    println!(
        "drive: {scenarios} scenarios, {levels} levels, {ends} ends, {steady} samples of a still key, the worst error \
         {worst:.3} of its bound"
    );
}

/// An envelope's parameters and settings for the tests below: fast rates, a
/// rate scaling, and the DX7's laws.
fn fast() -> (Parameters, Settings) {
    (
        Parameters {
            rates: [92.0, 85.0, 70.0, 88.0],
            levels: [99.0, 75.0, 60.0, 0.0],
            rate_scaling: 7.0,
        },
        Settings {
            sample_rate: 48_000.0,
            clock_hz: 49_096.0,
            rate_law: RateLaw::Dx7,
            attack_jump: true,
            waits: true,
        },
    )
}

#[test]
fn a_still_key_keeps_the_closed_form() {
    // A key that holds still at the key of the envelope's speed: the run is
    // the closed form, each segment ending at its own end, bit for bit; with
    // rates beyond the scale, segments 0, 1 and 2 end within the first
    // sample.
    let (fast, settings) = fast();
    let beyond = Parameters {
        rates: [300.0, 300.0, 300.0, 150.0],
        levels: [99.0, 98.0, 97.0, 0.0],
        rate_scaling: 7.0,
    };
    for (parameters, most) in [(fast, 1), (beyond, 3)] {
        let (key, offset) = (60.0, 4064.0);
        let start = Instant::at(0);
        let mut run = Envelope::new(&parameters, offset, start);
        run.key_on(start, &parameters, &settings, key, offset);
        let mut closed = run;
        let mut keys = Keys::new(16).expect("memory");
        keys.hold(key);
        let (mut ended, mut together) = (0, 0);
        for n in 1..=20_000_u64 {
            let from = Instant::at(n - 1);
            let to = Instant::at(n);
            let mut found = Vec::new();
            run.run(from, &keys, &parameters, &settings, |_| offset, |at| found.push(at));
            let mut expected = Vec::new();
            while let Some(end) = closed.end().filter(|end| *end <= to) {
                closed.next(&parameters, &settings, key, offset);
                expected.push(end);
            }
            assert_eq!(found, expected, "at sample {n}");
            assert_eq!(run.level(to, offset), closed.level(to, offset), "at sample {n}");
            ended += found.len();
            together = together.max(found.len());
            if n == 10_000 {
                run.key_off(to, &parameters, &settings, key, offset);
                closed.key_off(to, &parameters, &settings, key, offset);
            }
        }
        assert!(ended >= 4 && run.stage() == Stage::Resting, "{ended} ends");
        assert_eq!(together, most, "the most ends within a sample");
    }
}

#[test]
fn a_still_key_elsewhere_drives_at_its_speed() {
    // A key that holds still at another key than the one that the envelope's
    // speed was taken at: the sample drives the envelope at the speed of the
    // key there, and the closed form then goes on at it, as it does for an
    // envelope keyed on at that key, each segment after it starting at the
    // key that the sample holds. R1 50 at KRS 7 rises at q 37 from key 40 and
    // at q 54 from key 100, so that the two speeds differ.
    let (mut parameters, settings) = fast();
    parameters.rates[0] = 50.0;
    let offset = 4064.0;
    let start = Instant::at(0);
    let mut elsewhere = Envelope::new(&parameters, offset, start);
    elsewhere.key_on(start, &parameters, &settings, 40.0, offset);
    let mut there = Envelope::new(&parameters, offset, start);
    there.key_on(start, &parameters, &settings, 100.0, offset);
    let mut keys = Keys::new(16).expect("memory");
    keys.hold(100.0);
    let fastest = fastest(&parameters, &settings, 100.0);
    let (mut ended, mut drift, mut last) = (0, 0.0, 0.0);
    for n in 1..=4000_u64 {
        let from = Instant::at(n - 1);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        elsewhere.run(from, &keys, &parameters, &settings, |_| offset, |at| a.push(at));
        there.run(from, &keys, &parameters, &settings, |_| offset, |at| b.push(at));
        assert_eq!(a.len(), b.len(), "ends at sample {n}: {a:?} against {b:?}");
        for (x, y) in a.iter().zip(&b) {
            // Each closed form's end lies within u·D of its quotient and u(D +
            // 1) of the instant's sum, D its segment's length in samples, from
            // a start that the ends before it moved; the first sample's
            // progress, from which the closed form goes on, adds u·D: within
            // 5u·D + 2u for the two, below 8u(D + 1).
            drift += 8.0 * U * (samples(*y) - last + 1.0);
            last = samples(*y);
            assert!(
                (samples(*x) - samples(*y)).abs() <= drift,
                "an end at sample {n}: {a:?} against {b:?}"
            );
        }
        ended += a.len();
        // Their progresses differ by 3u of themselves, the first sample's and
        // the closed forms' products and sums, and by the drift of their
        // segments' starts.
        let to = Instant::at(n);
        let (x, y) = (elsewhere.level(to, offset), there.level(to, offset));
        let (x, y) = (x.expect("a level"), y.expect("a level"));
        assert!(
            (x - y).abs() <= agreement(y, fastest, drift),
            "at sample {n}: {x} against {y}"
        );
    }
    assert!(ended >= 2, "{ended} ends");
}

#[test]
fn a_moving_key_at_a_still_speed_drives_as_the_closed_form() {
    // Without a rate scaling the speed holds still wherever the key lies: a
    // pitch envelope that moves the key drives the envelope sample by sample,
    // and its levels and ends lie as near the closed form's at that speed as
    // their roundings allow, the two-sums keeping the progress that the samples
    // sum within 5u of itself: each sample's, the speed times its pieces'
    // lengths, which sum to 1 within u, within 2u, summed exactly beside the
    // base and read within u, against the closed form's product within 2u.
    let (mut parameters, settings) = fast();
    parameters.rate_scaling = 0.0;
    let pitch_parameters = peg::Parameters {
        rates: [60.0, 60.0, 60.0, 60.0],
        levels: [99.0, 0.0, 70.0, 50.0],
    };
    let (key, offset) = (60.0, 4064.0);
    let start = Instant::at(0);
    let mut pitch = PitchEnvelope::new(&pitch_parameters, start);
    pitch.key_on(start, &pitch_parameters, settings.sample_rate);
    let mut driven = Envelope::new(&parameters, offset, start);
    driven.key_on(start, &parameters, &settings, key, offset);
    let mut closed = driven;
    let (mut keys, mut held) = (Keys::new(64).expect("memory"), Keys::new(4).expect("memory"));
    held.hold(key);
    let mut still = Portamento::Still;
    let fastest = fastest(&parameters, &settings, key);
    let (mut moving, mut drift, mut last) = (0, 0.0, 0.0);
    for n in 1..=30_000_u64 {
        let from = Instant::at(n - 1);
        let to = Instant::at(n);
        keys.walk(from, key, pitch, &pitch_parameters, still, settings.sample_rate);
        if keys.still().is_none() {
            moving += 1;
        }
        let (mut found, mut expected) = (Vec::new(), Vec::new());
        driven.run(from, &keys, &parameters, &settings, |_| offset, |at| found.push(at));
        closed.run(from, &held, &parameters, &settings, |_| offset, |at| expected.push(at));
        advance(&mut pitch, &pitch_parameters, &mut still, to, settings.sample_rate);
        assert_eq!(found.len(), expected.len(), "ends at sample {n}");
        for (found, expected) in found.into_iter().zip(expected) {
            // The driven end, the quadratic's root on a line at a still speed
            // within 2.5u of its part of a sample, from the progress made
            // before it, within 5u of itself, over the speed, and summed into
            // an instant within 2u of a sample, lies within 5u·D + 5u of its
            // exact instant, D its segment's length in samples; the closed
            // form's within 2u·D + u: below 8u(D + 1) for the two.
            drift += 8.0 * U * (samples(expected) - last + 1.0);
            last = samples(expected);
            let error = (samples(found) - samples(expected)).abs();
            assert!(error <= drift, "an end at sample {n}: {found:?} against {expected:?}");
        }
        match (driven.level(to, offset), closed.level(to, offset)) {
            (Some(a), Some(b)) => assert!(
                (a - b).abs() <= agreement(b, fastest, drift),
                "at sample {n}: {a} against {b}"
            ),
            (a, b) => assert_eq!(a, b, "at sample {n}"),
        }
        if n == 15_000 {
            pitch.key_off(to, &pitch_parameters, settings.sample_rate);
            driven.key_off(to, &parameters, &settings, key, offset);
            closed.key_off(to, &parameters, &settings, key, offset);
        }
    }
    assert!(moving > 10_000, "{moving} samples of a moving key");
}

/// The fastest progress a sample of an envelope's rates at a key, under its
/// settings.
fn fastest(parameters: &Parameters, settings: &Settings, key: f64) -> f64 {
    parameters
        .rates
        .iter()
        .map(|&rate| rates(rate, parameters.rate_scaling, settings).step(key))
        .fold(0.0, f64::max)
}

/// A segment's rates at a rate R, with a rate scaling, under an envelope's
/// settings.
const fn rates(rate: f64, sensitivity: f64, settings: &Settings) -> Rates {
    Rates {
        rate,
        sensitivity,
        clock_hz: settings.clock_hz,
        law: settings.rate_law,
        sample_rate: settings.sample_rate,
    }
}

/// The distance within which two runs of an envelope's laws agree on its level
/// T, where their progresses differ by 8u of themselves at most and their
/// segments' starts by a drift in samples.
///
/// A segment's progress lies below 420 steps in an attack from the jump, and
/// below 4352 + |T| in a fall; the laws pass its error on by their slopes, at
/// most (4352 − 1716)/256 = 10.3 in such an attack and 1 in a fall: below
/// 8u(4352 + |T|) + 10.3·8u·420, 7·10⁴u + 8u|T|. Each evaluation of a law
/// rounds within u|T| + 4.24u|T − T₀| (`eg::attack`), below 15 600u in an
/// attack, or u|T| in a fall, and the offset's sum within u|T|: below
/// 3.2·10⁴u + 4u|T| more for the two. A drift moves a segment's progress by
/// its speed times it, at most the fastest speed ρ in steps a sample, which
/// the slopes pass on: 11ρ times the drift. In all, below 1.02·10⁵u +
/// 12u|T| + 11ρ·drift, within 64u·max(|T|, 4096) + 11ρ·drift.
fn agreement(level: f64, fastest: f64, drift: f64) -> f64 {
    64.0 * U * level.abs().max(4096.0) + 11.0 * fastest * drift
}

#[test]
fn keys_follow_their_signals() {
    // The rates' key over a sample against the pitch envelope's and the
    // portamento's own laws at interior instants of its pieces, a glide in
    // octaves or keys and a glissando among them, and the pieces tiling the
    // sample; past the budget, the key at the rule's instants.
    let rate = 48_000.0;
    let start = Instant::at(0);
    let pitch_parameters = peg::Parameters {
        rates: [99.0, 1e7, 95.0, 99.0],
        levels: [99.0, 20.0, 80.0, 50.0],
    };
    let kappa = portamento::speed(40.0).expect("a glide");
    for (case, portamento) in [
        Portamento::Still,
        Portamento::Glide(Glide::new(start, 1.5, kappa, Unit::Octaves, rate)),
        Portamento::Glide(Glide::new(start, -18.0, kappa, Unit::Keys, rate)),
        Portamento::Glissando(Glissando::new(
            start,
            30.5,
            portamento::speed(0.01).expect("fast"),
            rate,
        )),
        Portamento::Glissando(Glissando::new(
            start,
            -3000.5,
            portamento::speed(1e-5).expect("fast"),
            rate,
        )),
    ]
    .into_iter()
    .enumerate()
    {
        let mut pitch = PitchEnvelope::new(&pitch_parameters, start);
        pitch.key_on(start, &pitch_parameters, rate);
        let mut portamento = portamento;
        let mut keys = Keys::new(64).expect("memory");
        let (mut pieces, mut rules) = (0, 0);
        for n in 1..=400_u64 {
            let from = Instant::at(n - 1);
            keys.walk(from, 61.5, pitch, &pitch_parameters, portamento, rate);
            if let Some(walked) = keys.pieces() {
                let mut covered = 0.0;
                for piece in walked {
                    for share in [0.25, 0.5, 0.75] {
                        let x = share * piece.length;
                        let at = from.after(piece.start + x).expect("within the sample");
                        let expected = expected_key(pitch, portamento, &pitch_parameters, at, rate);
                        let got = piece.key.at(x);
                        // The walk takes the pitch envelope's level at the
                        // piece's start with its slope, and the glide's offset
                        // there with its law, where the key here takes each
                        // signal's law at the instant. The pitch envelope's
                        // level, within u|L| + 2u|L − L₀| + u·r/2 of its law
                        // (`peg`), r its step in octaves a sample, |L| at most
                        // 4 octaves and its moves 8 here, counts twelve times on
                        // either side; a glide's offset, within 4.35u(|d| + 3)
                        // of its law, |d| at most 18 keys, once here and twice
                        // in the walk; the slope's product and the line within
                        // u(|b| + |kᴿ|), b the key's slope in keys a sample; and
                        // each side's sums within 3u|kᴿ|: below 800u + 7u|kᴿ| +
                        // 2u|b|.
                        let bound = 256.0 * U * expected.abs().max(64.0) + 4.0 * U * piece.key.slope.abs();
                        assert!(
                            (got - expected).abs() <= bound,
                            "case {case}, sample {n}: {got} against {expected}"
                        );
                    }
                    covered += piece.length;
                }
                assert!((covered - 1.0).abs() <= 8.0 * U * f64::from(u32::try_from(walked.len()).expect("few")));
                pieces += walked.len();
            } else {
                let nodes = keys.nodes();
                assert_eq!(nodes.len(), 64, "case {case}: the rule's parts");
                for (index, nodes) in (0_u32..).zip(nodes) {
                    let boundary = |index: u32| if index >= 64 { 1.0 } else { f64::from(index) / 64.0 };
                    let part = boundary(index + 1) - boundary(index);
                    for (node, instant) in nodes.iter().zip(gauss_instants(part)) {
                        let at = from.after(boundary(index) + instant).expect("within the sample");
                        let expected = expected_key(pitch, portamento, &pitch_parameters, at, rate);
                        // A node takes the signals' laws at its instant as the
                        // key here does: within the pieces' bound.
                        assert!(
                            (node - expected).abs() <= 256.0 * U * expected.abs().max(64.0),
                            "case {case}, sample {n}: a node {node} against {expected}"
                        );
                    }
                }
                rules += 1;
            }
            advance(&mut pitch, &pitch_parameters, &mut portamento, Instant::at(n), rate);
        }
        assert!(
            pieces + rules >= 400,
            "case {case}: {pieces} pieces, {rules} samples of the rule"
        );
        assert!(case != 4 || rules >= 3, "case {case}: {rules} samples of the rule");
    }
}

/// kᴿ at an instant by the pitch envelope's and the portamento's own laws,
/// taken there as the walk takes them: 61.5 plus twelve times the pitch
/// envelope's level, and the portamento's offset in keys.
fn expected_key(
    pitch: PitchEnvelope,
    portamento: Portamento,
    parameters: &peg::Parameters,
    at: Instant,
    rate: f64,
) -> f64 {
    let (mut p, mut q) = (pitch, portamento);
    advance(&mut p, parameters, &mut q, at, rate);
    let offset = match q {
        Portamento::Still => 0.0,
        Portamento::Glide(glide) if glide.unit() == Unit::Keys => glide.offset(at),
        Portamento::Glide(glide) => 12.0 * glide.offset(at),
        Portamento::Glissando(glissando) => glissando.offset(),
    };
    12.0_f64.mul_add(p.level(at), 61.5) + offset
}

#[test]
fn a_sample_past_the_budget_takes_the_rule_on_equal_parts() {
    // A glissando over thousands of keys in a sample, its key heard stepping
    // past a budget of 8 pieces: the key at the rule's instants of 8 equal
    // parts, on which the envelope's progress is the rule's, each segment that
    // ends within a part ending where its progress, taken as even across the
    // part, reaches its end, as computed here from the rule, the targets and
    // the attack's law apart from the envelope.
    let settings = Settings {
        sample_rate: 48_000.0,
        clock_hz: 49_096.0,
        rate_law: RateLaw::Dx7,
        attack_jump: true,
        waits: true,
    };
    let parameters = Parameters {
        rates: [300.0, 300.0, 300.0, 150.0],
        levels: [99.0, 98.0, 97.0, 0.0],
        rate_scaling: 7.0,
    };
    let (rate, offset) = (settings.sample_rate, 4064.0);
    let start = Instant::at(0);
    let pitch_parameters = peg::Parameters {
        rates: [99.0; 4],
        levels: [50.0; 4],
    };
    let pitch = PitchEnvelope::new(&pitch_parameters, start);
    let speed = portamento::speed(1e-6).expect("a glide");
    let mut glissando = Portamento::Glissando(Glissando::new(start, -40_000.5, speed, rate));
    let mut keys = Keys::new(8).expect("memory");
    let target =
        |level: f64| keptone_engine::level::total(keptone_engine::level::envelope(level).expect("a level"), offset);
    let rho = |rate: f64, key: f64| rates(rate, 7.0, &settings).step(key);
    // The rule's progress over part j of a sample at a rate, from its nodes.
    let whole = |nodes: &[f64; 3], rate: f64| {
        (5.0 * (rho(rate, nodes[0]) + rho(rate, nodes[2])) + 8.0 * rho(rate, nodes[1])) / 18.0 / 8.0
    };
    let mut envelope = Envelope::new(&parameters, offset, start);
    envelope.key_on(start, &parameters, &settings, 60.0, offset);
    // Sample 1: segments 0, 1 and 2 end within the first part, the attack
    // from the jump to L1; L2 at L1's level as the DX7 takes it, a segment
    // that waits; and the fall to L3.
    keys.walk(start, 60.0, pitch, &pitch_parameters, glissando, rate);
    assert!(keys.pieces().is_none());
    let nodes = keys.nodes()[0];
    let mut found = Vec::new();
    envelope.run(start, &keys, &parameters, &settings, |_| offset, |at| found.push(at));
    let first = keptone_engine::eg::attack_progress(keptone_engine::eg::jump(16.0, target(99.0)), target(99.0));
    let fall = |from: f64, to: f64| {
        let moved = from - to;
        moved.max(keptone_engine::eg::wait(false, moved))
    };
    let (second, third) = (fall(target(99.0), target(98.0)), fall(target(98.0), target(97.0)));
    assert!(second >= 479.0 && third >= 1.0, "{second} {third}");
    let part = whole(&nodes, 300.0);
    let taken = [first / part, (first + second) / part, (first + second + third) / part];
    assert_eq!(found.len(), 3, "{found:?}");
    for (at, share) in found.iter().zip(taken) {
        // The engine sums the rule fused, within 2u of its exact sum at the
        // nodes' speeds, and this test unfused, within 3u; the engine's shares
        // take a quotient and a sum an end, within 5u of themselves after
        // three, and these one quotient after two sums: within 13u of the
        // share, an eighth of which is exact, and so is the instant's sum in
        // the sample's first part.
        let expected = share / 8.0;
        assert!(
            (samples(*at) - expected).abs() <= 16.0 * U * expected,
            "{at:?} against {expected}"
        );
    }
    // A fall stops at its target, which the engine takes as this test does.
    let held = envelope.level(Instant::at(1), offset).expect("a level");
    assert_eq!(held.to_bits(), target(97.0).to_bits(), "{held}");
    assert_eq!(envelope.stage(), Stage::Held);
    // Sample 2, after a key-off at 1: segment 3 falls from L3's level to the
    // floor within the sample, the rest after it going on by the same fall.
    let one = Instant::at(1);
    glissando.advance(one);
    envelope.key_off(one, &parameters, &settings, 60.0, offset);
    keys.walk(one, 60.0, pitch, &pitch_parameters, glissando, rate);
    assert!(keys.pieces().is_none());
    let parts: Vec<f64> = keys.nodes().iter().map(|nodes| whole(nodes, 150.0)).collect();
    let mut found = Vec::new();
    envelope.run(one, &keys, &parameters, &settings, |_| offset, |at| found.push(at));
    let fall = target(97.0) - 16.0;
    let (mut done, mut expected_end) = (0.0, None);
    for (index, part) in (0_u32..).zip(&parts) {
        if expected_end.is_none() && done + part >= fall {
            expected_end = Some((1.0 + (f64::from(index) + (fall - done) / part) / 8.0, *part));
        }
        done += part;
    }
    let (expected_end, part) = expected_end.expect("an end within the sample");
    assert_eq!(found.len(), 1, "{found:?}");
    // The progress left at the end's part, the fall less the parts before it,
    // which the engine sums exactly and this test within 6u, each part within
    // 5u in the two orders, lies within 2u of the fall and 11u of the parts
    // before, and so within 13u of the fall, which the part's progress and an
    // eighth divide; the quotients, the parts in the two orders and the
    // instants' sums add 5u of a sample: within u(13·fall/(8·part) + 5).
    assert!(
        (samples(found[0]) - expected_end).abs() <= U * (2.0 * fall / part + 8.0),
        "{:?} against {expected_end}",
        found[0]
    );
    assert_eq!(envelope.stage(), Stage::Resting);
    // The rest goes on from the floor, exactly, by the progress left in the
    // sample, the parts after the end and the rest of the end's part: within
    // 12u of the sample's progress in the two orders and u of the fall, and
    // each level within u of itself: below 16u(done + |T|).
    let level = envelope.level(Instant::at(2), offset).expect("a level");
    let expected = target(97.0) - done;
    assert!(
        (level - expected).abs() <= 16.0 * U * (done + expected.abs()),
        "{level} against {expected}"
    );
}

#[test]
fn a_sample_whose_progress_outruns_the_budget_takes_the_rule_from_its_start() {
    // A glide across some 50 keys within a sample, from key 20 up toward 100:
    // one piece of the key, within the budget of 8. Segments 0 and 1, at R 300,
    // where q is 63 and more at every key, end within the piece's first
    // stretch; segment 2, at R3 60 and KRS 7 under the dx7 law, has q from 38
    // to 53 along the piece, and its progress over the rest of the piece would
    // take more stretches than the budget leaves. The sample then takes the
    // rule on 8 equal parts from where it started: the two segments end within
    // the first part where their progress, taken as even across the part,
    // reaches their ends, and segment 2 falls by the rule's progress over the
    // rest, the key at the rule's instants by the piece's law.
    let settings = Settings {
        sample_rate: 48_000.0,
        clock_hz: 49_096.0,
        rate_law: RateLaw::Dx7,
        attack_jump: true,
        waits: true,
    };
    let parameters = Parameters {
        rates: [300.0, 300.0, 60.0, 150.0],
        levels: [99.0, 98.0, 50.0, 0.0],
        rate_scaling: 7.0,
    };
    let (rate, offset) = (settings.sample_rate, 4064.0);
    let start = Instant::at(0);
    let pitch_parameters = peg::Parameters {
        rates: [99.0; 4],
        levels: [50.0; 4],
    };
    let pitch = PitchEnvelope::new(&pitch_parameters, start);
    // 4κ/fₑ = 1 a sample: the glide arrives after ln(83/3) = 3.3 samples.
    let glide = Portamento::Glide(Glide::new(start, -80.0, 12_000.0, Unit::Keys, rate));
    let mut keys = Keys::new(8).expect("memory");
    keys.walk(start, 100.0, pitch, &pitch_parameters, glide, rate);
    let [piece] = keys.pieces().expect("the walk's pieces") else {
        panic!("one piece")
    };
    let rates = |rate: f64| rates(rate, 7.0, &settings);
    // Segment 2's speed has more corners along the piece than the budget
    // takes, and segment 0's has none.
    let mut budget = 8.0;
    assert_eq!(
        keptone_engine::eg::progress::advance(&rates(60.0), &piece.key, 1.0, None, &mut budget),
        None
    );
    let step = |rate: f64, key: f64| rates(rate).step(key);
    assert!((step(300.0, 20.0) - step(300.0, 72.0)).abs() <= 0.0);
    // The rule's progress over each part at a rate, from the key at its nodes.
    let whole = |rate: f64, index: u32| {
        let start = f64::from(index) / 8.0;
        let [first, middle, end] = gauss_instants(0.125).map(|instant| step(rate, piece.key.at(start + instant)));
        0.125 * 5.0_f64.mul_add(first + end, 8.0 * middle) / 18.0
    };
    let target =
        |level: f64| keptone_engine::level::total(keptone_engine::level::envelope(level).expect("a level"), offset);
    let first = keptone_engine::eg::attack_progress(keptone_engine::eg::jump(16.0, target(99.0)), target(99.0));
    let moved = target(99.0) - target(98.0);
    let second = moved.max(keptone_engine::eg::wait(false, moved));
    let part = whole(300.0, 0);
    let taken = [first / part, (first + second) / part];
    let rest = (1.0 - taken[1]).mul_add(whole(60.0, 0), (1..8).map(|index| whole(60.0, index)).sum::<f64>());
    let mut envelope = Envelope::new(&parameters, offset, start);
    envelope.key_on(start, &parameters, &settings, 100.0, offset);
    let mut found = Vec::new();
    envelope.run(start, &keys, &parameters, &settings, |_| offset, |at| found.push(at));
    assert_eq!(found.len(), 2, "{found:?}");
    for (at, share) in found.iter().zip(taken) {
        // The engine's and this test's parts are the same sums here; the
        // engine's shares take a quotient and a sum an end, within 3u of
        // themselves after two, and these one quotient after a sum: within 5u
        // of the share, an eighth of which is exact, and so is the instant's
        // sum in the sample's first part.
        let expected = share / 8.0;
        assert!(
            (samples(*at) - expected).abs() <= 8.0 * U * expected,
            "{at:?} against {expected}"
        );
    }
    assert_eq!(envelope.stage(), Stage::Segment(2));
    // Segment 2 falls from L2's target by the rest of the first part, the
    // share's complement within 5u of the share and 2u, and by the parts
    // after it, which the engine sums exactly and this test within 6u, part
    // 0's speeds the slowest: within 15u of that progress, and each level
    // within u of itself: below 15u·rest + 2u|T|.
    let level = envelope.level(Instant::at(1), offset).expect("a level");
    let expected = target(98.0) - rest;
    assert!(
        (level - expected).abs() <= 16.0 * U * (rest + expected.abs()),
        "{level} against {expected}"
    );
}
