// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The engine's configuration and a part's compiled patch: every setting that
//! the engine takes read into its field, each choice to its own value; the
//! settings whose values M3 does not compute refused by name, in MATH §17's
//! order, and a host rate that is no rate refused; and a compiled patch's
//! every value the law's at the patch's parameters: the DX7's VOICE INIT,
//! patches drawn at random on the scales and beyond them, every algorithm
//! with and without its feedback edge, and the loops of algorithms 4 and 6.

use keptone_engine::compile::CompiledPatch;
use keptone_engine::configuration::{
    AlgorithmScope, Channels, Configuration, FeedbackModel, FreePhaseStart, FunctionSource, LfoScope, Modulation,
    Normalization, RateKey, Refusal, Retrigger, ScalingKey, Steal,
};
use keptone_engine::control::{amplitude_sensitivity, pitch_sensitivity};
use keptone_engine::eg::RateLaw;
use keptone_engine::feedback::kappa;
use keptone_engine::graph::{Component, components};
use keptone_engine::pitch::{FixedDetune, TransposeOctave, fixed_pitch, ratio};
use keptone_engine::portamento;
use keptone_model::function::Function;
use keptone_model::graph::{Graph, Kind};
use keptone_model::patch::Patch;
use keptone_model::settings::{self, Settings, Value};
use keptone_model::voice::{Algorithm, Curve, Mode, Value as Parameter, Wave};

/// Settings at their defaults but for the three that `keptone-tools render`
/// sets to M3's values (DESIGN §16).
fn settings() -> Settings {
    let mut settings = Settings::new();
    for (name, value) in [
        ("engine.oversampling", Value::Integer(1)),
        ("adaa.order", Value::Choice(0)),
        ("feedback.series_beta_max", Value::Number(0.0)),
    ] {
        assert!(set(&mut settings, name, value), "{name}");
    }
    settings
}

/// Sets a setting by its name.
fn set(settings: &mut Settings, name: &str, value: Value) -> bool {
    settings.set(settings::number_of(name).expect("a setting"), value)
}

/// The configuration of settings at 48 kHz.
fn configured(settings: &Settings) -> Configuration {
    Configuration::new(settings, 48_000.0).expect("a configuration")
}

#[test]
fn the_configuration_reads_every_setting_it_takes() {
    let configuration = configured(&settings());
    assert_eq!(configuration.host_rate, 48_000.0);
    assert_eq!(configuration.sample_rate, 48_000.0);
    assert_eq!(configuration.a4_hz, 440.0);
    assert_eq!(configuration.envelope.sample_rate, 48_000.0);
    assert_eq!(configuration.envelope.clock_hz, 49_096.0);
    assert_eq!(configuration.envelope.rate_law, RateLaw::Dx7);
    assert!(configuration.envelope.attack_jump && configuration.envelope.waits);
    assert_eq!(configuration.end_db, -200.0);
    assert_eq!(configuration.level_glide_s, 0.002);
    assert_eq!(configuration.lfo_seed, 0);
    assert_eq!(configuration.am_max_steps, 2048.0);
    assert_eq!(configuration.operators, 6);
    assert_eq!(configuration.feedback.tau_s, 1.5 / 49_096.0);
    assert_eq!(configuration.feedback.beta_clamp, 0.999);
    assert_eq!(configuration.feedback.max_iterations, 16);
    assert_eq!(configuration.feedback.relaxation_onset, 0.9);
    assert_eq!(configuration.feedback.max_substeps, 64);
    assert_eq!(configuration.max_pieces, 256);
    assert_eq!(configuration.voice_count, 16);
    assert_eq!(configuration.idle_s, 0.5);
    assert_eq!(configuration.part_count, 2);
    assert_eq!(configuration.output_gain, 0.5);
    // Numbers and counts other than their defaults.
    let mut changed = settings();
    for (name, value) in [
        ("tuning.a4_hz", Value::Number(432.0)),
        ("eg.clock_hz", Value::Number(50_000.0)),
        ("eg.attack_jump", Value::Switch(false)),
        ("eg.end_db", Value::Number(-120.0)),
        ("param.level_glide_s", Value::Number(0.0)),
        ("lfo.seed", Value::Integer(u64::MAX)),
        ("am.max_steps", Value::Number(1000.0)),
        ("feedback.tau_s", Value::Number(1e-5)),
        ("feedback.beta_clamp", Value::Number(0.5)),
        ("feedback.max_iterations", Value::Integer(3)),
        ("feedback.relaxation_onset", Value::Number(0.25)),
        ("feedback.max_substeps", Value::Integer(9)),
        ("osc.max_pieces", Value::Integer(12)),
        ("voice.count", Value::Integer(5)),
        ("engine.idle_s", Value::Number(2.0)),
        ("part.count", Value::Integer(7)),
        ("output.gain", Value::Number(1.5)),
    ] {
        assert!(set(&mut changed, name, value), "{name}");
    }
    let configuration = configured(&changed);
    assert_eq!(configuration.a4_hz, 432.0);
    assert_eq!(configuration.envelope.clock_hz, 50_000.0);
    assert!(!configuration.envelope.attack_jump);
    assert_eq!(configuration.end_db, -120.0);
    assert_eq!(configuration.level_glide_s, 0.0);
    assert_eq!(configuration.lfo_seed, u64::MAX);
    assert_eq!(configuration.am_max_steps, 1000.0);
    assert_eq!(configuration.feedback.tau_s, 1e-5);
    assert_eq!(configuration.feedback.beta_clamp, 0.5);
    assert_eq!(configuration.feedback.max_iterations, 3);
    assert_eq!(configuration.feedback.relaxation_onset, 0.25);
    assert_eq!(configuration.feedback.max_substeps, 9);
    assert_eq!(configuration.max_pieces, 12);
    assert_eq!(configuration.voice_count, 5);
    assert_eq!(configuration.idle_s, 2.0);
    assert_eq!(configuration.part_count, 7);
    assert_eq!(configuration.output_gain, 1.5);
}

/// Asserts that each choice of a setting, by its index in MATH §17's order of
/// its values, is read into a field as its own value.
fn choices<T: PartialEq + core::fmt::Debug>(name: &str, field: impl Fn(&Configuration) -> T, expected: &[T]) {
    for (index, expected) in expected.iter().enumerate() {
        let mut changed = settings();
        let index = u8::try_from(index).expect("an index");
        assert!(set(&mut changed, name, Value::Choice(index)), "{name} {index}");
        assert_eq!(&field(&configured(&changed)), expected, "{name} {index}");
    }
}

#[test]
fn the_configuration_reads_every_choice() {
    choices(
        "tuning.transpose_octave",
        |c| c.transpose_octave,
        &[TransposeOctave::Keys, TransposeOctave::Period],
    );
    choices(
        "tuning.modulation",
        |c| c.modulation,
        &[Modulation::Pitch, Modulation::Keys],
    );
    choices(
        "osc.fixed_detune",
        |c| c.fixed_detune,
        &[FixedDetune::Msfa, FixedDetune::Symmetric, FixedDetune::None],
    );
    choices(
        "osc.free_phase_start",
        |c| c.free_phase_start,
        &[FreePhaseStart::Continue, FreePhaseStart::Zero, FreePhaseStart::Random],
    );
    choices(
        "eg.rate_law",
        |c| c.envelope.rate_law,
        &[RateLaw::Dx7, RateLaw::Exponential],
    );
    choices("eg.static_segments", |c| c.envelope.waits, &[true, false]);
    choices(
        "scaling.key",
        |c| c.scaling_key,
        &[ScalingKey::Key, ScalingKey::OnsetPitch, ScalingKey::Pitch],
    );
    choices(
        "scaling.rate_key",
        |c| c.rate_key,
        &[RateKey::Frequency, RateKey::Level],
    );
    choices("lfo.scope", |c| c.lfo_scope, &[LfoScope::Part, LfoScope::Voice]);
    choices(
        "graph.algorithm_scope",
        |c| c.algorithm_scope,
        &[AlgorithmScope::Patch, AlgorithmScope::Voice],
    );
    choices(
        "feedback.model",
        |c| c.feedback.model,
        &[
            FeedbackModel::Relaxation,
            FeedbackModel::Dx7Discrete,
            FeedbackModel::Clamp,
        ],
    );
    choices(
        "voice.steal",
        |c| c.steal,
        &[Steal::Dx7, Steal::Oldest, Steal::Quietest, Steal::ReleasedFirst],
    );
    choices(
        "voice.retrigger",
        |c| c.retrigger,
        &[Retrigger::SameVoice, Retrigger::NewVoice],
    );
    choices(
        "part.function_source",
        |c| c.function_source,
        &[FunctionSource::Patch, FunctionSource::Part],
    );
    choices(
        "output.carrier_normalization",
        |c| c.normalization,
        &[Normalization::None, Normalization::Count, Normalization::SqrtCount],
    );
    choices("output.channels", |c| c.channels, &[Channels::Stereo, Channels::Mono]);
    // The defaults.
    let configuration = configured(&settings());
    assert_eq!(configuration.retrigger, Retrigger::NewVoice);
    assert_eq!(configuration.rate_key, RateKey::Frequency);
    assert_eq!(configuration.feedback.model, FeedbackModel::Relaxation);
}

#[test]
fn the_configuration_refuses_what_m3_does_not_compute() {
    // The defaults themselves: `feedback.series_beta_max` is the first in MATH
    // §17's order of the three that M3 does not compute at their defaults.
    assert_eq!(
        Configuration::new(&Settings::new(), 48_000.0),
        Err(Refusal::Setting {
            name: "feedback.series_beta_max",
            value: Value::Number(0.8),
        })
    );
    for (name, value) in [
        ("tuning.source", Value::Choice(1)),
        ("tuning.source", Value::Choice(2)),
        ("graph.operators", Value::Integer(4)),
        ("graph.operators", Value::Integer(7)),
        ("feedback.series_beta_max", Value::Number(0.5)),
        ("adaa.order", Value::Choice(1)),
        ("adaa.order", Value::Choice(2)),
        ("engine.oversampling", Value::Integer(2)),
        ("engine.oversampling", Value::Integer(8)),
    ] {
        let mut refused = settings();
        assert!(set(&mut refused, name, value), "{name}");
        assert_eq!(
            Configuration::new(&refused, 48_000.0),
            Err(Refusal::Setting { name, value }),
            "{name}"
        );
    }
    for rate in [0.0, -48_000.0, f64::NAN, f64::INFINITY, -0.0] {
        assert!(
            matches!(Configuration::new(&settings(), rate), Err(Refusal::HostRate(_))),
            "{rate}"
        );
    }
    assert!(Configuration::new(&settings(), 8000.0).is_ok());
}

#[test]
fn the_voice_init_compiles_as_the_laws_take_it() {
    let configuration = configured(&settings());
    let compiled = CompiledPatch::new(&Patch::init(), &Function::initial(), &configuration);
    assert_eq!(compiled.operators.len(), 6);
    for (k, operator) in compiled.operators.iter().enumerate() {
        assert_eq!(operator.envelope.rates, [99.0; 4]);
        assert_eq!(operator.envelope.levels, [99.0, 99.0, 99.0, 0.0]);
        assert_eq!(operator.envelope.rate_scaling, 0.0);
        assert_eq!(operator.output_level, if k == 0 { 99.0 } else { 0.0 });
        assert_eq!(operator.break_point, 0.0);
        assert_eq!((operator.left.depth, operator.right.depth), (0.0, 0.0));
        assert_eq!(operator.velocity_sensitivity, 0.0);
        assert_eq!(operator.amplitude_sensitivity, 0.0);
        assert_eq!(operator.mode, Mode::Ratio);
        assert_eq!(operator.ratio, 1.0);
        assert_eq!(operator.detune, 7.0);
        assert!(operator.on);
    }
    // Algorithm 1 with FB 0: no feedback edge, OP6 down to OP1 in order.
    assert_eq!(
        compiled.graph,
        Graph::of_algorithm(Algorithm::new(1).expect("1"), false)
    );
    assert_eq!(compiled.feedback.len(), 0);
    assert_eq!(compiled.components, components(&compiled.graph));
    assert_eq!(compiled.pitch_envelope.rates, [99.0; 4]);
    assert_eq!(compiled.pitch_envelope.levels, [50.0; 4]);
    assert_eq!(compiled.lfo.speed, 35.0);
    assert_eq!(compiled.lfo.delay, 0.0);
    assert_eq!((compiled.lfo.pitch_depth, compiled.lfo.amplitude_depth), (0.0, 0.0));
    assert!(compiled.lfo.sync);
    assert_eq!(compiled.lfo.wave, Wave::Triangle);
    assert_eq!(compiled.pitch_sensitivity, 33.0 / 256.0);
    assert!(compiled.key_sync);
    assert_eq!(compiled.transpose, 24.0);
    // Keptone's function data: poly, a bend of 2 without steps, retain with no
    // time, which glides not at all, and the wheel to the pitch at 99.
    let function = compiled.function;
    assert!(!function.mono && !function.follow && !function.glissando);
    assert_eq!((function.bend_range, function.bend_step), (2.0, 0.0));
    assert_eq!(function.portamento_speed, None);
    assert_eq!(function.controllers[0].range, 99.0);
    assert!(function.controllers[0].pitch && !function.controllers[0].amplitude);
    for controller in &function.controllers[1..] {
        assert_eq!(controller.range, 0.0);
        assert!(!controller.pitch && !controller.amplitude && !controller.eg_bias);
    }
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

    /// A value of a scale of 0 to `end`: whole, between the steps, or beyond.
    fn parameter(&mut self, end: f64) -> f64 {
        let unit = f64::from(u32::try_from(self.next() >> 40).expect("24 bits")) / 16_777_216.0;
        match self.next() % 4 {
            0 => (unit * (end + 1.0)).floor(),
            1 => unit * end,
            2 => unit.mul_add(1.4 * end + 4.0, -0.2 * end - 2.0),
            _ => (unit * end).round(),
        }
    }
}

/// A parameter of a scale.
fn value<const MAX: u8>(value: f64) -> Parameter<MAX> {
    Parameter::new(value).expect("a finite value")
}

/// A patch and function data drawn at random: their parameters whole, between
/// the steps and beyond the scales, the switches and choices drawn.
fn drawn(random: &mut Random) -> (Patch, Function) {
    let curves = [
        Curve::NegativeLinear,
        Curve::NegativeExponential,
        Curve::PositiveExponential,
        Curve::PositiveLinear,
    ];
    let mut patch = Patch::init();
    let mut function = Function::initial();
    let voice = &mut patch.voice;
    for operator in &mut voice.operators {
        operator.coarse = value(random.parameter(31.0));
        operator.fine = value(random.parameter(99.0));
        operator.detune = value(random.parameter(14.0));
        operator.amplitude_sensitivity = value(random.parameter(3.0));
        operator.output_level = value(random.parameter(99.0));
        operator.break_point = value(random.parameter(99.0));
        operator.left_depth = value(random.parameter(99.0));
        operator.right_curve = curves[usize::try_from(random.next() % 4).expect("an index")];
        operator.left_curve = curves[usize::try_from(random.next() % 4).expect("an index")];
        operator.right_depth = value(random.parameter(99.0));
        operator.velocity_sensitivity = value(random.parameter(7.0));
        operator.envelope.levels[2] = value(random.parameter(99.0));
        operator.mode = if random.next().is_multiple_of(2) {
            Mode::Ratio
        } else {
            Mode::Fixed
        };
        operator.on = !random.next().is_multiple_of(4);
        operator.envelope.rates[1] = value(random.parameter(99.0));
        operator.rate_scaling = value(random.parameter(7.0));
    }
    let number = u8::try_from(1 + random.next() % 32).expect("an algorithm");
    voice.algorithm = Algorithm::new(number).expect("an algorithm");
    voice.feedback = value(if random.next().is_multiple_of(3) {
        0.0
    } else {
        random.parameter(7.0)
    });
    voice.pitch_sensitivity = value(random.parameter(7.0));
    voice.transpose = value(random.parameter(48.0));
    voice.lfo.speed = value(random.parameter(99.0));
    function.portamento_time = value(random.parameter(99.0));
    function.pitch_bend_step = value(random.parameter(12.0));
    function.pitch_bend_range = value(random.parameter(12.0));
    function.glissando = random.next().is_multiple_of(2);
    for controller in [
        &mut function.modulation_wheel,
        &mut function.foot_controller,
        &mut function.breath_controller,
        &mut function.aftertouch,
    ] {
        controller.range = value(random.parameter(99.0));
        let step = u8::try_from(random.next() % 8).expect("a step");
        controller.assign = keptone_model::function::Assign::from_step(step).expect("an assignment");
    }
    voice.oscillator_sync = random.next().is_multiple_of(2);
    (patch, function)
}

/// Asserts that a compiled patch's every value is the law's at the patch's and
/// the function data's parameters.
fn checked(compiled: &CompiledPatch, patch: &Patch, function: &Function) {
    let voice = &patch.voice;
    for (compiled, operator) in compiled.operators.iter().zip(&voice.operators) {
        let (coarse, fine, detune) = (operator.coarse.get(), operator.fine.get(), operator.detune.get());
        assert_eq!(compiled.ratio.to_bits(), ratio(coarse, fine).to_bits());
        let fixed = fixed_pitch(coarse, fine, detune, FixedDetune::Msfa);
        assert_eq!(compiled.fixed_pitch.to_bits(), fixed.to_bits());
        assert_eq!(compiled.detune, detune);
        let alpha = amplitude_sensitivity(operator.amplitude_sensitivity.get());
        assert_eq!(compiled.amplitude_sensitivity.to_bits(), alpha.to_bits());
        assert_eq!(compiled.output_level, operator.output_level.get());
        assert_eq!(compiled.break_point, operator.break_point.get());
        assert_eq!(compiled.left.depth, operator.left_depth.get());
        assert_eq!(compiled.right.curve, operator.right_curve);
        assert_eq!(compiled.left.curve, operator.left_curve);
        assert_eq!(compiled.right.depth, operator.right_depth.get());
        assert_eq!(compiled.velocity_sensitivity, operator.velocity_sensitivity.get());
        assert_eq!(compiled.envelope.levels[2], operator.envelope.levels[2].get());
        assert_eq!(compiled.mode, operator.mode);
        assert_eq!(compiled.on, operator.on);
        assert_eq!(compiled.envelope.rates[1], operator.envelope.rates[1].get());
        assert_eq!(compiled.envelope.rate_scaling, operator.rate_scaling.get());
    }
    let fb = voice.feedback.get();
    assert_eq!(compiled.graph, Graph::of_algorithm(voice.algorithm, fb != 0.0));
    assert_eq!(compiled.components, components(&compiled.graph));
    let feedback_edges = compiled
        .graph
        .edges()
        .iter()
        .filter(|edge| edge.kind == Kind::Feedback)
        .count();
    assert_eq!(compiled.feedback.len(), feedback_edges);
    assert_eq!(feedback_edges, usize::from(fb != 0.0));
    for edge in &compiled.feedback {
        assert_eq!(edge.kappa.to_bits(), kappa(fb).to_bits());
    }
    let sigma = pitch_sensitivity(voice.pitch_sensitivity.get());
    assert_eq!(compiled.pitch_sensitivity.to_bits(), sigma.to_bits());
    assert_eq!(compiled.transpose, voice.transpose.get());
    assert_eq!(compiled.lfo.speed, voice.lfo.speed.get());
    let speed = portamento::speed(function.portamento_time.get());
    assert_eq!(
        compiled.function.portamento_speed.map(f64::to_bits),
        speed.map(f64::to_bits)
    );
    assert_eq!(compiled.function.bend_step, function.pitch_bend_step.get());
    assert_eq!(compiled.function.bend_range, function.pitch_bend_range.get());
    assert_eq!(compiled.function.glissando, function.glissando);
    assert_eq!(compiled.key_sync, voice.oscillator_sync);
    let given = [
        function.modulation_wheel,
        function.foot_controller,
        function.breath_controller,
        function.aftertouch,
    ];
    for (compiled, given) in compiled.function.controllers.iter().zip(given) {
        assert_eq!(compiled.range, given.range.get());
        assert_eq!(
            (compiled.pitch, compiled.amplitude, compiled.eg_bias),
            (given.assign.pitch, given.assign.amplitude, given.assign.eg_bias)
        );
    }
}

#[test]
fn patches_compile_as_the_laws_take_their_parameters() {
    let configuration = configured(&settings());
    let mut random = Random(0x0063_6f6d_7069_6c65);
    for _ in 0..500 {
        let (patch, function) = drawn(&mut random);
        checked(
            &CompiledPatch::new(&patch, &function, &configuration),
            &patch,
            &function,
        );
    }
}

#[test]
fn the_feedback_edges_and_the_loops_compile() {
    let configuration = configured(&settings());
    let mut patch = Patch::init();
    for number in 1..=32 {
        patch.voice.algorithm = Algorithm::new(number).expect("an algorithm");
        for fb in [0.0, 0.5, 7.0, -1.0] {
            patch.voice.feedback = value(fb);
            let compiled = CompiledPatch::new(&patch, &Function::initial(), &configuration);
            assert_eq!(
                compiled.feedback.len(),
                usize::from(fb != 0.0),
                "algorithm {number}, FB {fb}"
            );
            let loops = compiled
                .components
                .iter()
                .filter(|component| matches!(component, Component::Loop(_)))
                .count();
            // The edges from OP4 and from OP5 to OP6 close the loops of
            // algorithms 4 and 6; every other has at most a self-loop.
            let expected = usize::from(fb != 0.0 && (number == 4 || number == 6));
            assert_eq!(loops, expected, "algorithm {number}, FB {fb}");
            if let Some(edge) = compiled.feedback.first() {
                let (from, to) = match number {
                    4 => (3, 5),
                    6 => (4, 5),
                    _ => (edge.from, edge.from),
                };
                assert_eq!((edge.from, edge.to), (from, to), "algorithm {number}");
                assert_eq!(edge.kappa, kappa(fb));
            }
        }
    }
    // A negative FB inverts the feedback (MATH §9.1).
    patch.voice.feedback = value(-1.0);
    let compiled = CompiledPatch::new(&patch, &Function::initial(), &configuration);
    assert_eq!(compiled.feedback[0].kappa, -1.0 / 128.0);
}
