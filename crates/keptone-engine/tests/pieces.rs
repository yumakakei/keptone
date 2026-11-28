// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The pieces of a sample (docs/MATH.md §2.5): each kind of piece's increment,
//! in closed form or by the rule on the parts that its truncation needs,
//! against its exact value at binary64 inputs, which
//! `scripts/pieces_references.py` computed with mpmath's quadrature
//! (`tests/data/pieces.txt`), within the bound that the parts document; and
//! the walk over samples of voices whose pitch envelopes, glides,
//! glissandos and LFOs move within them, its parts against those signals' own
//! laws at the parts' instants, the parts tiling each sample, and a sample past
//! `osc.max_pieces` on the rule's equal parts; and the level's key under
//! `scaling.key = pitch` over those samples, which the rates' key then takes,
//! against the same laws.

use keptone_engine::control::{Limited, Source, controllers, modulation, pitch_sensitivity};
use keptone_engine::lfo::Lfo;
use keptone_engine::peg::{self, PitchEnvelope};
use keptone_engine::pieces::{Keys, Part, Parts, Piece, Portamento, Shape, Signals, advance, sample, span};
use keptone_engine::pitch::phase::gauss_instants;
use keptone_engine::portamento::{self, Glide, Glissando, Unit};
use keptone_engine::time::Instant;
use keptone_math::eft::two_sum;
use keptone_model::voice::Wave;

/// u = 2⁻⁵³, the unit roundoff of binary64.
const U: f64 = f64::EPSILON / 2.0;

/// The references: each line's kind and its numbers.
fn references() -> Vec<(String, Vec<f64>)> {
    let number = |word: &str| f64::from_bits(u64::from_str_radix(word, 16).expect("hexadecimal bits"));
    include_str!("data/pieces.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let mut words = line.split(' ');
            let kind = words.next().expect("a kind").to_owned();
            (kind, words.map(number).collect())
        })
        .collect()
}

/// A count of parts as a binary64.
fn real(count: usize) -> f64 {
    f64::from(u32::try_from(count).expect("a count of parts"))
}

/// A count of parts as a u32.
fn whole(count: f64) -> u32 {
    let mut whole = 0_u32;
    while f64::from(whole) < count {
        whole += 1;
    }
    whole
}

/// The increment of an operator over a piece, its parts' increments summed
/// by two-sums, so that the sum adds no rounding of the first order.
fn increment(piece: &Piece, ratio: f64, constant: f64, fixed: bool) -> (f64, f64) {
    let count = whole(piece.count());
    let (mut high, mut low) = (0.0, 0.0);
    for index in 0..count {
        let (sum, error) = two_sum(high, piece.part(count, index).increment(ratio, constant, fixed));
        high = sum;
        low += error;
    }
    two_sum(high, low)
}

#[test]
fn pieces_within_their_bound() {
    let (mut checked, mut worst, mut ruled) = (0, 0.0_f64, 0);
    for (kind, x) in references() {
        let [
            length,
            pitch,
            slope,
            glide,
            rate,
            depth,
            depth_slope,
            first,
            second,
            ratio,
            constant,
            fixed,
            hi,
            lo,
            bound,
            fewest,
        ] = x[..]
        else {
            panic!("sixteen numbers")
        };
        let shape = match kind.as_str() {
            "constant" => Shape::Constant(first),
            "linear" => Shape::Linear {
                value: first,
                slope: second,
            },
            "sine" => Shape::Sine {
                turns: first,
                step: second,
            },
            _ => panic!("a kind of wave"),
        };
        let piece = Piece {
            length,
            pitch,
            slope,
            glide,
            rate,
            depth,
            depth_slope,
            shape,
        };
        // The parts: the fewest on which MATH's condition holds, or a few
        // more, 0.3 % at most: the share 0.999 of the part's length that the
        // sizing takes, and its step e^(−a₁h₀/6) for e^(−a₁h/6), which costs
        // at most 0.2 %, B₆ being at least a₁⁶, so that a₁h₀ ≤ 2016000^(1/6)·2⁻¹⁰.
        let count = piece.count();
        assert!(
            fewest <= count && count <= (1.003 * fewest).ceil(),
            "{kind} {x:?}: {count} parts, the fewest {fewest}"
        );
        let (high, low) = increment(&piece, ratio, constant, fixed != 0.0);
        let error = ((high - hi) + (low - lo)).abs();
        assert!(
            error <= bound,
            "{kind} {x:?}: {high:e}, exactly {hi:e}, error {error:e} > {bound:e}"
        );
        worst = worst.max(error / bound);
        checked += 1;
        if piece.count() > 1.0 {
            ruled += 1;
        }
    }
    assert!(
        checked > 2000 && ruled > 500,
        "{checked} pieces, {ruled} on several parts"
    );
    println!("pieces: {checked}, {ruled} on several parts, the worst error {worst:.3} of its bound");
}

#[test]
fn a_sine_takes_the_parts_that_math_gives() {
    // MATH §2.5: at the LFO's top speed on its scale and a depth of an
    // octave, two parts at 44.1 kHz, three at 22.05 kHz and eight at 8 kHz.
    for (rate, parts) in [(44_100.0, 2.0), (22_050.0, 3.0), (8_000.0, 8.0), (96_000.0, 1.0)] {
        let piece = Piece {
            length: 1.0,
            pitch: 0.0,
            slope: 0.0,
            glide: 0.0,
            rate: 0.0,
            depth: 1.0,
            depth_slope: 0.0,
            shape: Shape::Sine {
                turns: 0.0,
                step: 49.25 / rate,
            },
        };
        assert_eq!(piece.count(), parts, "{rate} Hz");
    }
    // A line takes one part, a glide that does not move adds its offset.
    let line = Piece {
        length: 1.0,
        pitch: 1.0,
        slope: 1e-3,
        glide: 2.0,
        rate: 0.0,
        depth: 0.5,
        depth_slope: 1e-3,
        shape: Shape::Constant(-1.0),
    };
    assert_eq!(line.count(), 1.0);
    let Part::Linear {
        pitch,
        slope,
        fixed_pitch,
        fixed_slope,
        ..
    } = line.part(1, 0)
    else {
        panic!("a line in closed form")
    };
    assert_eq!((pitch, slope, fixed_pitch, fixed_slope), (2.5, 0.0, -0.5, -1e-3));
}

/// `SplitMix64`, as the references' scripts have it.
struct Random(u64);

impl Random {
    /// The next output.
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let z = (self.0 ^ (self.0 >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        let z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A real of [low, high).
    fn uniform(&mut self, low: f64, high: f64) -> f64 {
        let unit = f64::from_bits(0x3ff0_0000_0000_0000 | (self.next() >> 12)) - 1.0;
        (high - low).mul_add(unit, low)
    }

    /// One of a few.
    fn pick<T: Copy>(&mut self, choices: &[T]) -> T {
        let index = usize::try_from(self.next() % u64::try_from(choices.len()).expect("a few")).expect("a few");
        choices[index]
    }
}

/// What moves a voice's pitch in a scenario: its states, as a voice keeps
/// them between samples.
#[derive(Clone)]
struct Voice {
    /// Its pitch envelope.
    envelope: PitchEnvelope,
    /// Its parameters.
    parameters: peg::Parameters,
    /// Its portamento.
    portamento: Portamento,
    /// Its LFO.
    lfo: Lfo,
    /// The LFO's wave.
    wave: Wave,
    /// σ(PMS).
    sensitivity: f64,
    /// PMD.
    depth: f64,
    /// The controllers' depth of the pitch.
    controllers: Limited,
    /// The engine's rate.
    sample_rate: f64,
}

impl Voice {
    /// A copy of it whose states are taken to an instant, as the walk takes
    /// them within a sample.
    fn clone_at(&self, at: Instant) -> Self {
        let mut copy = self.clone();
        advance(
            &mut copy.envelope,
            &copy.parameters,
            &mut copy.portamento,
            at,
            copy.sample_rate,
        );
        copy
    }

    /// The signals of its states.
    fn signals(&self) -> Signals<'_> {
        Signals {
            pitch_envelope: self.envelope,
            pitch_parameters: &self.parameters,
            portamento: self.portamento,
            lfo: &self.lfo,
            wave: self.wave,
            sensitivity: self.sensitivity,
            depth: self.depth,
            controllers: self.controllers,
            sample_rate: self.sample_rate,
        }
    }

    /// λ at an instant by the signals' own laws, the pitch envelope taken
    /// through its ends to it as the walk takes it, in the ratio mode and in
    /// the fixed mode, with the bound of the difference between it and a
    /// part's: each part is the same signals' laws evaluated from the
    /// piece's start, and the two evaluations' errors, those that the pitch
    /// envelope's, the glide's, the LFO's and the depth's laws document and
    /// those of the parts' forms (`keptone_engine::pieces`), each a few u of
    /// a magnitude, sum to less than 64u times the magnitudes below; the LFO's
    /// phase, within 3u turns in each, moves its wave by its slope of 4 a turn
    /// at most, or 2π. Besides, a law evaluated at an instant between samples
    /// counts the samples since its anchor within u(Δ + 1/2) of themselves,
    /// where the walk counts them from the sample's start, exactly: that moves
    /// the pitch envelope by its speed, the glide by its rate times its
    /// distance and the LFO's phase by its step, each a sample, times that
    /// count's error, Δ being at most the instant's samples since the
    /// scenario's start.
    fn at(&self, at: Instant) -> (f64, f64, f64) {
        let mut envelope = self.envelope;
        let mut portamento = self.portamento;
        advance(&mut envelope, &self.parameters, &mut portamento, at, self.sample_rate);
        let level = envelope.level(at);
        let (porta, glide, rate) = match portamento {
            Portamento::Still => (0.0, 0.0, 0.0),
            Portamento::Glide(glide) => {
                let offset = glide.offset(at);
                let octaves = if glide.unit() == Unit::Keys {
                    offset / 12.0
                } else {
                    offset
                };
                (octaves, octaves.abs(), glide.rate())
            }
            Portamento::Glissando(glissando) => (glissando.offset_at(at) / 12.0, 0.0, 0.0),
        };
        let depth = self.sensitivity * modulation(self.depth, self.lfo.fade(at), self.controllers).value;
        let lfo = depth * self.lfo.wave(at, self.wave);
        let magnitude = level.abs() + porta.abs() + glide + 0.25 + depth.abs() * 8.0;
        let speeds =
            envelope.slope().abs() + rate * (glide + 0.25) + core::f64::consts::TAU * depth.abs() * self.lfo.step();
        let tolerance = 64.0 * U * magnitude + 8.0 * U * (elapsed(at) + 1.0) * speeds;
        (level + porta + lfo, lfo, tolerance)
    }
}

/// The samples since the scenario's start at an instant, within u of them.
fn elapsed(at: Instant) -> f64 {
    f64::from(u32::try_from(at.sample()).expect("a short scenario")) + at.fraction()
}

/// A voice drawn at random, at a rate: its pitch envelope keyed on at sample
/// 0, a glide in octaves or keys, a glissando or none, and its LFO's speed,
/// delay and wave, on the scales and beyond them.
fn voice(random: &mut Random, sample_rate: f64) -> Voice {
    let level = |random: &mut Random| random.pick(&[0.0, 50.0, 99.0, 25.5, 75.25, 120.0, -10.0]);
    let speed = |random: &mut Random| random.pick(&[99.0, 99.0, 99.0, 60.0, 80.5, 150.0, 400.0, 1e7, 1e7]);
    let parameters = peg::Parameters {
        rates: [speed(random), speed(random), speed(random), speed(random)],
        levels: [level(random), level(random), level(random), level(random)],
    };
    let start = Instant::at(0);
    let mut envelope = PitchEnvelope::new(&parameters, start);
    envelope.key_on(start, &parameters, sample_rate);
    let time = random.pick(&[30.0, 70.0, 90.0, 99.0, 0.5, 0.01]);
    let kappa = portamento::speed(time).expect("a glide");
    let offset = random.uniform(-3.0, 3.0);
    let portamento = match random.next() % 4 {
        0 => Portamento::Still,
        1 => Portamento::Glide(Glide::new(start, offset, kappa, Unit::Octaves, sample_rate)),
        2 => Portamento::Glide(Glide::new(start, 12.0 * offset, kappa, Unit::Keys, sample_rate)),
        _ => Portamento::Glissando(Glissando::new(start, 12.0 * offset, kappa, sample_rate)),
    };
    let wave = random.pick(&[
        Wave::Triangle,
        Wave::SawDown,
        Wave::SawUp,
        Wave::Square,
        Wave::Sine,
        Wave::SampleAndHold,
    ]);
    let lfo_speed = random.pick(&[35.0, 99.0, 99.0, 70.5, 300.0, 700.0, 1500.0, 2e6, 1e18]);
    let mut lfo = Lfo::new(start, lfo_speed, sample_rate, random.next(), 0);
    let delay = random.pick(&[0.0, 0.0, 0.01, 0.05, 0.3, 1.0, 5.0]);
    lfo.key_on(start, random.next().is_multiple_of(2), delay, sample_rate);
    let wheel = Source {
        position: random.uniform(0.0, 1.0),
        range: random.pick(&[0.0, 50.0, 99.0]),
    };
    Voice {
        envelope,
        parameters,
        portamento,
        lfo,
        wave,
        sensitivity: pitch_sensitivity(random.pick(&[3.0, 7.0, 7.0, 12.0])),
        depth: random.pick(&[0.0, 99.0, 99.0, 60.0, 200.0]),
        controllers: controllers(&[wheel]),
        sample_rate,
    }
}

/// Checks a sample's parts against the voice's signals (see [`checked_over`]).
fn checked(parts: &Parts, from: Instant, voice: &Voice) -> usize {
    checked_over(parts, from, 1.0, voice)
}

/// Checks the parts of a span of so many samples against the voice's signals:
/// their lengths tile the span, within the rounding of the pieces' lengths, u
/// a piece; and at interior instants of each part, the rule's or a linear
/// part's quarters, λ agrees with the signals' laws within their bound, but
/// within the uncertainty of a jump of the LFO's wave, its phase's 6u turns
/// over its step, and that of the instant, which the sum of the lengths before
/// it carries; returns the parts' count.
fn checked_over(parts: &Parts, from: Instant, span: f64, voice: &Voice) -> usize {
    let (mut start, mut sum_low) = (0.0, 0.0);
    let jumps = matches!(
        voice.wave,
        Wave::SawDown | Wave::SawUp | Wave::Square | Wave::SampleAndHold
    );
    let step = voice.lfo.step();
    let count = parts.as_slice().len();
    for part in parts.as_slice() {
        let length = part.length();
        assert!(length > 0.0, "a part of no length");
        let instants: Vec<(f64, f64, f64)> = match *part {
            Part::Linear {
                pitch,
                slope,
                fixed_pitch,
                fixed_slope,
                ..
            } => [0.25, 0.5, 0.75]
                .into_iter()
                .map(|share| {
                    let x = share * length;
                    (x, slope.mul_add(x, pitch), fixed_slope.mul_add(x, fixed_pitch))
                })
                .collect(),
            Part::Rule { pitches, fixed, .. } => gauss_instants(length)
                .into_iter()
                .zip(pitches.into_iter().zip(fixed))
                .map(|(x, (pitch, fixed))| (x, pitch, fixed))
                .collect(),
        };
        for (x, pitch, fixed) in instants {
            let position = start + x;
            let at = from.after(position).expect("an instant within the sample");
            let (expected, expected_fixed, tolerance) = voice.at(at);
            // A jump of the wave lies within the uncertainty of the LFO's
            // phase, 6u turns over its step and the count of samples' u(Δ + 1)
            // in each evaluation, of where the walk puts it.
            let edge = if step > 0.0 { 8.0 * U / step } else { 0.0 } + 8.0 * U * (elapsed(at) + 1.0);
            let near = jumps && x.min(length - x) <= 32.0 * U * real(count) + edge;
            let error = (pitch - expected).abs().max((fixed - expected_fixed).abs());
            assert!(
                error <= tolerance || near,
                "at {position} of the sample from {from:?}: {pitch} and {fixed}, the signals {expected} and \
                 {expected_fixed}, error {error:e} > {tolerance:e}"
            );
        }
        let (sum, error) = two_sum(start, length);
        start = sum;
        sum_low += error;
    }
    let total = start + sum_low;
    assert!(
        (total - span).abs() <= 2.0 * U * real(count),
        "the parts cover {total} of the span of {span}"
    );
    count
}

#[test]
fn walks_follow_their_signals() {
    let mut random = Random(0x7761_6c6b);
    let (mut samples, mut most, mut fallbacks, mut held) = (0, 0, 0, 0);
    for scenario in 0..150 {
        let rate = random.pick(&[8_000.0, 44_100.0, 48_000.0, 192_000.0]);
        let mut voice = voice(&mut random, rate);
        if scenario % 5 == 1 {
            // A pitch envelope far faster than its scale, which holds within a
            // sample of its key-on.
            voice.parameters.rates = [1e7; 4];
            let start = Instant::at(0);
            voice.envelope.key_on(start, &voice.parameters, rate);
        }
        let mut parts = Parts::new(256).expect("memory");
        let length = if scenario % 10 == 0 { 2000 } else { 120 };
        for n in 1..=length {
            let from = Instant::at(n - 1);
            sample(&mut parts, from, &voice.signals());
            let count = checked(&parts, from, &voice);
            most = most.max(count);
            if count == 256 {
                fallbacks += 1;
            }
            let to = Instant::at(n);
            advance(
                &mut voice.envelope,
                &voice.parameters,
                &mut voice.portamento,
                to,
                voice.sample_rate,
            );
            if n == length / 4 {
                // A change of the levels re-anchors the envelope where it is,
                // a hold among the places, which moves toward its new level
                // with no segment's end: its corner is where it gets there.
                voice.parameters.levels = [voice.parameters.levels[3], 30.0, 70.0, 45.5];
                voice.envelope.change(to, &voice.parameters, voice.sample_rate);
                held += usize::from(voice.envelope.end().is_none() && voice.envelope.corner().is_some());
            }
            if n == length / 2 {
                voice.envelope.key_off(to, &voice.parameters, voice.sample_rate);
            }
            samples += 1;
        }
    }
    println!("walks: {samples} samples, at most {most} parts in one, {fallbacks} past the budget, {held} holds moving");
    assert!(fallbacks > 0 && most == 256, "some samples past the budget");
    assert!(held > 5, "holds moved by a change");
}

/// 2⁻⁶⁰, the rule's truncation relative to a part's integral (MATH §2.5).
const TRUNCATION: f64 = f64::from_bits((1023 - 60) << 52);

/// A span's increment at the ratio mode's unit ratio and no constant, ∫2^λ
/// over the span, its parts' increments summed, with the bound of its error:
/// each part's within 12u of its exact value in closed form and within 8u and
/// the rule's truncation, 2⁻⁶⁰ of it, by the rule, and λ's rounding moves it by
/// ln 2·u/2·|λ| relatively (`keptone_engine::pieces`); the sum adds u of each
/// partial sum.
fn increment_of(parts: &Parts) -> (f64, f64) {
    let (mut sum, mut bound) = (0.0, 0.0);
    for part in parts.as_slice() {
        let value = part.increment(1.0, 0.0, false);
        let largest = match *part {
            Part::Linear {
                length, pitch, slope, ..
            } => pitch.abs().max(slope.mul_add(length, pitch).abs()),
            Part::Rule { pitches, .. } => pitches.into_iter().fold(0.0_f64, |most, pitch| most.max(pitch.abs())),
        };
        sum += value;
        bound += value.abs() * (12.0 * U + TRUNCATION + 0.35 * U * largest) + U * sum.abs();
    }
    (sum, bound)
}

#[test]
fn spans_follow_their_signals() {
    // A span of a sample, up to an instant within it, is walked as the sample
    // is: its parts follow the signals' laws and tile it. A sample walked in
    // two spans, up to an instant and on from there, the signals taken there,
    // integrates as the sample walked whole, within the three walks' bounds,
    // where none of them is past the budget and the LFO's wave has no jumps,
    // whose instants each walk finds within its phase's error over its step,
    // which its jump would then carry.
    let mut random = Random(0x7370_616e);
    let (mut spans, mut compared, mut worst) = (0, 0, 0.0_f64);
    for _ in 0..120 {
        let rate = random.pick(&[8_000.0, 44_100.0, 48_000.0, 192_000.0]);
        let mut voice = voice(&mut random, rate);
        let smooth = matches!(voice.wave, Wave::Triangle | Wave::Sine);
        let (mut whole, mut first, mut second) = (
            Parts::new(256).expect("memory"),
            Parts::new(256).expect("memory"),
            Parts::new(256).expect("memory"),
        );
        for n in 1..=60 {
            let from = Instant::at(n - 1);
            let length = random.pick(&[0.5, 0.25, 1e-9, 1.0 - 1e-12, 0.0]) + random.uniform(0.0, 0.1);
            let length = length.clamp(1e-12, 1.0 - 1e-12);
            sample(&mut whole, from, &voice.signals());
            span(&mut first, from, length, &voice.signals());
            checked_over(&first, from, length, &voice);
            let at = from.after(length).expect("within the sample");
            let rest = voice.clone_at(at);
            span(&mut second, at, 1.0 - length, &rest.signals());
            let limits = [&whole, &first, &second].map(|parts| parts.as_slice().len() >= 256);
            if smooth && !limits.contains(&true) {
                let ((a, a_bound), (b, b_bound), (c, c_bound)) =
                    (increment_of(&first), increment_of(&second), increment_of(&whole));
                // Each walk's λ lies within the voice's tolerance of the
                // signals' laws, which moves an increment by ln 2 times it
                // relatively; a glissando's steps, at instants that each walk
                // finds within u of a sample, carry a twelfth of an octave
                // over that; and τ and 1 − τ round within u each, moving the
                // two spans' sum by the integrand's largest value times 2u.
                let tolerance = [0.0, 0.5, 1.0 - 1e-9]
                    .map(|x| voice.at(from.after(x).expect("within the sample")).2)
                    .into_iter()
                    .fold(0.0, f64::max);
                let largest = [&whole, &first, &second]
                    .iter()
                    .flat_map(|parts| {
                        parts
                            .as_slice()
                            .iter()
                            .map(|part| part.increment(1.0, 0.0, false) / part.length())
                    })
                    .fold(0.0, f64::max);
                let bound = a_bound
                    + b_bound
                    + c_bound
                    + U * (a + b).abs()
                    + 2.0 * core::f64::consts::LN_2 * tolerance * (a.abs() + b.abs() + c.abs())
                    + 8.0 * U * largest;
                let error = ((a + b) - c).abs();
                assert!(
                    error <= bound,
                    "sample {n}, span {length}: {a} + {b} against {c}, error {error:e} > {bound:e}"
                );
                worst = worst.max(error / bound);
                compared += 1;
            }
            spans += 1;
            advance(
                &mut voice.envelope,
                &voice.parameters,
                &mut voice.portamento,
                Instant::at(n),
                voice.sample_rate,
            );
        }
    }
    println!("spans: {spans} spans, {compared} compared with their samples, the worst {worst:.3} of its bound");
    assert!(compared > 1500, "{compared} samples compared");
}

#[test]
fn a_swinging_key_never_holds_still() {
    // A pitch envelope at rest, no portamento and a sine LFO: the level's key
    // under `scaling.key = pitch` is one piece over the sample, with neither a
    // slope nor a glide, which the LFO swings; it does not hold still, so that
    // an envelope driven at it never takes its closed form. Without the LFO's
    // depth it holds still at the base and the bend.
    let rate = 48_000.0;
    let start = Instant::at(0);
    let parameters = peg::Parameters {
        rates: [99.0; 4],
        levels: [50.0; 4],
    };
    let envelope = PitchEnvelope::new(&parameters, start);
    let mut lfo = Lfo::new(start, 60.0, rate, 7, 0);
    lfo.key_on(start, true, 0.0, rate);
    let signals = |depth: f64| Signals {
        pitch_envelope: envelope,
        pitch_parameters: &parameters,
        portamento: Portamento::Still,
        lfo: &lfo,
        wave: Wave::Sine,
        sensitivity: pitch_sensitivity(7.0),
        depth,
        controllers: controllers(&[]),
        sample_rate: rate,
    };
    let mut keys = Keys::new(64).expect("memory");
    keys.pitch_walk(Instant::at(100), 52.75, -1.5, &signals(99.0));
    let pieces = keys.pieces().expect("the walk's pieces");
    assert!(
        pieces.len() == 1 && pieces[0].key.swing.is_some() && pieces[0].key.slope == 0.0,
        "{pieces:?}"
    );
    assert_eq!(keys.still(), None);
    keys.pitch_walk(Instant::at(100), 52.75, -1.5, &signals(0.0));
    assert_eq!(keys.still(), Some(51.25));
}

#[test]
fn pitch_keys_follow_their_signals() {
    // The level's key under `scaling.key = pitch` over samples of random
    // voices (MATH §2.6): a base plus twelve keys an octave of the pitch that
    // the pitch envelope, the portamento and the LFO move, plus the bend,
    // against the signals' own laws at interior instants of its pieces, which
    // tile the sample, with a swing where a sine moves it or a fade moves a
    // triangle's or a saw's depth; past the budget, the key at the rule's
    // instants of its equal parts.
    let mut random = Random(0x7069_7463);
    let (base, bend) = (52.75, -1.5);
    let (mut pieces, mut swings, mut rules, mut worst) = (0, 0, 0, 0.0_f64);
    for _ in 0..150 {
        let rate = random.pick(&[8_000.0, 44_100.0, 48_000.0, 192_000.0]);
        let mut voice = voice(&mut random, rate);
        let mut keys = Keys::new(64).expect("memory");
        for n in 1..=120 {
            let from = Instant::at(n - 1);
            keys.pitch_walk(from, base, bend, &voice.signals());
            // The key takes λ from the same signals' laws as the walk's parts,
            // in keys: twelve times their tolerance (Voice::at), and the
            // products by 12 and the sums with the base and the bend within 4u
            // of the key on either side.
            let check = |at: Instant, got: f64| {
                let (pitch, _, tolerance) = voice.at(at);
                let expected = 12.0_f64.mul_add(pitch, base) + bend;
                let bound = 12.0 * tolerance + 8.0 * U * expected.abs().max(64.0);
                assert!((got - expected).abs() <= bound, "sample {n}: {got} against {expected}");
                (got - expected).abs() / bound
            };
            if let Some(walked) = keys.pieces() {
                // A key that the LFO swings never holds still.
                if walked.iter().any(|piece| piece.key.swing.is_some()) {
                    assert_eq!(keys.still(), None, "sample {n}");
                }
                let mut covered = 0.0;
                for piece in walked {
                    swings += usize::from(piece.key.swing.is_some());
                    for share in [0.25, 0.5, 0.75] {
                        let x = share * piece.length;
                        let at = from.after(piece.start + x).expect("within the sample");
                        worst = worst.max(check(at, piece.key.at(x)));
                    }
                    covered += piece.length;
                }
                let count = f64::from(u32::try_from(walked.len()).expect("few"));
                assert!((covered - 1.0).abs() <= 8.0 * U * count, "{covered}");
                pieces += walked.len();
            } else {
                let nodes = keys.nodes();
                assert_eq!(nodes.len(), 64, "the rule's parts");
                for (index, nodes) in (0_u32..).zip(nodes) {
                    let boundary = |index: u32| if index >= 64 { 1.0 } else { f64::from(index) / 64.0 };
                    let part = boundary(index + 1) - boundary(index);
                    for (node, instant) in nodes.iter().zip(gauss_instants(part)) {
                        let at = from.after(boundary(index) + instant).expect("within the sample");
                        worst = worst.max(check(at, *node));
                    }
                }
                rules += 1;
            }
            advance(
                &mut voice.envelope,
                &voice.parameters,
                &mut voice.portamento,
                Instant::at(n),
                voice.sample_rate,
            );
        }
    }
    println!(
        "pitch keys: {pieces} pieces, {swings} of them swung, {rules} samples of the rule, the worst {worst:.3} of its bound"
    );
    assert!(
        swings > 500 && rules > 100,
        "{swings} swung, {rules} samples of the rule"
    );
}

#[test]
fn a_span_walks_the_lfo_cells_within_it() {
    // A triangle whose sample holds some 300 cells, past a budget of 256, and
    // a span of half the sample, which holds some 150: the sample takes the
    // rule on equal parts, and the span is walked, each of its cells a part.
    let rate = 48_000.0;
    let start = Instant::at(0);
    let parameters = peg::Parameters {
        rates: [99.0; 4],
        levels: [50.0; 4],
    };
    let at_speed = |speed: f64| {
        let mut lfo = Lfo::new(start, speed, rate, 7, 0);
        lfo.key_on(start, true, 0.0, rate);
        lfo
    };
    // The speed at which a sample holds 300 half-turn cells, 150 turns, found
    // by bisection, the step rising with the speed.
    let (mut low, mut high) = (0.0, 1e30);
    for _ in 0..200 {
        let middle = f64::midpoint(low, high);
        if at_speed(middle).step() < 150.0 {
            low = middle;
        } else {
            high = middle;
        }
    }
    let voice = Voice {
        envelope: PitchEnvelope::new(&parameters, start),
        parameters,
        portamento: Portamento::Still,
        lfo: at_speed(high),
        wave: Wave::Triangle,
        sensitivity: pitch_sensitivity(7.0),
        depth: 99.0,
        controllers: controllers(&[]),
        sample_rate: rate,
    };
    let mut parts = Parts::new(256).expect("memory");
    sample(&mut parts, start, &voice.signals());
    assert_eq!(parts.as_slice().len(), 256, "the sample past the budget");
    span(&mut parts, start, 0.5, &voice.signals());
    let count = checked_over(&parts, start, 0.5, &voice);
    assert!((140..256).contains(&count), "{count} parts in the span");
}

#[test]
fn walks_meet_every_edge() {
    // The LFO's waves of corners and edges at audio rates and their scales'
    // full depth, from a few corners in a sample to dozens: each sample's
    // pieces meet them all, each its own part.
    let mut random = Random(0x6564_6765);
    let mut edges = 0;
    for scenario in 0..48 {
        let rate = [8_000.0, 22_050.0, 48_000.0][scenario % 3];
        let wave = [Wave::Triangle, Wave::SawDown, Wave::SawUp, Wave::Square][scenario / 3 % 4];
        let speed = random.pick(&[300.0, 700.0, 1500.0, 2500.0]);
        let start = Instant::at(0);
        let parameters = peg::Parameters {
            rates: [99.0; 4],
            levels: [50.0; 4],
        };
        let mut lfo = Lfo::new(start, speed, rate, random.next(), 0);
        lfo.key_on(start, true, 0.0, rate);
        let voice = Voice {
            envelope: PitchEnvelope::new(&parameters, start),
            parameters,
            portamento: Portamento::Still,
            lfo,
            wave,
            sensitivity: pitch_sensitivity(7.0),
            depth: 99.0,
            controllers: controllers(&[]),
            sample_rate: rate,
        };
        let mut parts = Parts::new(256).expect("memory");
        for n in 1..=400 {
            let from = Instant::at(n - 1);
            sample(&mut parts, from, &voice.signals());
            edges += checked(&parts, from, &voice) - 1;
        }
    }
    println!("edges: {edges}");
    assert!(edges > 30_000, "{edges} corners and edges");
}

#[test]
fn walks_follow_the_delay() {
    // The LFO's delay through its stages, at a depth that its sum leaves
    // below 255/256 and one beyond the scale, whose part beyond it passes the
    // saturation, the delay restarted by a key-on during its rise and changed
    // during it, a rise within a sample, and the depth's corner reached within
    // a rise: the walk's pieces end where the stages and the corner do.
    let mut random = Random(0x0064_656c_6179);
    let (mut rising, mut cornered) = (0, 0);
    for scenario in 0..36 {
        let rate = [8_000.0, 48_000.0, 192_000.0][scenario % 3];
        let wave = [Wave::Triangle, Wave::Sine, Wave::SawUp, Wave::Square][scenario / 3 % 4];
        let delay = random.pick(&[1e-4, 0.01, 0.05, 0.3]);
        let start = Instant::at(0);
        let parameters = peg::Parameters {
            rates: [99.0; 4],
            levels: [50.0; 4],
        };
        let mut lfo = Lfo::new(start, random.pick(&[35.0, 99.0, 300.0]), rate, random.next(), 0);
        lfo.key_on(start, true, delay, rate);
        let wheel = Source {
            position: random.uniform(0.2, 0.9),
            range: 99.0,
        };
        let mut voice = Voice {
            envelope: PitchEnvelope::new(&parameters, start),
            parameters,
            portamento: Portamento::Still,
            lfo,
            wave,
            sensitivity: pitch_sensitivity(7.0),
            depth: random.pick(&[60.0, 200.0]),
            controllers: controllers(&[wheel]),
            sample_rate: rate,
        };
        let mut parts = Parts::new(256).expect("memory");
        let length = if rate > 100_000.0 { 3000 } else { 900 };
        let mut changed = false;
        for n in 1..=length {
            let from = Instant::at(n - 1);
            sample(&mut parts, from, &voice.signals());
            checked(&parts, from, &voice);
            let fading = voice.lfo.fading(from);
            if fading.rise.is_none() && fading.full.is_some_and(|full| full < 1.0) {
                rising += 1;
            }
            if parts.as_slice().len() > 1 && fading.full.is_some() && voice.depth < 100.0 {
                cornered += 1;
            }
            if n == length / 3 {
                // A key-on during the rise restarts the delay from 0.
                voice.lfo.key_on(Instant::at(n), false, delay, rate);
            }
            if !changed && n > length / 3 && fading.rise.is_none() && fading.full.is_some() {
                // A change of DELAY during the rise carries over the share of
                // it that has passed (MATH §6.5).
                voice
                    .lfo
                    .set_delay(Instant::at(n), random.pick(&[0.02, 0.1, 0.3]), rate);
                changed = true;
            }
        }
    }
    println!("delays: {rising} samples in which a rise ends, {cornered} split during a rise");
    assert!(
        rising > 30 && cornered > 30,
        "{rising} rises ending, {cornered} samples split"
    );
}

#[test]
fn a_sample_past_the_budget_takes_the_rule_on_equal_parts() {
    // A sample and hold that draws thousands of values in a sample.
    let rate = 48_000.0;
    let start = Instant::at(0);
    let parameters = peg::Parameters {
        rates: [99.0; 4],
        levels: [50.0; 4],
    };
    let mut lfo = Lfo::new(start, 2_000_000.0, rate, 7, 0);
    lfo.key_on(start, true, 0.0, rate);
    let voice = Voice {
        envelope: PitchEnvelope::new(&parameters, start),
        parameters,
        portamento: Portamento::Still,
        lfo,
        wave: Wave::SampleAndHold,
        sensitivity: pitch_sensitivity(7.0),
        depth: 99.0,
        controllers: controllers(&[]),
        sample_rate: rate,
    };
    assert!(voice.lfo.step() > 1000.0);
    let mut parts = Parts::new(16).expect("memory");
    sample(&mut parts, start, &voice.signals());
    assert_eq!(parts.as_slice().len(), 16);
    let mut covered = 0.0;
    for part in parts.as_slice() {
        let Part::Rule { length, .. } = *part else {
            panic!("the rule's parts")
        };
        assert!((length - 1.0 / 16.0).abs() <= U, "{length}");
        covered += length;
    }
    assert_eq!(covered, 1.0);
    checked(&parts, start, &voice);
    // Just below the step at which the phase stands still, 2⁵³ turns a
    // sample, the sample's corners lie closer than binary64 tells its phases
    // apart: the walk takes the rule's equal parts at once.
    let mut fast = Lfo::new(start, 1.9e11, rate, 7, 0);
    fast.key_on(start, true, 0.0, rate);
    assert!((1_125_899_906_842_624.0..9_007_199_254_740_992.0).contains(&fast.step()));
    let near = Voice {
        envelope: PitchEnvelope::new(&voice.parameters, start),
        parameters: voice.parameters,
        portamento: Portamento::Still,
        lfo: fast,
        wave: Wave::Triangle,
        sensitivity: voice.sensitivity,
        depth: 99.0,
        controllers: controllers(&[]),
        sample_rate: rate,
    };
    sample(&mut parts, start, &near.signals());
    assert!(
        parts.as_slice().len() == 16 && parts.as_slice().iter().all(|part| matches!(part, Part::Rule { .. })),
        "{:?}",
        parts.as_slice().first()
    );
    // The same LFO at its speed on the scale walks the sample exactly.
    let mut slow = voice;
    slow.lfo = Lfo::new(start, 99.0, rate, 7, 0);
    sample(&mut parts, start, &slow.signals());
    assert_eq!(parts.as_slice().len(), 1);
}

#[test]
fn a_glissando_advances_as_its_steps_do() {
    // A glissando over thousands of keys within a few samples: its closed
    // form at each instant is the state that its steps one by one leave.
    let rate = 48_000.0;
    let kappa = portamento::speed(0.0001).expect("a glide");
    let mut stepped = Glissando::new(Instant::at(0), -3000.5, kappa, rate);
    let mut jumped = stepped;
    for n in 1..=40_u64 {
        let to = Instant::at(n);
        let mut steps = 0;
        while stepped.end().is_some_and(|end| end <= to) {
            stepped.next();
            steps += 1;
        }
        jumped.advance(to);
        assert_eq!(stepped, jumped, "at sample {n}, after {steps} steps");
        assert_eq!(Glissando::offset_at(&stepped, to), stepped.offset());
    }
    assert_eq!(stepped.offset(), 0.0);
}
