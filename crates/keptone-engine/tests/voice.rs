// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! One voice (docs/MATH.md §1, §13) against the laws that it composes, each
//! of which its own tests check against its references: its phases against
//! its operators' frequencies of §2.2–§2.4 in the ratio and the fixed mode,
//! within a bound from rounding analysis, and against OSC KEY SYNC and each
//! `osc.free_phase_start` at its key-ons; its operators' levels and gains
//! against envelopes driven apart at the rates' key and offsets taken at the
//! scaling key of each `scaling.key`, the amplitude modulation and the EG
//! bias from its part's LFO and from its own under `lfo.scope = voice`, and
//! its switches' factors, bit for bit; its end (§13.3); the values within a
//! sample that the relaxation takes (§9.6) against its states at the sample's
//! start taken over the part of the sample up to each instant; a voice taken
//! while it sounds and a new one, its envelopes and its feedback; and the
//! carriers' normalization (§14).

use keptone_engine::compile::CompiledPatch;
use keptone_engine::configuration::Configuration;
use keptone_engine::control::{Limited, amplitude_sensitivity, kept, lowering, modulation};
use keptone_engine::eg::end_level;
use keptone_engine::eg::envelope::{Envelope, Parameters, Stage};
use keptone_engine::feedback::kappa;
use keptone_engine::level::{distance, gain, keyboard_scaling, offset};
use keptone_engine::lfo::Lfo;
use keptone_engine::peg::{self, PitchEnvelope};
use keptone_engine::pieces::{self, Keys, Parts, Portamento, Signals, advance};
use keptone_engine::pitch::detune;
use keptone_engine::pitch::{TwelveTet, transposed_key};
use keptone_engine::random::{self, Streams};
use keptone_engine::time::Instant;
use keptone_engine::voice::{Context, Note, Voice};
use keptone_math::eft::two_product;
use keptone_math::{exp2, log2, sin2pi};
use keptone_model::function::Function;
use keptone_model::patch::Patch;
use keptone_model::settings::{self, Settings, Value};
use keptone_model::voice::{Algorithm, Curve, Mode, Value as Parameter, Wave};

/// u = 2⁻⁵³, the unit roundoff of binary64.
const U: f64 = f64::EPSILON / 2.0;

/// The engine's rate of these tests.
const RATE: f64 = 48_000.0;

/// Settings at their defaults but for the three that `keptone-tools render`
/// sets to M3's values (DESIGN §16), and for some changes.
fn settings(changes: &[(&str, Value)]) -> Settings {
    let mut settings = Settings::new();
    let m3 = [
        ("engine.oversampling", Value::Integer(1)),
        ("adaa.order", Value::Choice(0)),
        ("feedback.series_beta_max", Value::Number(0.0)),
    ];
    for &(name, value) in m3.iter().chain(changes) {
        assert!(
            settings.set(settings::number_of(name).expect("a setting"), value),
            "{name}"
        );
    }
    settings
}

/// A parameter of a scale.
fn value<const MAX: u8>(value: f64) -> Parameter<MAX> {
    Parameter::new(value).expect("a finite value")
}

/// A count of samples below 2³² as a binary64.
fn count(samples: u64) -> f64 {
    f64::from(u32::try_from(samples).expect("a short test"))
}

/// The distance between two phases on the circle, in turns.
fn apart(a: f64, b: f64) -> f64 {
    let difference = a - b;
    (difference - difference.round()).abs()
}

/// The fraction of n·step, from the exact product: within 2u of it.
fn fraction(step: f64, samples: u64) -> f64 {
    let (high, low) = two_product(step, count(samples));
    let part = (high - high.floor()) + low;
    part - part.floor()
}

/// No controller.
const NONE: Limited = Limited {
    value: 0.0,
    on_scale: 0.0,
};

/// What a voice's part gives it, the part's LFO from sample 0 on.
struct Rig {
    configuration: Configuration,
    tuning: TwelveTet,
    lfo: Lfo,
    patch: CompiledPatch,
    streams: Streams,
    note_shift: f64,
    bend: f64,
    master_tune: f64,
    pitch_controllers: Limited,
    amplitude_controllers: Limited,
    bias: Limited,
}

impl Rig {
    /// A part of a patch in settings changed so.
    fn new(changes: &[(&str, Value)], patch: &Patch) -> Self {
        let configuration = Configuration::new(&settings(changes), RATE).expect("a configuration");
        let streams = Streams::of(configuration.lfo_seed, 0);
        let patch = CompiledPatch::new(patch, &Function::initial(), &configuration);
        let lfo = Lfo::new(Instant::at(0), patch.lfo.speed, RATE, streams.part, 0);
        Self {
            tuning: TwelveTet::new(configuration.a4_hz),
            configuration,
            lfo,
            patch,
            streams,
            note_shift: 0.0,
            bend: 0.0,
            master_tune: 0.0,
            pitch_controllers: NONE,
            amplitude_controllers: NONE,
            bias: NONE,
        }
    }

    /// What a voice takes from it.
    const fn context(&self) -> Context<'_> {
        Context {
            patch: &self.patch,
            configuration: &self.configuration,
            tuning: &self.tuning,
            lfo: &self.lfo,
            note_shift: self.note_shift,
            pitch_controllers: self.pitch_controllers,
            amplitude_controllers: self.amplitude_controllers,
            bias: self.bias,
            bend: self.bend,
            master_tune: self.master_tune,
            streams: self.streams,
        }
    }

    /// A voice of six operators from sample 0 on.
    fn voice(&self) -> Voice {
        Voice::new(6, self.configuration.max_pieces, Instant::at(0)).expect("memory")
    }

    /// The key k′ of a key (MATH §2.1).
    fn transposed(&self, key: f64) -> f64 {
        transposed_key(
            key,
            self.patch.transpose,
            self.note_shift,
            self.configuration.transpose_octave,
            TwelveTet::PERIOD,
        )
    }

    /// An operator's offset at a scaling key and a velocity (MATH §3.2).
    fn offset(&self, k: usize, key: f64, velocity: f64) -> Option<f64> {
        let operator = &self.patch.operators[k];
        let scaling = keyboard_scaling(distance(key, operator.break_point), operator.left, operator.right);
        offset(operator.output_level, scaling, operator.velocity_sensitivity, velocity)
    }
}

/// A note.
const fn note(key: f64, velocity: f64, per_note: f64) -> Note {
    Note {
        key,
        velocity,
        per_note,
    }
}

/// One engine sample n of a voice from 1 on: the sample from n − 1 integrated,
/// then its output at n.
fn step(voice: &mut Voice, context: &Context<'_>, n: u64) -> f64 {
    voice.advance(Instant::at(n - 1), context);
    voice.output(Instant::at(n), context)
}

/// A bound of the difference between a phase after n samples and n times the
/// increment that the test computes, step: each of the two pitches, the
/// voice's and the test's, sums at most five terms whose partial sums lie
/// within M of 0, each rounding within u·M, log₂ 440's 0.81 ulp and 12-TET's
/// twelfth within 3u·M more, so within 8u·M each; their exponentials differ
/// by ln 2·16u·M < 12u·M relatively, beside the voice's increment's 12u
/// (`linear_increment`), its ratio's quotient by the rate, u, and the test's
/// exp2, product and quotient, 4u: within (12M + 17)u·step a sample. The
/// phase's two-sums keep the voice's sum within 2⁻⁵³ + n·2⁻¹⁰⁵ of the sum of
/// its increments, and the test's fraction lies within 2u of n·step.
fn phase_bound(step: f64, samples: u64, magnitude: f64) -> f64 {
    count(samples) * step * (12.0 * magnitude + 17.0) * U + 4.0 * U
}

#[test]
fn the_phases_run_at_their_operators_frequencies() {
    let mut patch = Patch::init();
    patch.voice.transpose = value(26.0);
    patch.voice.pitch_envelope.levels = [value(60.0); 4];
    patch.voice.algorithm = Algorithm::new(1).expect("an algorithm");
    let ratio_operator = &mut patch.voice.operators[0];
    ratio_operator.coarse = value(2.0);
    ratio_operator.fine = value(50.0);
    ratio_operator.detune = value(10.0);
    let fixed_operator = &mut patch.voice.operators[1];
    fixed_operator.mode = Mode::Fixed;
    fixed_operator.coarse = value(1.0);
    fixed_operator.fine = value(30.0);
    fixed_operator.output_level = value(60.0);
    let mut rig = Rig::new(&[], &patch);
    rig.note_shift = -1.0;
    rig.bend = 1.5;
    rig.master_tune = 0.02;
    let context = rig.context();
    let mut voice = rig.voice();
    voice.key_on(Instant::at(0), note(60.0, 100.0, 0.25), Portamento::Still, 0, &context);
    voice.output(Instant::at(0), &context);
    let samples = 4800;
    for n in 1..=samples {
        step(&mut voice, &context, n);
    }
    // k′ = 60 + 2 − 1; the bend and the per-note pitch in octaves.
    assert_eq!(voice.transposed(), 61.0);
    let key = log2(440.0) + (61.0 - 69.0) / 12.0;
    let moved = (1.5 + 0.25) / 12.0;
    let ratio_pitch = key + detune(10.0, key) + moved + 0.02 + peg::level(60.0);
    let fixed_pitch = rig.patch.operators[1].fixed_pitch + moved;
    let at = Instant::at(samples);
    for (k, ratio, pitch) in [(0, 3.0, ratio_pitch), (1, 1.0, fixed_pitch)] {
        let step = ratio * exp2(pitch) / RATE;
        let probe = voice.probe(k, at, &context).expect("a probe");
        let magnitude = log2(440.0).max(pitch.abs()) + 1.0;
        let error = apart(probe.phase, fraction(step, samples));
        let bound = phase_bound(step, samples, magnitude);
        println!("operator {k}'s phase: {:.3} of its bound", error / bound);
        assert!(error <= bound, "operator {k}: {error:e} > {bound:e}");
        // The probe's pitch: the same sums in two orders.
        assert!(
            (probe.pitch - pitch).abs() <= 16.0 * U * magnitude,
            "operator {k}'s pitch"
        );
    }
}

/// A patch whose OP1 holds its level after a fast attack and falls fast after
/// its key-off, with OSC KEY SYNC as given.
fn held(sync: bool) -> Patch {
    let mut patch = Patch::init();
    patch.voice.oscillator_sync = sync;
    for operator in &mut patch.voice.operators {
        operator.output_level = value(90.0);
        operator.envelope.rates = [value(99.0); 4];
        operator.envelope.levels = [value(99.0), value(99.0), value(99.0), value(0.0)];
    }
    patch
}

/// Runs a voice from sample `from` up to and with `to`, giving each sample's
/// output, and the sample at which it ends where it ends, recorded there.
fn run(voice: &mut Voice, context: &Context<'_>, from: u64, to: u64) -> Option<u64> {
    for n in from..=to {
        step(voice, context, n);
        if voice.ending().is_none() && voice.note().is_some() && voice.ended(Instant::at(n), context) {
            voice.end(Instant::at(n));
            return Some(n);
        }
    }
    None
}

#[test]
fn a_key_on_starts_the_phases_as_key_sync_and_the_free_start_say() {
    let note = note(69.0, 100.0, 0.0);
    // OSC KEY SYNC on: 0 at every key-on, a sounding voice's too.
    let rig = Rig::new(&[], &held(true));
    let context = rig.context();
    let mut voice = rig.voice();
    voice.key_on(Instant::at(0), note, Portamento::Still, 0, &context);
    run(&mut voice, &context, 1, 100);
    voice.key_on(Instant::at(100), note, Portamento::Still, 1, &context);
    for k in 0..6 {
        assert_eq!(voice.probe(k, Instant::at(100), &context).expect("a probe").phase, 0.0);
    }
    // Off, `continue`: 0 at the first note, on from where it is for a voice taken
    // while it sounds, and run on from its end at its last increment.
    let rig = Rig::new(&[], &held(false));
    let context = rig.context();
    let mut voice = rig.voice();
    voice.key_on(Instant::at(0), note, Portamento::Still, 0, &context);
    assert_eq!(voice.probe(0, Instant::at(0), &context).expect("a probe").phase, 0.0);
    run(&mut voice, &context, 1, 100);
    let sounding = voice.probe(0, Instant::at(100), &context).expect("a probe").phase;
    assert!(sounding > 0.0);
    voice.key_on(Instant::at(100), note, Portamento::Still, 1, &context);
    assert_eq!(
        voice.probe(0, Instant::at(100), &context).expect("a probe").phase,
        sounding
    );
    run(&mut voice, &context, 101, 200);
    voice.key_off(Instant::at(200), &context);
    let end = run(&mut voice, &context, 201, 2000).expect("an end");
    let last = voice.probe(0, Instant::at(end), &context).expect("a probe").phase;
    // An ended voice does not move.
    run(&mut voice, &context, end + 1, end + 50);
    assert_eq!(voice.probe(0, Instant::at(end), &context).expect("a probe").phase, last);
    let gap = 12_345;
    voice.key_on(Instant::at(end + gap), note, Portamento::Still, 2, &context);
    let phase = voice.probe(0, Instant::at(end + gap), &context).expect("a probe").phase;
    let step = 440.0 / RATE;
    // The run-on's own error, 3u + 2u²·step·gap, and the increment's against
    // the test's, as phase_bound derives it, over the gap.
    let bound = phase_bound(step, gap, log2(440.0) + 1.0) + 4.0 * U;
    let error = apart(phase, last + step * count(gap));
    println!("the run-on: {:.3} of its bound", error / bound);
    assert!(error <= bound);
    // `zero` and `random`.
    for (start, choice) in [("zero", 1), ("random", 2)] {
        let rig = Rig::new(&[("osc.free_phase_start", Value::Choice(choice))], &held(false));
        let context = rig.context();
        let mut voice = rig.voice();
        for (nth, at) in [(0, 0), (7, 100)] {
            voice.key_on(Instant::at(at), note, Portamento::Still, nth, &context);
            for k in 0..6 {
                let phase = voice.probe(k, Instant::at(at), &context).expect("a probe").phase;
                let expected = if choice == 1 {
                    0.0
                } else {
                    let index = Streams::phase_index(nth, u64::try_from(k + 1).expect("an operator"));
                    random::phase(random::value(rig.streams.phases, index))
                };
                assert_eq!(phase, expected, "{start}, note {nth}, operator {k}");
            }
            run(&mut voice, &context, at + 1, at + 100);
        }
    }
}

/// A patch whose OP1 moves through every segment with a rate scaling, a
/// keyboard scaling on both sides and a velocity sensitivity, under a pitch
/// envelope that moves its rates' key and an LFO that moves its pitch.
fn scaled() -> Patch {
    let mut patch = Patch::init();
    patch.voice.transpose = value(27.0);
    patch.voice.pitch_envelope.rates = [value(70.0), value(60.0), value(50.0), value(80.0)];
    patch.voice.pitch_envelope.levels = [value(60.0), value(40.0), value(55.0), value(50.0)];
    patch.voice.lfo.speed = value(50.0);
    patch.voice.lfo.pitch_depth = value(40.0);
    let operator = &mut patch.voice.operators[0];
    operator.envelope.rates = [value(60.0), value(40.0), value(50.0), value(70.0)];
    operator.envelope.levels = [value(99.0), value(70.0), value(80.0), value(0.0)];
    operator.rate_scaling = value(5.0);
    operator.break_point = value(50.0);
    operator.left_depth = value(40.0);
    operator.left_curve = Curve::NegativeExponential;
    operator.right_depth = value(60.0);
    operator.right_curve = Curve::PositiveLinear;
    operator.velocity_sensitivity = value(4.0);
    operator.output_level = value(95.0);
    patch
}

/// An operator's envelope and a pitch envelope driven apart from a voice, as
/// MATH §4.7 and §5 say a voice drives them: at the rates' key of
/// `frequency`, or at a key held under `level`.
struct Driven {
    operator: usize,
    held: Option<f64>,
    pitch: PitchEnvelope,
    envelope: Envelope,
    keys: Keys,
}

impl Driven {
    /// Both from a key-on at an instant, at a key k′ and an offset there, the
    /// rates' key held at a key under `level`.
    fn new(rig: &Rig, operator: usize, key: f64, held: Option<f64>, offset: f64, at: Instant) -> Self {
        let mut pitch = PitchEnvelope::new(&rig.patch.pitch_envelope, at);
        pitch.key_on(at, &rig.patch.pitch_envelope, RATE);
        let parameters = &rig.patch.operators[operator].envelope;
        let mut driven = Self {
            operator,
            held,
            pitch,
            envelope: Envelope::new(parameters, offset, at),
            keys: Keys::new(rig.configuration.max_pieces).expect("memory"),
        };
        let rates_key = driven.rates_key(key, at);
        driven
            .envelope
            .key_on(at, parameters, &rig.configuration.envelope, rates_key, offset);
        driven
    }

    /// The rates' key at an instant (MATH §2.6).
    fn rates_key(&self, key: f64, at: Instant) -> f64 {
        self.held
            .unwrap_or_else(|| 12.0_f64.mul_add(self.pitch.level(at) + 0.0, key))
    }

    /// The parameters of its operator's envelope.
    fn parameters<'a>(&self, rig: &'a Rig) -> &'a Parameters {
        &rig.patch.operators[self.operator].envelope
    }

    /// The rates' key over a span of a sample from an instant, from the pitch
    /// envelope's state at the sample's start.
    fn keys(&mut self, rig: &Rig, key: f64, from: Instant, length: f64, start: PitchEnvelope) {
        match self.held {
            Some(held) => self.keys.hold_over(held, length),
            None => self.keys.span(
                from,
                length,
                key,
                start,
                &rig.patch.pitch_envelope,
                Portamento::Still,
                RATE,
            ),
        }
    }

    /// The sample from an instant, the offset that a segment starting within it
    /// takes given by the pitch envelope at the sample's start.
    fn sample(&mut self, rig: &Rig, key: f64, from: Instant, offset: impl Fn(Instant, PitchEnvelope) -> f64) {
        let start = self.pitch;
        self.keys(rig, key, from, 1.0, start);
        self.envelope.run(
            from,
            &self.keys,
            self.parameters(rig),
            &rig.configuration.envelope,
            |at| offset(at, start),
            |_| {},
        );
        let mut still = Portamento::Still;
        let to = from.after(1.0).expect("an instant");
        advance(&mut self.pitch, &rig.patch.pitch_envelope, &mut still, to, RATE);
    }

    /// A key-off at an instant, at a key k′ and an offset there.
    fn key_off(&mut self, rig: &Rig, key: f64, offset: f64, at: Instant) {
        let rates_key = self.rates_key(key, at);
        self.envelope
            .key_off(at, self.parameters(rig), &rig.configuration.envelope, rates_key, offset);
        self.pitch.key_off(at, &rig.patch.pitch_envelope, RATE);
    }
}

/// A way to take the scaling key: the settings that choose it, the rates' key
/// held under `scaling.rate_key = level` where they choose that, and the key
/// itself at an instant from k′ and the pitch envelope there.
struct Scaling<F> {
    changes: Vec<(&'static str, Value)>,
    held: Option<f64>,
    key: F,
}

/// The scaling key's forms of MATH §2.6, the per-note pitch 0.75: `key`, k′;
/// `onset-pitch`, k′ with the per-note pitch at the onset, under each
/// `scaling.rate_key`; and `pitch`, k′ and the per-note pitch with twelve
/// keys an octave of the pitch envelope, the portamento's offset, none here,
/// and the LFO's pitch, and the bend.
fn scalings(key: f64) -> [Scaling<KeyOf>; 4] {
    let pitch_key = move |rig: &Rig, at: Instant, pitch: PitchEnvelope| {
        let patch = &rig.patch;
        let depth = modulation(patch.lfo.pitch_depth, rig.lfo.fade(at), rig.pitch_controllers).value;
        let lfo = (patch.pitch_sensitivity * depth) * rig.lfo.wave(at, patch.lfo.wave);
        12.0_f64.mul_add((pitch.level(at) + 0.0) + lfo, key + PER_NOTE) + rig.bend
    };
    [
        Scaling {
            changes: vec![("scaling.key", Value::Choice(0))],
            held: None,
            key: Box::new(move |_, _, _| key),
        },
        Scaling {
            changes: vec![("scaling.key", Value::Choice(1))],
            held: None,
            key: Box::new(move |_, _, _| key + PER_NOTE),
        },
        Scaling {
            changes: vec![
                ("scaling.key", Value::Choice(1)),
                ("scaling.rate_key", Value::Choice(1)),
            ],
            held: Some(key + PER_NOTE),
            key: Box::new(move |_, _, _| key + PER_NOTE),
        },
        Scaling {
            changes: vec![("scaling.key", Value::Choice(2))],
            held: None,
            key: Box::new(pitch_key),
        },
    ]
}

/// A scaling key at an instant from k′ and the pitch envelope there.
type KeyOf = Box<dyn Fn(&Rig, Instant, PitchEnvelope) -> f64>;

/// The per-note pitch of the scalings' notes.
const PER_NOTE: f64 = 0.75;

/// The rig of a patch in a scaling's settings, its note shift and bend given,
/// and its note's k′.
fn scaled_rig<F>(scaling: &Scaling<F>, patch: &Patch) -> (Rig, f64) {
    let mut rig = Rig::new(&scaling.changes, patch);
    rig.note_shift = -2.0;
    rig.bend = 0.5;
    let key = rig.transposed(52.0);
    (rig, key)
}

/// OP1's offset at an instant under a scaling, from the pitch envelope's state
/// at or before it.
fn offset_under(
    rig: &Rig,
    scaling: &Scaling<impl Fn(&Rig, Instant, PitchEnvelope) -> f64>,
    at: Instant,
    pitch: PitchEnvelope,
) -> f64 {
    let mut pitch = pitch;
    let mut still = Portamento::Still;
    advance(&mut pitch, &rig.patch.pitch_envelope, &mut still, at, RATE);
    rig.offset(0, (scaling.key)(rig, at, pitch), VELOCITY).unwrap_or(0.0)
}

/// The velocity of the scalings' notes.
const VELOCITY: f64 = 80.0;

/// OP1's level and gain against an envelope driven apart under a scaling.
fn levels_follow(scaling: &Scaling<impl Fn(&Rig, Instant, PitchEnvelope) -> f64>, patch: &Patch) {
    let (rig, key) = scaled_rig(scaling, patch);
    let offset_at = |at: Instant, pitch: PitchEnvelope| offset_under(&rig, scaling, at, pitch);
    let context = rig.context();
    let mut voice = rig.voice();
    let start = Instant::at(0);
    voice.key_on(start, note(52.0, VELOCITY, PER_NOTE), Portamento::Still, 0, &context);
    let mut onset = PitchEnvelope::new(&rig.patch.pitch_envelope, start);
    onset.key_on(start, &rig.patch.pitch_envelope, RATE);
    let mut driven = Driven::new(&rig, 0, key, scaling.held, offset_at(start, onset), start);
    let end = end_level(rig.configuration.end_db);
    let (mut release, mut last) = (u64::MAX, u64::MAX);
    let mut stages = Vec::new();
    for n in 1..=1_000_000 {
        let (from, at) = (Instant::at(n - 1), Instant::at(n));
        voice.advance(from, &context);
        driven.sample(&rig, key, from, offset_at);
        // Released 1000 samples into the hold, and on for 1000 at the rest.
        if voice.stage(0) == Some(Stage::Held) && release == u64::MAX {
            release = n + 1000;
        }
        if voice.stage(0) == Some(Stage::Resting) && last == u64::MAX {
            last = n + 1000;
        }
        if n == last {
            break;
        }
        if n == release {
            voice.key_off(at, &context);
            driven.key_off(&rig, key, offset_at(at, driven.pitch), at);
        }
        voice.output(at, &context);
        let probe = voice.probe(0, at, &context).expect("a probe");
        let level = driven.envelope.level(at, offset_at(at, driven.pitch));
        assert_eq!(probe.level, level, "sample {n}");
        let expected = level.filter(|level| *level >= end).map_or(0.0, gain);
        assert_eq!(probe.gain, expected, "sample {n}");
        assert_eq!(voice.pitch_envelope(at, &context), driven.pitch.level(at), "sample {n}");
        if stages.last() != voice.stage(0).as_ref() {
            stages.extend(voice.stage(0));
        }
    }
    // The scenario passes every segment and the rest.
    let expected = [0, 1, 2]
        .map(Stage::Segment)
        .into_iter()
        .chain([Stage::Held, Stage::Segment(3), Stage::Resting]);
    assert!(expected.eq(stages.iter().copied()), "{stages:?}");
}

/// The scaled patch with its pitch envelope held still at a level, at which
/// its rates' key stands apart from k′ from the key-on on.
fn still() -> Patch {
    let mut patch = scaled();
    patch.voice.pitch_envelope.levels = [value(60.0); 4];
    patch
}

/// The scaled patch with its pitch envelope's and OP1's segments ending within
/// samples: the pitch envelope's rates beyond their scale, a segment taking
/// about half a sample, and OP1's beyond theirs, its first attack ending
/// within the first sample after its key-on, where its second starts, whose
/// speed takes the offset at its start (MATH §4.3, §4.7), while the pitch
/// envelope's second segment goes on in that sample.
fn fast() -> Patch {
    let mut patch = scaled();
    patch.voice.pitch_envelope.rates = [value(1e6); 4];
    let operator = &mut patch.voice.operators[0];
    operator.envelope.rates = [value(130.0); 4];
    operator.envelope.levels = [value(60.0), value(99.0), value(50.0), value(0.0)];
    patch
}

/// The k′ of the scalings' notes.
fn scalings_key() -> f64 {
    scaled_rig(&scalings(0.0)[0], &scaled()).1
}

#[test]
fn levels_follow_their_envelopes_at_the_scaling_keys() {
    for scaling in &scalings(scalings_key()) {
        levels_follow(scaling, &scaled());
    }
    levels_follow(&scalings(scalings_key())[0], &still());
}

/// A patch with the amplitude modulation on OP1, its LFO at a speed, a delay
/// and a wave.
fn modulated(delay: f64, wave: Wave, sync: bool) -> Patch {
    let mut patch = held(true);
    patch.voice.lfo.speed = value(60.0);
    patch.voice.lfo.delay = value(delay);
    patch.voice.lfo.wave = wave;
    patch.voice.lfo.sync = sync;
    patch.voice.lfo.amplitude_depth = value(70.0);
    patch.voice.operators[0].amplitude_sensitivity = value(3.0);
    patch
}

/// OP1's gain against its level lowered by the amplitude modulation and the
/// EG bias from an LFO's fade and wave at each instant.
fn modulation_follows(rig: &Rig, voice: &mut Voice, from: u64, to: u64, lfo: impl Fn(Instant) -> (f64, f64)) {
    let context = rig.context();
    let patch = &rig.patch;
    for n in from..=to {
        step(voice, &context, n);
        let at = Instant::at(n);
        let probe = voice.probe(0, at, &context).expect("a probe");
        let (fade, wave) = lfo(at);
        let depth = modulation(patch.lfo.amplitude_depth, fade, rig.amplitude_controllers);
        let kept = kept(rig.bias, depth, wave);
        let lowered = lowering(rig.configuration.am_max_steps, amplitude_sensitivity(3.0), kept);
        let expected = gain(probe.level.expect("a level") - lowered);
        assert_eq!(probe.gain, expected, "sample {n}");
    }
}

#[test]
fn the_amplitude_modulation_follows_the_part_s_lfo_or_the_voice_s() {
    let note = note(64.0, 100.0, 0.0);
    // The part's LFO.
    let mut rig = Rig::new(&[], &modulated(0.0, Wave::Triangle, true));
    rig.amplitude_controllers = Limited {
        value: 0.1,
        on_scale: 0.1,
    };
    rig.bias = Limited {
        value: 0.2,
        on_scale: 0.2,
    };
    let mut voice = rig.voice();
    voice.key_on(Instant::at(0), note, Portamento::Still, 0, &rig.context());
    modulation_follows(&rig, &mut voice, 1, 2000, |at| {
        (rig.lfo.fade(at), rig.lfo.wave(at, Wave::Triangle))
    });
    // The voice's own, with LFO SYNC: from phase 0 and the delay's start at each
    // key-on, its values from the voices' stream; DELAY 5 waits 2200 samples
    // and rises over 2200 more, which the windows pass.
    let scope = [("lfo.scope", Value::Choice(1))];
    for wave in [Wave::Triangle, Wave::SampleAndHold] {
        let rig = Rig::new(&scope, &modulated(5.0, wave, true));
        let mut voice = rig.voice();
        for (nth, from) in [(0_u64, 0_u64), (5, 8000)] {
            let at = Instant::at(from);
            voice.key_on(at, note, Portamento::Still, nth, &rig.context());
            let mut own = Lfo::new(at, rig.patch.lfo.speed, RATE, rig.streams.voices, nth << 32);
            own.key_on(at, false, 5.0, RATE);
            modulation_follows(&rig, &mut voice, from + 1, from + 7999, |at| {
                (own.fade(at), own.wave(at, wave))
            });
        }
    }
    // Without it: the part's phase, its values counted from the key-on in the
    // voices' stream, the delay from the key-on.
    for wave in [Wave::Triangle, Wave::SampleAndHold] {
        let rig = Rig::new(&scope, &modulated(5.0, wave, false));
        let mut voice = rig.voice();
        // At sample 30 000 the part's LFO has drawn six values.
        let (nth, from) = (3, 30_000);
        let at = Instant::at(from);
        assert_eq!(rig.lfo.draws(at), 6);
        // The voice's LFO starts only at its key-on.
        run(&mut voice, &rig.context(), 1, from);
        voice.key_on(at, note, Portamento::Still, nth, &rig.context());
        let mut fade = Lfo::new(at, 0.0, RATE, 0, 0);
        fade.key_on(at, false, 5.0, RATE);
        let first = rig.lfo.draws(at);
        let lfo = |instant: Instant| {
            let shape = if wave == Wave::SampleAndHold {
                let draws = rig.lfo.draws(instant).wrapping_sub(first);
                random::sample_hold(random::value(rig.streams.voices, Streams::voice_index(nth, draws)))
            } else {
                rig.lfo.wave(instant, wave)
            };
            (fade.fade(instant), shape)
        };
        modulation_follows(&rig, &mut voice, from + 1, from + 8000, lfo);
    }
}

#[test]
fn switches_ramp_their_operators_gains() {
    // Algorithm 16's one carrier, OP1, falls slowly after its key-off.
    let mut patch = held(true);
    patch.voice.algorithm = Algorithm::new(16).expect("an algorithm");
    patch.voice.operators[0].envelope.rates[3] = value(30.0);
    let rig = Rig::new(&[], &patch);
    let context = rig.context();
    let mut voice = rig.voice();
    voice.key_on(Instant::at(0), note(60.0, 100.0, 0.0), Portamento::Still, 0, &context);
    run(&mut voice, &context, 1, 100);
    // Off over param.level_glide_s, 96 samples at 48 kHz, from 1.
    let glide = rig.configuration.level_glide_s * RATE;
    voice.switch(Instant::at(100), 0, false, &context);
    let factor = |voice: &Voice, n: u64| {
        let probe = voice.probe(0, Instant::at(n), &context).expect("a probe");
        let expected = gain(probe.level.expect("a level"));
        (probe.gain, expected)
    };
    for n in 101..=140 {
        step(&mut voice, &context, n);
        let (actual, full) = factor(&voice, n);
        assert_eq!(actual, full * (1.0 - count(n - 100) / glide).max(0.0), "sample {n}");
    }
    // On again from where it is; a switch to where it moves changes nothing.
    voice.switch(Instant::at(140), 0, true, &context);
    let from = (1.0 - 40.0 / glide).max(0.0);
    for n in 141..=300 {
        step(&mut voice, &context, n);
        if n == 150 {
            voice.switch(Instant::at(n), 0, true, &context);
        }
        let (actual, full) = factor(&voice, n);
        assert_eq!(actual, full * (from + count(n - 140) / glide).min(1.0), "sample {n}");
    }
    // A held voice does not end, though its carrier is off, its factor at 0;
    // released, it ends once the factor reaches 0, its envelope still above
    // the end level.
    voice.switch(Instant::at(300), 0, false, &context);
    for n in 301..=500 {
        step(&mut voice, &context, n);
        assert!(!voice.ended(Instant::at(n), &context), "sample {n}");
        let (actual, full) = factor(&voice, n);
        assert_eq!(actual, full * (1.0 - count(n - 300) / glide).max(0.0), "sample {n}");
    }
    voice.switch(Instant::at(500), 0, true, &context);
    run(&mut voice, &context, 501, 700);
    voice.switch(Instant::at(700), 0, false, &context);
    voice.key_off(Instant::at(710), &context);
    for n in 701..=900 {
        step(&mut voice, &context, n);
        let off = n >= 796;
        assert_eq!(voice.ended(Instant::at(n), &context), off, "sample {n}");
    }
    // An operator switched off in its patch is off from its voice's first note.
    let mut patch = held(true);
    patch.voice.operators[0].on = false;
    let rig = Rig::new(&[("param.level_glide_s", Value::Number(0.0))], &patch);
    let context = rig.context();
    let mut voice = rig.voice();
    voice.key_on(Instant::at(0), note(60.0, 100.0, 0.0), Portamento::Still, 0, &context);
    for n in 1..=50 {
        step(&mut voice, &context, n);
        assert_eq!(voice.probe(0, Instant::at(n), &context).expect("a probe").gain, 0.0);
        assert!(voice.probe(1, Instant::at(n), &context).expect("a probe").gain > 0.0);
    }
    // With no glide, a switch acts at once.
    voice.switch(Instant::at(50), 0, true, &context);
    let probe = voice.probe(0, Instant::at(50), &context).expect("a probe");
    assert_eq!(probe.gain, gain(probe.level.expect("a level")));
}

#[test]
fn a_voice_ends_once_its_carriers_have_ended() {
    // Algorithm 5's carriers OP1, OP3 and OP5 fall after the key-off at rates
    // that differ, OP5 the slowest; the voice ends at the sample at which the
    // last of them falls below the end level, its envelope driven apart.
    let mut patch = held(true);
    patch.voice.algorithm = Algorithm::new(5).expect("an algorithm");
    for (k, rate) in [(0, 80.0), (2, 70.0), (4, 60.0)] {
        patch.voice.operators[k].envelope.rates[3] = value(rate);
    }
    let rig = Rig::new(&[], &patch);
    let context = rig.context();
    let mut voice = rig.voice();
    let key = rig.transposed(60.0);
    let offset = rig.offset(4, key, 100.0).expect("an offset");
    voice.key_on(Instant::at(0), note(60.0, 100.0, 0.0), Portamento::Still, 0, &context);
    let mut slowest = Driven::new(&rig, 4, key, None, offset, Instant::at(0));
    let end = end_level(rig.configuration.end_db);
    let release = 1000;
    let mut ended = None;
    for n in 1..=1_000_000 {
        let at = Instant::at(n);
        voice.advance(Instant::at(n - 1), &context);
        slowest.sample(&rig, key, Instant::at(n - 1), |_, _| offset);
        if n == release {
            voice.key_off(at, &context);
            slowest.key_off(&rig, key, offset, at);
        }
        voice.output(at, &context);
        let below = slowest.envelope.level(at, offset).is_none_or(|level| level < end);
        assert_eq!(voice.ended(at, &context), n >= release && below, "sample {n}");
        if voice.ended(at, &context) {
            ended = Some(n);
            break;
        }
    }
    assert!(ended.is_some_and(|n| n > release + 1000), "{ended:?}");
}

#[test]
fn the_carriers_sum_is_normalised_by_their_count() {
    let mut patch = held(true);
    patch.voice.algorithm = Algorithm::new(5).expect("an algorithm");
    patch.voice.oscillator_sync = false;
    let mut outputs = Vec::new();
    for choice in 0..3 {
        let rig = Rig::new(&[("output.carrier_normalization", Value::Choice(choice))], &patch);
        let context = rig.context();
        let mut voice = rig.voice();
        voice.key_on(Instant::at(0), note(57.0, 100.0, 0.0), Portamento::Still, 0, &context);
        voice.output(Instant::at(0), &context);
        outputs.push((1..=200).map(|n| step(&mut voice, &context, n)).collect::<Vec<_>>());
    }
    let carriers: f64 = 3.0;
    for ((none, count), sqrt) in outputs[0].iter().zip(&outputs[1]).zip(&outputs[2]) {
        assert_eq!(*count, none / carriers);
        assert_eq!(*sqrt, none / carriers.sqrt());
    }
    assert!(outputs[0].iter().any(|output| output.abs() > 0.5));
}

/// The bound of a phase within a sample against the phase at the sample's
/// start, as a probe reads it, plus the span's increment, which the test sums
/// as the voice does: the voice's phase keeps beside its turns what its sums
/// rounded away, at most 2⁻⁵³, and its two-sums give the sum within 2⁻⁵³ of
/// it, while the test's one sum rounds within 2⁻⁵⁴.
const WITHIN: f64 = 2.0 * U + 2.0 * U + U;

/// OP1's θ and g within each sample under a scaling: θ at τ against the
/// phase at the sample's start and the span's increments, g against the
/// envelope driven apart and run over the span from its state at the sample's
/// start, at the offsets that the pitch envelope's state there gives.
/// The samples of a scenario within a sample, the one at which its key is
/// released, and the instants τ within each sample.
struct Spans {
    samples: u64,
    release: u64,
    taus: &'static [f64],
}

/// Gives the worst phase against its bound, the spans within which a segment
/// of OP1's envelope ended, and those within which the pitch envelope's did
/// too.
fn within_follows(
    scaling: &Scaling<impl Fn(&Rig, Instant, PitchEnvelope) -> f64>,
    patch: &Patch,
    spans: &Spans,
) -> (f64, usize, usize) {
    let (rig, key) = scaled_rig(scaling, patch);
    let offset_at = |at: Instant, pitch: PitchEnvelope| offset_under(&rig, scaling, at, pitch);
    let context = rig.context();
    let mut voice = rig.voice();
    voice.key_on(
        Instant::at(0),
        note(52.0, VELOCITY, PER_NOTE),
        Portamento::Still,
        0,
        &context,
    );
    voice.output(Instant::at(0), &context);
    let mut onset = PitchEnvelope::new(&rig.patch.pitch_envelope, Instant::at(0));
    onset.key_on(Instant::at(0), &rig.patch.pitch_envelope, RATE);
    let mut driven = Driven::new(
        &rig,
        0,
        key,
        scaling.held,
        offset_at(Instant::at(0), onset),
        Instant::at(0),
    );
    let mut parts = Parts::new(rig.configuration.max_pieces).expect("memory");
    let patch = &rig.patch;
    // OP1's pitch: the key's, its detune at 7 adding +0, no bend in the phase's
    // constant but the rig's, and no master tune.
    let ratio = patch.operators[0].ratio / RATE;
    let constant = rig.tuning.pitch(key) + (0.5 + PER_NOTE) / 12.0;
    let end = end_level(rig.configuration.end_db);
    let gain_at = |envelope: &Envelope, at: Instant, pitch: PitchEnvelope| {
        envelope
            .level(at, offset_at(at, pitch))
            .filter(|level| *level >= end)
            .map_or(0.0, gain)
    };
    let mut out = [(0.0, 0.0); 6];
    let (mut ends, mut both, mut worst) = (0, 0, 0.0_f64);
    for n in 1..=spans.samples {
        let (from, to) = (Instant::at(n - 1), Instant::at(n));
        let before = voice.probe(0, from, &context).expect("a probe");
        let (moving, start) = (driven.pitch, driven.envelope);
        voice.advance(from, &context);
        voice.within(0.0, &mut out, &context);
        assert_eq!(out[0], (before.phase, before.gain), "sample {n} at 0");
        driven.sample(&rig, key, from, offset_at);
        voice.within(1.0, &mut out, &context);
        let after = voice.probe(0, to, &context).expect("a probe").phase;
        assert_eq!(
            out[0],
            (after, gain_at(&driven.envelope, to, driven.pitch)),
            "sample {n} at 1"
        );
        for &tau in spans.taus {
            voice.within(tau, &mut out, &context);
            let signals = Signals {
                pitch_envelope: moving,
                pitch_parameters: &patch.pitch_envelope,
                portamento: Portamento::Still,
                lfo: &rig.lfo,
                wave: patch.lfo.wave,
                sensitivity: patch.pitch_sensitivity,
                depth: patch.lfo.pitch_depth,
                controllers: NONE,
                sample_rate: RATE,
            };
            pieces::span(&mut parts, from, tau, &signals);
            let increment = parts
                .as_slice()
                .iter()
                .fold(0.0, |sum, part| sum + part.increment(ratio, constant, false));
            let error = apart(out[0].0, before.phase + increment);
            worst = worst.max(error / WITHIN);
            assert!(error <= WITHIN, "sample {n} at {tau}");
            driven.keys(&rig, key, from, tau, moving);
            let mut span = start;
            span.run(
                from,
                &driven.keys,
                driven.parameters(&rig),
                &rig.configuration.envelope,
                |at| offset_at(at, moving),
                |_| {},
            );
            let at = from.after(tau).expect("an instant");
            let ended = span.stage() != start.stage();
            let (mut there, mut still) = (moving, Portamento::Still);
            advance(&mut there, &patch.pitch_envelope, &mut still, at, RATE);
            ends += usize::from(ended);
            both += usize::from(ended && there.stage() != moving.stage());
            assert_eq!(out[0].1, gain_at(&span, at, moving), "sample {n} at {tau}");
        }
        if n == spans.release {
            voice.key_off(to, &context);
            driven.key_off(&rig, key, offset_at(to, driven.pitch), to);
        }
        voice.output(to, &context);
    }
    (worst, ends, both)
}

#[test]
fn values_within_a_sample_are_its_start_states_taken_over_part_of_it() {
    // OP1's pitch moves with the pitch envelope and the LFO, and its
    // envelope's segments end within samples.
    let scalings = scalings(scalings_key());
    let spans = Spans {
        samples: 40_000,
        release: 20_000,
        taus: &[0.25, 0.5, 0.75],
    };
    for (k, scaling) in scalings.iter().enumerate() {
        let (worst, ends, _) = within_follows(scaling, &scaled(), &spans);
        println!("scaling {k}: phases within samples {worst:.3} of their bound; {ends} spans with ends");
        assert!(ends >= 3, "{ends}");
    }
    // Under `pitch`, where a segment of the envelope starts within a span in
    // which the pitch envelope's ended too, its offset is the pitch envelope's
    // at that instant, not at the sample's end.
    let spans = Spans {
        samples: 400,
        release: 200,
        taus: &[0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875],
    };
    let (worst, ends, both) = within_follows(&scalings[3], &fast(), &spans);
    println!("fast: phases within samples {worst:.3} of their bound; {ends} spans with ends, {both} with both");
    assert!(both >= 2, "{both}");
}

/// A patch of algorithm 1 whose OP6, a modulator with its feedback at FB 7,
/// holds its level after its key-off, while the carriers OP1 and OP3 fall
/// fast.
fn fed_back() -> Patch {
    let mut patch = held(true);
    patch.voice.feedback = value(7.0);
    patch.voice.operators[5].envelope.levels[3] = value(99.0);
    patch
}

#[test]
fn a_voice_taken_while_it_sounds_goes_on_and_a_new_one_starts_anew() {
    // Under dx7-discrete, OP6 takes the mean of its two outputs before.
    let rig = Rig::new(&[("feedback.model", Value::Choice(1))], &fed_back());
    let context = rig.context();
    let note = note(60.0, 100.0, 0.0);
    let mut voice = rig.voice();
    voice.key_on(Instant::at(0), note, Portamento::Still, 0, &context);
    voice.output(Instant::at(0), &context);
    let mut history = [0.0; 2];
    for n in 1..200 {
        step(&mut voice, &context, n);
        history = [history[1], voice.outputs()[5]];
    }
    // Taken while it sounds: its levels go on from where they are, and OP6's
    // feedback from its two outputs before, its phase 0 by OSC KEY SYNC.
    let at = Instant::at(200);
    voice.advance(Instant::at(199), &context);
    let levels: Vec<_> = (0..6)
        .map(|k| voice.probe(k, at, &context).expect("a probe").level)
        .collect();
    voice.key_on(at, note, Portamento::Still, 1, &context);
    for (k, level) in levels.iter().enumerate() {
        assert_eq!(
            voice.probe(k, at, &context).expect("a probe").level,
            *level,
            "operator {k}"
        );
    }
    voice.output(at, &context);
    // OP6's output, g·sin2pi(0 + κ·mean) as the plan forms it, the mean of
    // its outputs at the two samples before.
    let held = voice.probe(5, at, &context).expect("a probe").gain;
    let expected = held * sin2pi(kappa(7.0) * f64::midpoint(history[1], history[0]));
    assert_eq!(voice.outputs()[5], expected);
    assert!(expected.abs() > 0.01);
    // Ended while OP6 still sounds, then a new voice: its envelopes from T(L4)
    // at their offsets, as envelopes driven apart start, and OP6's feedback
    // anew, its first output g·sin 2π(0 + κ·0) = 0.
    voice.key_off(Instant::at(300), &context);
    let ended = run(&mut voice, &context, 201, 100_000).expect("an end");
    assert!(voice.outputs()[5].abs() > 0.01);
    let at = Instant::at(ended + 10);
    voice.key_on(at, note, Portamento::Still, 2, &context);
    let key = rig.transposed(60.0);
    for k in 0..6 {
        let parameters = &rig.patch.operators[k].envelope;
        let offset = rig.offset(k, key, 100.0).expect("an offset");
        let mut envelope = Envelope::new(parameters, offset, at);
        envelope.key_on(at, parameters, &rig.configuration.envelope, key, offset);
        let level = voice.probe(k, at, &context).expect("a probe").level;
        assert_eq!(level, envelope.level(at, offset), "operator {k}");
    }
    voice.output(at, &context);
    assert_eq!(voice.outputs()[5], 0.0);
}

#[test]
fn idleness_leaves_a_sounding_voice_s_feedback_alone() {
    // Algorithm 32 with FB 7: OP6's loop gain of 2π at full level lags under
    // the relaxation, in two substeps a sample at 48 kHz, which take θ and g
    // within the sample. A voice that sounds is never idle, whatever
    // `engine.idle_s` says.
    let mut patch = held(false);
    patch.voice.algorithm = Algorithm::new(32).expect("an algorithm");
    patch.voice.feedback = value(7.0);
    patch.voice.operators[5].output_level = value(99.0);
    let outputs = |idle: f64| {
        let rig = Rig::new(&[("engine.idle_s", Value::Number(idle))], &patch);
        let context = rig.context();
        let mut voice = rig.voice();
        voice.key_on(Instant::at(0), note(48.0, 127.0, 0.0), Portamento::Still, 0, &context);
        voice.output(Instant::at(0), &context);
        (1..=2000)
            .map(|n| {
                step(&mut voice, &context, n);
                voice.outputs()[5]
            })
            .collect::<Vec<_>>()
    };
    let (idle, waiting) = (outputs(0.0), outputs(0.5));
    assert_eq!(idle, waiting);
    assert!(idle.iter().all(|output| output.is_finite()) && idle.iter().any(|output| output.abs() > 0.5));
}

#[test]
fn a_voice_without_room_for_its_patch_does_nothing() {
    assert!(Voice::new(usize::MAX, 256, Instant::at(0)).is_none());
    let rig = Rig::new(&[], &held(true));
    let context = rig.context();
    let mut voice = Voice::new(5, 256, Instant::at(0)).expect("memory");
    voice.key_on(Instant::at(0), note(60.0, 100.0, 0.0), Portamento::Still, 0, &context);
    assert!(voice.note().is_none() && !voice.held());
    assert_eq!(step(&mut voice, &context, 1), 0.0);
    assert!(voice.ended(Instant::at(1), &context));
    assert!(voice.probe(0, Instant::at(1), &context).is_none());
}

#[test]
fn the_pitch_follows_the_lfo_that_the_voice_follows() {
    // OP1's phase against one accumulated from the parts of each sample that
    // pieces walks with an LFO given, the part's or one of the voice's own as
    // lfo.rs makes it, under a deep modulation of the pitch; and the probe's
    // pitch against the key's and the LFO's.
    let mut patch = Patch::init();
    patch.voice.pitch_sensitivity = value(6.0);
    patch.voice.lfo.speed = value(70.0);
    patch.voice.lfo.pitch_depth = value(99.0);
    patch.voice.lfo.delay = value(5.0);
    let key = 64.0;
    for (wave, scope, sync) in [
        (Wave::Sine, 0, false),
        (Wave::Sine, 1, true),
        (Wave::SampleAndHold, 1, true),
        (Wave::Triangle, 1, false),
        (Wave::SampleAndHold, 1, false),
    ] {
        patch.voice.lfo.wave = wave;
        patch.voice.lfo.sync = sync;
        let rig = Rig::new(&[("lfo.scope", Value::Choice(scope))], &patch);
        let context = rig.context();
        let mut voice = rig.voice();
        let (nth, on) = (4, 777);
        run(&mut voice, &context, 1, on);
        let at = Instant::at(on);
        voice.key_on(at, note(key, 100.0, 0.0), Portamento::Still, nth, &context);
        voice.output(at, &context);
        let own;
        let lfo = if scope == 0 {
            &rig.lfo
        } else {
            let patch = &rig.patch;
            own = Lfo::of_voice(&rig.lfo, at, sync, patch.lfo.delay, RATE, rig.streams.voices, nth);
            &own
        };
        let mut moving = PitchEnvelope::new(&rig.patch.pitch_envelope, at);
        moving.key_on(at, &rig.patch.pitch_envelope, RATE);
        let mut parts = Parts::new(rig.configuration.max_pieces).expect("memory");
        let (ratio, constant) = (rig.patch.operators[0].ratio / RATE, rig.tuning.pitch(key));
        let mut phase = keptone_engine::pitch::phase::Phase::ZERO;
        for n in on + 1..=on + 6000 {
            let from = Instant::at(n - 1);
            let signals = Signals {
                pitch_envelope: moving,
                pitch_parameters: &rig.patch.pitch_envelope,
                portamento: Portamento::Still,
                lfo,
                wave,
                sensitivity: rig.patch.pitch_sensitivity,
                depth: rig.patch.lfo.pitch_depth,
                controllers: NONE,
                sample_rate: RATE,
            };
            pieces::sample(&mut parts, from, &signals);
            phase.advance(
                parts
                    .as_slice()
                    .iter()
                    .fold(0.0, |sum, part| sum + part.increment(ratio, constant, false)),
            );
            let mut still = Portamento::Still;
            advance(&mut moving, &rig.patch.pitch_envelope, &mut still, Instant::at(n), RATE);
            step(&mut voice, &context, n);
            let probe = voice.probe(0, Instant::at(n), &context).expect("a probe");
            assert_eq!(
                probe.phase,
                phase.turns(),
                "{wave:?}, scope {scope}, sync {sync}, sample {n}"
            );
            let depth = modulation(rig.patch.lfo.pitch_depth, lfo.fade(Instant::at(n)), NONE).value;
            let modulated = (rig.patch.pitch_sensitivity * depth) * lfo.wave(Instant::at(n), wave);
            assert_eq!(
                probe.pitch,
                constant + ((moving.level(Instant::at(n)) + 0.0) + modulated)
            );
        }
    }
}

#[test]
fn a_per_note_pitch_moves_the_pitch_and_under_pitch_the_scaling() {
    // OP1 under the scaled patch with its pitch envelope and LFO still: a
    // per-note pitch of 12 keys given in its hold moves its pitch by an octave
    // under every scaling.key, and its level by the offset's change between
    // the scaling keys k′ and k′ + 12, on the right of the break point, under
    // `pitch` alone, the other two keeping the onset's scaling key (MATH §2.6).
    let mut patch = scaled();
    patch.voice.pitch_envelope.levels = [value(50.0); 4];
    patch.voice.lfo.pitch_depth = value(0.0);
    // At output level 60 the scaling stays below the level's limit (§0.7).
    patch.voice.operators[0].output_level = value(60.0);
    for choice in 0..3 {
        let rig = Rig::new(&[("scaling.key", Value::Choice(choice))], &patch);
        let context = rig.context();
        let mut voice = rig.voice();
        voice.key_on(
            Instant::at(0),
            note(80.0, VELOCITY, 0.0),
            Portamento::Still,
            0,
            &context,
        );
        voice.output(Instant::at(0), &context);
        for n in 1..=3000 {
            step(&mut voice, &context, n);
        }
        let at = Instant::at(3000);
        let before = voice.probe(0, at, &context).expect("a probe");
        voice.set_per_note(12.0, &context);
        let after = voice.probe(0, at, &context).expect("a probe");
        assert!(
            (after.pitch - before.pitch - 1.0).abs() <= 32.0 * U * after.pitch.abs(),
            "{choice}"
        );
        let (before, after) = (before.level.expect("a level"), after.level.expect("a level"));
        if choice == 2 {
            let key = rig.transposed(80.0);
            let moved = rig.offset(0, key + 12.0, VELOCITY).expect("an offset")
                - rig.offset(0, key, VELOCITY).expect("an offset");
            assert!(moved != 0.0);
            // Each level rounds within u of itself, and so do the differences.
            assert!((after - before - moved).abs() <= 4.0 * U * after.abs().max(before.abs()));
        } else {
            assert_eq!(after.to_bits(), before.to_bits(), "{choice}");
        }
    }
}

/// The held patch with OP1's output level at a value.
fn leveled(level: f64) -> Patch {
    let mut patch = held(true);
    patch.voice.operators[0].output_level = value(level);
    patch
}

/// A bound of the difference between a level T that a ramp moves and the
/// test's sum: the ramp's difference d and its share (1 − r)d, its target's
/// sum and the level's two sums round within u each of magnitudes below M, r
/// within 2u of itself, and the test's own sum and product within 2u·M:
/// within 9u·M.
fn ramp_bound(magnitude: f64) -> f64 {
    9.0 * U * magnitude
}

#[test]
fn a_change_ramps_each_operator_s_offset_in_t() {
    // OP1 held at L3, its envelope's own level still, under param.level_glide_s
    // of 0.01 s, 480 samples. From OL 90 to 70 its T moves linearly from the
    // one to the other; a change of its rates in the middle leaves the ramp as
    // it was; to OL 0, at which it falls silent, from where the ramp had come
    // linearly to the end level of eg.end_db, silent from that ramp's end on;
    // and back to OL 90 from the end level.
    // The rigs of OL 90, 70, 70 with R4 at 50, 0 and 90.
    let rigs_in = |changes: &[(&str, Value)]| -> Vec<Rig> {
        [90.0, 70.0, 70.0, 0.0, 90.0]
            .iter()
            .enumerate()
            .map(|(k, &level)| {
                let mut patch = leveled(level);
                if k == 2 {
                    patch.voice.operators[0].envelope.rates[3] = value(50.0);
                }
                Rig::new(changes, &patch)
            })
            .collect()
    };
    let rigs = rigs_in(&[("param.level_glide_s", Value::Number(0.01))]);
    let context = |k: usize| Context {
        patch: &rigs[k].patch,
        ..rigs[0].context()
    };
    let glide = 0.01 * RATE;
    let end = end_level(rigs[0].configuration.end_db);
    let mut voice = rigs[0].voice();
    voice.key_on(
        Instant::at(0),
        note(60.0, 100.0, 0.0),
        Portamento::Still,
        0,
        &context(0),
    );
    run(&mut voice, &context(0), 1, 1000);
    assert_eq!(voice.stage(0), Some(Stage::Held));
    let level = |voice: &Voice, k: usize, n: u64| {
        voice
            .probe(0, Instant::at(n), &context(k))
            .and_then(|probe| probe.level)
    };
    let held = level(&voice, 0, 1000).expect("a level");
    let offsets = [0, 1].map(|k| rigs[k].offset(0, 60.0, 100.0).expect("an offset"));
    let moved = offsets[1] - offsets[0];
    let share = |from: u64, n: u64| (count(n - from) / glide).min(1.0);
    let check =
        |voice: &mut Voice, k: usize, range: core::ops::RangeInclusive<u64>, expected: &dyn Fn(u64) -> Option<f64>| {
            for n in range {
                if n > 1000 {
                    step(voice, &context(k), n);
                }
                match (level(voice, k, n), expected(n)) {
                    (Some(actual), Some(expected)) => assert!(
                        (actual - expected).abs() <= ramp_bound(4400.0),
                        "sample {n}: {actual} against {expected}"
                    ),
                    (actual, expected) => assert_eq!(actual, expected, "sample {n}"),
                }
            }
        };
    voice.change(Instant::at(1000), &context(0), &context(1));
    check(&mut voice, 1, 1000..=1100, &|n| Some(held + share(1000, n) * moved));
    voice.change(Instant::at(1100), &context(1), &context(2));
    check(&mut voice, 2, 1101..=1240, &|n| Some(held + share(1000, n) * moved));
    let reached = held + share(1000, 1240) * moved;
    voice.change(Instant::at(1240), &context(2), &context(3));
    check(&mut voice, 3, 1240..=1719, &|n| {
        Some(reached + share(1240, n) * (end - reached))
    });
    check(&mut voice, 3, 1720..=2000, &|_| None);
    voice.change(Instant::at(2000), &context(3), &context(4));
    check(&mut voice, 4, 2000..=2600, &|n| {
        Some(end + share(2000, n) * (held - end))
    });
    // With no glide, a change acts at once; a change to silence from below the
    // end level, too.
    let rigs = rigs_in(&[("param.level_glide_s", Value::Number(0.0))]);
    let context = |k: usize| Context {
        patch: &rigs[k].patch,
        ..rigs[0].context()
    };
    let mut voice = rigs[0].voice();
    voice.key_on(
        Instant::at(0),
        note(60.0, 100.0, 0.0),
        Portamento::Still,
        0,
        &context(0),
    );
    run(&mut voice, &context(0), 1, 1000);
    voice.change(Instant::at(1000), &context(0), &context(1));
    let probe = voice.probe(0, Instant::at(1000), &context(1)).expect("a probe");
    assert!((probe.level.expect("a level") - (held + moved)).abs() <= ramp_bound(4400.0));
    voice.change(Instant::at(1000), &context(1), &context(3));
    assert_eq!(
        voice.probe(0, Instant::at(1000), &context(3)).expect("a probe").level,
        None
    );
}

/// The scaled patch with OP1's envelope and the pitch envelope's rates
/// changed, its offset as it was.
fn rescaled() -> Patch {
    let mut patch = scaled();
    patch.voice.pitch_envelope.rates = [value(40.0), value(80.0), value(30.0), value(90.0)];
    let operator = &mut patch.voice.operators[0];
    operator.envelope.rates = [value(30.0), value(70.0), value(20.0), value(40.0)];
    operator.envelope.levels = [value(90.0), value(60.0), value(85.0), value(0.0)];
    patch
}

/// OP1's level and the pitch envelope's against both driven apart, the voice
/// changed at sample 700 from the first rig's patch to the second's, a note's
/// key k′ under each given, bit for bit (MATH §4.7, §5): OP1's offset the same
/// under both, its envelope re-anchored where its parameters or its rates' key
/// change, and the pitch envelope where its own do.
fn re_anchored(rigs: &[Rig; 2], keys: [f64; 2]) {
    let context = |k: usize| Context {
        patch: &rigs[k].patch,
        ..rigs[0].context()
    };
    let offset = rigs[0].offset(0, keys[0], VELOCITY).expect("an offset");
    assert_eq!(rigs[1].offset(0, keys[1], VELOCITY), Some(offset));
    let mut voice = rigs[0].voice();
    let start = Instant::at(0);
    voice.key_on(start, note(52.0, VELOCITY, 0.0), Portamento::Still, 0, &context(0));
    let mut driven = Driven::new(&rigs[0], 0, keys[0], None, offset, start);
    let mut kept = Driven::new(&rigs[0], 0, keys[0], None, offset, start);
    let mut differs = false;
    let (before, after) = (&rigs[0].patch, &rigs[1].patch);
    for n in 1..=3000 {
        let k = usize::from(n > 700);
        let (from, at) = (Instant::at(n - 1), Instant::at(n));
        voice.advance(from, &context(k));
        driven.sample(&rigs[k], keys[k], from, |_, _| offset);
        kept.sample(&rigs[0], keys[0], from, |_, _| offset);
        if n == 700 {
            voice.change(at, &context(0), &context(1));
            if before.pitch_envelope != after.pitch_envelope {
                driven.pitch.change(at, &after.pitch_envelope, RATE);
            }
            let moved = driven.rates_key(keys[1], at);
            let still = moved.to_bits() == driven.rates_key(keys[0], at).to_bits();
            if before.operators[0].envelope != after.operators[0].envelope || !still {
                let settings = &rigs[1].configuration.envelope;
                driven
                    .envelope
                    .change(at, &after.operators[0].envelope, settings, moved);
            }
        }
        let k = usize::from(n >= 700);
        voice.output(at, &context(k));
        let probe = voice.probe(0, at, &context(k)).expect("a probe");
        assert_eq!(probe.level, driven.envelope.level(at, offset), "sample {n}");
        assert_eq!(
            voice.pitch_envelope(at, &context(k)),
            driven.pitch.level(at),
            "sample {n}"
        );
        differs |= probe.level != kept.envelope.level(at, offset);
    }
    assert!(differs, "the change makes no difference");
}

#[test]
fn a_change_re_anchors_the_envelopes_where_their_parameters_or_keys_change() {
    // The scaled patch, its rates' key moved by its pitch envelope, changed
    // within its segments to rescaled rates and levels of OP1 and the pitch
    // envelope.
    let rigs = [Rig::new(&[], &scaled()), Rig::new(&[], &rescaled())];
    let key = rigs[0].transposed(52.0);
    re_anchored(&rigs, [key; 2]);
    // A transpose that moves the rates' key, OP1's level scaling off, so that
    // its offset holds.
    let mut unscaled = scaled();
    unscaled.voice.operators[0].left_depth = value(0.0);
    unscaled.voice.operators[0].right_depth = value(0.0);
    let mut up = unscaled.clone();
    up.voice.transpose = value(39.0);
    let rigs = [Rig::new(&[], &unscaled), Rig::new(&[], &up)];
    let keys = [rigs[0].transposed(52.0), rigs[1].transposed(52.0)];
    assert_eq!(keys[1], keys[0] + 12.0);
    re_anchored(&rigs, keys);
}

#[test]
fn a_change_moves_the_key_and_the_offset_with_it() {
    // A transpose 12 keys up moves k′ and the operators' pitches an octave up at
    // once, the phases going on, and the scaling key with them, whose offset
    // the level ramps to (MATH §2.1, §4.7); OP1's rate scaling at 0, so that
    // its rates do not move with the key.
    // Under scaling.key = key and pitch alike, the pitch envelope and the LFO
    // holding the pitch still, so that kₛ is k′.
    for choice in [0, 2] {
        moves_the_key(&[("scaling.key", Value::Choice(choice))]);
    }
}

/// The transpose test of [`a_change_moves_the_key_and_the_offset_with_it`]
/// in settings changed so.
fn moves_the_key(adjusted: &[(&str, Value)]) {
    let mut flat = scaled();
    flat.voice.operators[0].rate_scaling = value(0.0);
    flat.voice.pitch_envelope.levels = [value(50.0); 4];
    flat.voice.lfo.pitch_depth = value(0.0);
    let mut up = flat.clone();
    up.voice.transpose = value(39.0);
    let rigs = [Rig::new(adjusted, &flat), Rig::new(adjusted, &up)];
    let context = |k: usize| Context {
        patch: &rigs[k].patch,
        ..rigs[0].context()
    };
    let mut voice = rigs[0].voice();
    voice.key_on(
        Instant::at(0),
        note(52.0, VELOCITY, 0.0),
        Portamento::Still,
        0,
        &context(0),
    );
    run(&mut voice, &context(0), 1, 500);
    let at = Instant::at(500);
    let before = voice.probe(0, at, &context(0)).expect("a probe");
    voice.change(at, &context(0), &context(1));
    let after = voice.probe(0, at, &context(1)).expect("a probe");
    assert_eq!(voice.transposed(), rigs[0].transposed(52.0) + 12.0);
    assert_eq!(after.phase, before.phase);
    // Each pitch sums at most five terms below 16 in magnitude, each rounding
    // within u·16, and 12-TET's twelfth within u·16 more: within 6u·16 each.
    assert!((after.pitch - before.pitch - 1.0).abs() <= 12.0 * U * 16.0);
    assert_eq!(after.level, before.level);
    // The ramp of param.level_glide_s, 96 samples, is over by sample 700.
    let n = 700;
    assert!(rigs[0].configuration.level_glide_s * RATE < 200.0);
    run(&mut voice, &context(1), 501, n);
    let moved = rigs[1].offset(0, voice.transposed(), VELOCITY).expect("an offset")
        - rigs[0]
            .offset(0, voice.transposed() - 12.0, VELOCITY)
            .expect("an offset");
    assert!(moved.abs() > 1.0);
    let mut unchanged = rigs[0].voice();
    unchanged.key_on(
        Instant::at(0),
        note(52.0, VELOCITY, 0.0),
        Portamento::Still,
        0,
        &context(0),
    );
    run(&mut unchanged, &context(0), 1, n);
    let level = |voice: &Voice, k: usize| {
        voice
            .probe(0, Instant::at(n), &context(k))
            .and_then(|probe| probe.level)
    };
    let (changed, kept) = (
        level(&voice, 1).expect("a level"),
        level(&unchanged, 0).expect("a level"),
    );
    assert!(
        (changed - kept - moved).abs() <= ramp_bound(4400.0),
        "{changed} {kept} {moved}"
    );
}

#[test]
fn a_change_moves_the_switches_and_the_voice_s_own_lfo() {
    // A switch that the change turns off ramps as one that acts by itself.
    let mut off = held(true);
    off.voice.operators[1].on = false;
    let rigs = [Rig::new(&[], &held(true)), Rig::new(&[], &off)];
    let context = |k: usize| Context {
        patch: &rigs[k].patch,
        ..rigs[0].context()
    };
    let mut changed = rigs[0].voice();
    changed.key_on(
        Instant::at(0),
        note(60.0, 100.0, 0.0),
        Portamento::Still,
        0,
        &context(0),
    );
    run(&mut changed, &context(0), 1, 300);
    let mut switched = changed.clone();
    changed.change(Instant::at(300), &context(0), &context(1));
    switched.switch(Instant::at(300), 1, false, &context(0));
    for n in 301..=500 {
        let changed_out = step(&mut changed, &context(1), n);
        assert_eq!(changed_out, step(&mut switched, &context(0), n), "sample {n}");
    }
    // Under lfo.scope = voice its own LFO takes the new speed and DELAY: the
    // amplitude modulation against an LFO changed so apart.
    let scope = [("lfo.scope", Value::Choice(1))];
    let mut faster = modulated(5.0, Wave::Triangle, true);
    faster.voice.lfo.speed = value(80.0);
    faster.voice.lfo.delay = value(10.0);
    let rigs = [
        Rig::new(&scope, &modulated(5.0, Wave::Triangle, true)),
        Rig::new(&scope, &faster),
    ];
    let mut voice = rigs[0].voice();
    let at = Instant::at(0);
    voice.key_on(at, note(64.0, 100.0, 0.0), Portamento::Still, 0, &rigs[0].context());
    let mut own = Lfo::new(at, rigs[0].patch.lfo.speed, RATE, rigs[0].streams.voices, 0);
    own.key_on(at, false, 5.0, RATE);
    run(&mut voice, &rigs[0].context(), 1, 3000);
    let change = Instant::at(3000);
    let new = Context {
        patch: &rigs[1].patch,
        ..rigs[0].context()
    };
    voice.change(change, &rigs[0].context(), &new);
    own.set_speed(change, rigs[1].patch.lfo.speed, RATE);
    own.set_delay(change, rigs[1].patch.lfo.delay, RATE);
    modulation_follows(&rigs[1], &mut voice, 3001, 9000, |at| {
        (own.fade(at), own.wave(at, Wave::Triangle))
    });
}

#[test]
fn a_ramp_gives_way_to_a_note_and_to_levels_below_the_end_level() {
    // Under param.level_glide_s of 0.01 s: a key-on during a ramp from OL 90 to
    // 70 takes the new law's offset at once; OL 10⁻⁹, λ so far below 1 that
    // its held T lies below the end level, falls silent at once at OL 0, and
    // comes back from it at once.
    let changes = [("param.level_glide_s", Value::Number(0.01))];
    let rigs: Vec<Rig> = [90.0, 70.0, 1e-9, 0.0]
        .iter()
        .map(|&level| Rig::new(&changes, &leveled(level)))
        .collect();
    let context = |k: usize| Context {
        patch: &rigs[k].patch,
        ..rigs[0].context()
    };
    let end = end_level(rigs[0].configuration.end_db);
    let level = |voice: &Voice, k: usize, n: u64| {
        voice
            .probe(0, Instant::at(n), &context(k))
            .and_then(|probe| probe.level)
    };
    let mut voice = rigs[0].voice();
    let tune = note(60.0, 100.0, 0.0);
    voice.key_on(Instant::at(0), tune, Portamento::Still, 0, &context(0));
    run(&mut voice, &context(0), 1, 1000);
    let held = level(&voice, 0, 1000).expect("a level");
    let moved = rigs[1].offset(0, 60.0, 100.0).expect("an offset") - rigs[0].offset(0, 60.0, 100.0).expect("an offset");
    voice.change(Instant::at(1000), &context(0), &context(1));
    run(&mut voice, &context(1), 1001, 1100);
    voice.key_on(Instant::at(1100), tune, Portamento::Still, 1, &context(1));
    for n in [1100, 1101, 1200] {
        if n > 1100 {
            step(&mut voice, &context(1), n);
        }
        let actual = level(&voice, 1, n).expect("a level");
        assert!(
            (actual - (held + moved)).abs() <= ramp_bound(4400.0),
            "sample {n}: {actual}"
        );
    }
    let mut voice = rigs[2].voice();
    voice.key_on(Instant::at(0), tune, Portamento::Still, 0, &context(2));
    run(&mut voice, &context(2), 1, 1000);
    let low = level(&voice, 2, 1000).expect("a level");
    assert!(low < end, "{low} against {end}");
    voice.change(Instant::at(1000), &context(2), &context(3));
    assert_eq!(level(&voice, 3, 1000), None);
    run(&mut voice, &context(3), 1001, 1100);
    voice.change(Instant::at(1100), &context(3), &context(2));
    assert_eq!(level(&voice, 2, 1100), Some(low));
}
