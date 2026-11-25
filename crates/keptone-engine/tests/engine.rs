// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The engine (docs/MATH.md §0.3, §13, §14): a note through it against its
//! voice driven apart, bit for bit; the slots that notes take under each
//! `voice.steal`, `voice.retrigger` and note limit, the sustain pedal, and a
//! voice that another part takes, which fades out (§13.2); the notes that
//! parts take by their channels, keys, key filters and note shifts (§13.5);
//! the controllers, the bend, the per-note pitch, the master tune and the
//! LFO's restart as the voices take them (§2.4, §6.5, §7.1); the output, the
//! parts added in their order under each `output.channels` (§14); events
//! beyond their block; the refusals; and renders of random input that are the
//! same, bit for bit, however their samples are split into blocks (§0.3).

use core::num::NonZeroU32;

use keptone_engine::compile::CompiledPart;
use keptone_engine::configuration::Refusal;
use keptone_engine::control::{self, Source};
use keptone_engine::eg::envelope::Stage;
use keptone_engine::engine::{Controller, Engine, Event, Key, Kind};
use keptone_engine::lfo::Lfo;
use keptone_engine::pieces::Portamento;
use keptone_engine::pitch::TwelveTet;
use keptone_engine::random::{self, Streams};
use keptone_engine::time::Instant;
use keptone_engine::voice::{Context, Note, Voice};
use keptone_model::dx7ii::Index;
use keptone_model::part::{KeyFilter, NoteShift, Part};
use keptone_model::settings::{self, Settings, Value};
use keptone_model::voice::{Algorithm, Value as Parameter, Wave};

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

/// An engine prepared in settings changed so, with parts given.
fn prepared(changes: &[(&str, Value)], parts: &[Part]) -> Engine {
    Engine::new(&settings(changes), RATE, parts).expect("an engine")
}

/// A parameter of a scale.
fn value<const MAX: u8>(value: f64) -> Parameter<MAX> {
    Parameter::new(value).expect("a finite value")
}

/// A key.
fn key(number: u8) -> Key {
    Key::new(number).expect("a key")
}

/// A note's start on a channel.
fn on(channel: u8, number: u8, velocity: f64) -> Kind {
    Kind::NoteOn {
        channel,
        key: key(number),
        velocity,
        id: None,
    }
}

/// A note's release on a channel.
fn off(channel: u8, number: u8) -> Kind {
    Kind::NoteOff {
        channel,
        key: key(number),
        id: None,
    }
}

/// A count of samples below 2³² as a usize.
fn index(samples: u64) -> usize {
    usize::try_from(samples).expect("a short test")
}

/// Renders so many samples from the engine's next, the events at absolute
/// samples in their order, in blocks whose lengths `block` gives, and gives
/// both channels, each starting at `initial`.
fn render_with(
    engine: &mut Engine,
    events: &[(u64, Kind)],
    samples: u64,
    initial: f64,
    mut block: impl FnMut() -> u64,
) -> [Vec<f64>; 2] {
    let start = engine.sample();
    let mut channels = [vec![initial; index(samples)], vec![initial; index(samples)]];
    let mut done = 0;
    while done < samples {
        let length = block().clamp(1, samples - done);
        let first = start + done;
        let within: Vec<Event> = events
            .iter()
            .filter(|(at, _)| (first..first + length).contains(at))
            .map(|&(at, kind)| Event {
                offset: u32::try_from(at - first).expect("a short block"),
                kind,
            })
            .collect();
        let range = index(done)..index(done + length);
        let [left, right] = &mut channels;
        engine.render(&within, &mut [&mut left[range.clone()], &mut right[range]]);
        done += length;
    }
    channels
}

/// Renders so many samples in one block.
fn render(engine: &mut Engine, events: &[(u64, Kind)], samples: u64) -> [Vec<f64>; 2] {
    render_with(engine, events, samples, 0.0, || samples)
}

/// The key of the note that the voice in a part's slot plays or played last,
/// none before its first.
fn keyed(engine: &Engine, part: usize, slot: usize) -> Option<f64> {
    engine.voice(part, slot).and_then(Voice::note).map(|note| note.key)
}

/// Whether the voice in a part's slot sounds: it has played a note that has
/// not ended.
fn sounds(engine: &Engine, part: usize, slot: usize) -> bool {
    engine
        .voice(part, slot)
        .is_some_and(|voice| voice.note().is_some() && voice.ending().is_none())
}

#[test]
fn a_note_plays_as_its_voice_driven_apart() {
    // VOICE INIT on the first part, the second off, every setting at its
    // default: the engine's output against `output.gain` times a voice driven
    // apart with the part's LFO restarted at its key-on, its controllers at
    // rest and its bend at the centre.
    let mut engine = prepared(&[], &[]);
    let (on_at, off_at, samples) = (10, 2000, 30_000);
    let [left, right] = render(&mut engine, &[(on_at, on(0, 60, 100.0)), (off_at, off(0, 60))], samples);
    let configuration = *engine.configuration();
    let part = CompiledPart::new(&Part::initial(true), &configuration);
    let streams = Streams::of(configuration.lfo_seed, 0);
    let mut lfo = Lfo::new(Instant::at(0), part.patch.lfo.speed, RATE, streams.part, 0);
    let tuning = TwelveTet::new(configuration.a4_hz);
    let wheel = Source {
        position: 0.0,
        range: 99.0,
    };
    let mut voice = Voice::new(6, configuration.max_pieces, Instant::at(0)).expect("memory");
    let mut ended = false;
    for n in 0..samples {
        let at = Instant::at(n);
        if n == on_at {
            lfo.key_on(at, part.patch.lfo.sync, part.patch.lfo.delay, RATE);
        }
        let context = Context {
            patch: &part.patch,
            configuration: &configuration,
            tuning: &tuning,
            lfo: &lfo,
            note_shift: 0.0,
            pitch_controllers: control::controllers(&[wheel]),
            amplitude_controllers: control::controllers(&[]),
            bias: control::bias(&[]),
            bend: 0.0,
            master_tune: 0.0,
            streams,
        };
        if n > 0 {
            voice.advance(Instant::at(n - 1), &context);
        }
        if n == on_at {
            let note = Note {
                key: 60.0,
                velocity: 100.0,
                per_note: 0.0,
            };
            voice.key_on(at, note, Portamento::Still, 0, &context);
        }
        if n == off_at {
            voice.key_off(at, &context);
        }
        let expected = configuration.output_gain * voice.output(at, &context);
        assert_eq!(left[index(n)].to_bits(), expected.to_bits(), "sample {n}");
        assert_eq!(right[index(n)].to_bits(), expected.to_bits(), "sample {n}");
        if !ended && voice.ended(at, &context) {
            voice.end(at);
            ended = true;
        }
    }
    assert!(ended && !sounds(&engine, 0, 0) && engine.sounding() == 0);
    assert!(left.iter().any(|value| value.abs() > 0.5));
}

#[test]
fn notes_take_the_slots_that_the_dx7_gives() {
    // Four voices: free slots after the one given last while the pool has one
    // to spare; then the first voice whose key is up after the one given
    // last, sounding or not; and none where every key is held.
    let mut engine = prepared(&[("voice.count", Value::Integer(4))], &[]);
    let events = [
        (0, on(0, 60, 100.0)),
        (10, on(0, 62, 100.0)),
        (20, on(0, 64, 100.0)),
        (30, off(0, 62)),
        (40, on(0, 65, 100.0)),
        (50, on(0, 67, 100.0)),
        (60, on(0, 69, 100.0)),
    ];
    render(&mut engine, &events, 41);
    let slots = |engine: &Engine| (0..4).map(|slot| keyed(engine, 0, slot)).collect::<Vec<_>>();
    assert_eq!(slots(&engine), [Some(60.0), Some(62.0), Some(64.0), Some(65.0)]);
    assert_eq!(engine.sounding(), 4);
    render(&mut engine, &events, 20);
    // 67 took slot 1, whose key was up; 69 found every key held.
    assert_eq!(slots(&engine), [Some(60.0), Some(67.0), Some(64.0), Some(65.0)]);
    assert_eq!(engine.sounding(), 4);
    // Released and ended, the voices free their slots, and a note takes the
    // first free one after the one given last.
    let releases: Vec<(u64, Kind)> = [60, 67, 64, 65]
        .into_iter()
        .map(|number| (engine.sample(), off(0, number)))
        .collect();
    render(&mut engine, &releases, 20_000);
    assert_eq!(engine.sounding(), 0);
    let next = engine.sample();
    render(&mut engine, &[(next, on(0, 71, 100.0))], 1);
    assert_eq!(keyed(&engine, 0, 2), Some(71.0));
    assert_eq!(engine.sounding(), 1);
}

/// A part of algorithm 5, whose carriers OP1, OP3 and OP5 sound at output
/// levels 99, 70 and 40, OP1 alone taking the velocity, at its full
/// sensitivity, and every operator falling slowly after its key-off: the
/// loudest carrier of a soft note is OP3, and the quietest carrier of every
/// note OP5.
fn sensitive() -> Part {
    let mut part = Part::initial(true);
    let voice = &mut part.patch.voice;
    voice.algorithm = Algorithm::new(5).expect("an algorithm");
    for operator in &mut voice.operators {
        operator.envelope.rates[3] = value(20.0);
    }
    voice.operators[0].velocity_sensitivity = value(7.0);
    voice.operators[2].output_level = value(70.0);
    voice.operators[4].output_level = value(40.0);
    part
}

#[test]
fn the_stealing_policies_pick_their_voices() {
    // Four voices: A held, B released after D, C held and quietest, D released
    // first; the slot given last is D's. A fifth note takes B's slot under dx7,
    // the first key up after D's; A's under oldest; C's under quietest; and
    // D's under released-first.
    let events = [
        (0, on(0, 60, 127.0)),
        (100, on(0, 62, 127.0)),
        (200, on(0, 64, 20.0)),
        (300, on(0, 65, 127.0)),
        (400, off(0, 65)),
        (500, off(0, 62)),
        (600, on(0, 67, 127.0)),
    ];
    for (choice, slot) in [(0, 1), (1, 0), (2, 2), (3, 3)] {
        let mut engine = prepared(
            &[
                ("voice.count", Value::Integer(4)),
                ("voice.steal", Value::Choice(choice)),
            ],
            &[sensitive()],
        );
        render(&mut engine, &events, 601);
        assert_eq!(keyed(&engine, 0, slot), Some(67.0), "policy {choice}");
        assert_eq!(engine.sounding(), 4);
    }
    // Under released-first, where every key is held, the oldest.
    let held = [(0, on(0, 60, 127.0)), (100, on(0, 62, 127.0)), (200, on(0, 64, 127.0))];
    let mut engine = prepared(
        &[("voice.count", Value::Integer(3)), ("voice.steal", Value::Choice(3))],
        &[],
    );
    render(&mut engine, &[held[0], held[1], held[2], (300, on(0, 67, 100.0))], 301);
    assert_eq!(keyed(&engine, 0, 0), Some(67.0));
    // Under released-first, a key pressed again releases its voice once: A
    // released first, by its key pressed again, then B, then A's new voice;
    // the next note takes A's first voice.
    let mut engine = prepared(
        &[("voice.count", Value::Integer(3)), ("voice.steal", Value::Choice(3))],
        &[sensitive()],
    );
    let events = [
        (0, on(0, 60, 127.0)),
        (100, on(0, 62, 127.0)),
        (200, on(0, 60, 127.0)),
        (300, off(0, 62)),
        (400, off(0, 60)),
        (500, on(0, 67, 127.0)),
    ];
    render(&mut engine, &events, 501);
    assert_eq!(keyed(&engine, 0, 0), Some(67.0));
    assert_eq!(keyed(&engine, 0, 1), Some(62.0));
    // A key pressed again whose voice is already released leaves its release
    // where it was: A released first, B after it, A pressed again; the next
    // note takes A's first voice.
    let mut engine = prepared(
        &[("voice.count", Value::Integer(3)), ("voice.steal", Value::Choice(3))],
        &[sensitive()],
    );
    let events = [
        (0, on(0, 60, 127.0)),
        (50, off(0, 60)),
        (100, on(0, 62, 127.0)),
        (150, off(0, 62)),
        (200, on(0, 60, 127.0)),
        (300, on(0, 67, 127.0)),
    ];
    render(&mut engine, &events, 301);
    assert_eq!(keyed(&engine, 0, 0), Some(67.0));
    assert_eq!(keyed(&engine, 0, 1), Some(62.0));
}

#[test]
fn oldest_takes_the_note_that_started_first_wherever_it_lies() {
    // Under oldest, with B's slot freed and taken again by D, the slot given
    // last: the next note takes A's, the oldest, though C's comes first after
    // the one given last.
    let mut engine = prepared(
        &[("voice.count", Value::Integer(3)), ("voice.steal", Value::Choice(1))],
        &[],
    );
    let events = [
        (0, on(0, 60, 100.0)),
        (10, on(0, 62, 100.0)),
        (20, on(0, 64, 100.0)),
        (30, off(0, 62)),
        (20_000, on(0, 65, 100.0)),
        (20_010, on(0, 67, 100.0)),
    ];
    render(&mut engine, &events, 20_001);
    assert_eq!(keyed(&engine, 0, 1), Some(65.0));
    render(&mut engine, &events, 10);
    assert_eq!(keyed(&engine, 0, 0), Some(67.0));
    assert_eq!(keyed(&engine, 0, 2), Some(64.0));
}

#[test]
fn notes_are_known_by_their_identifiers_where_the_host_gives_them() {
    // Two notes of one key, with identifiers 7 and 8: under same-voice the
    // second restarts the first's voice, whatever its identifier. Under
    // new-voice the second takes a voice of its own; the per-note pitch of 8
    // reaches its voice alone, the note-off of 7 releases its own voice alone,
    // and a note-off without an identifier releases every note of the key.
    let note = |id, offset: u64| {
        (
            offset,
            Kind::NoteOn {
                channel: 0,
                key: key(60),
                velocity: 100.0,
                id: Some(id),
            },
        )
    };
    let mut engine = prepared(&[("voice.retrigger", Value::Choice(0))], &[]);
    render(&mut engine, &[note(7, 0), note(8, 10)], 11);
    assert!(sounds(&engine, 0, 0) && !sounds(&engine, 0, 1));
    let mut engine = prepared(&[], &[]);
    let pitch = Kind::PerNote {
        channel: 0,
        key: key(60),
        id: Some(8),
        keys: 1.5,
    };
    let release = |id| Kind::NoteOff {
        channel: 0,
        key: key(60),
        id,
    };
    let held = |engine: &Engine, slot| engine.voice(0, slot).is_some_and(Voice::held);
    render(&mut engine, &[note(7, 0), note(8, 10), (20, pitch)], 21);
    let per_note = |engine: &Engine, slot| engine.voice(0, slot).and_then(Voice::note).map(|note| note.per_note);
    assert_eq!((per_note(&engine, 0), per_note(&engine, 1)), (Some(0.0), Some(1.5)));
    assert!(!held(&engine, 0) && held(&engine, 1));
    render(&mut engine, &[(21, release(Some(7)))], 1);
    assert!(held(&engine, 1));
    render(&mut engine, &[(22, release(None))], 1);
    assert!(!held(&engine, 1));
    // A note-off of an identifier releases the note of that identifier alone.
    let mut engine = prepared(&[], &[]);
    let other = |id, offset: u64, number| {
        (
            offset,
            Kind::NoteOn {
                channel: 0,
                key: key(number),
                velocity: 100.0,
                id: Some(id),
            },
        )
    };
    let named = Kind::NoteOff {
        channel: 0,
        key: key(64),
        id: Some(3),
    };
    render(&mut engine, &[other(3, 0, 62), other(4, 1, 64), (2, named)], 3);
    assert!(!held(&engine, 0) && held(&engine, 1));
}

#[test]
fn a_key_pressed_again_restarts_its_voice_or_takes_another() {
    let events = [(0, on(0, 60, 100.0)), (100, on(0, 60, 80.0)), (200, off(0, 60))];
    // same-voice: the voice restarted in its slot.
    let mut engine = prepared(&[("voice.retrigger", Value::Choice(0))], &[]);
    render(&mut engine, &events, 150);
    let voice = engine.voice(0, 0).expect("a voice");
    assert_eq!(voice.note().map(|note| note.velocity), Some(80.0));
    assert!(voice.held() && !sounds(&engine, 0, 1));
    assert_eq!(engine.sounding(), 1);
    // new-voice: the voice released and a new one taken; the note-off then
    // releases the new one.
    let mut engine = prepared(&[], &[]);
    render(&mut engine, &events, 150);
    let (first, second) = (
        engine.voice(0, 0).expect("a voice"),
        engine.voice(0, 1).expect("a voice"),
    );
    assert!(!first.held() && first.stage(0) == Some(Stage::Segment(3)));
    assert!(second.held() && second.note().map(|note| note.velocity) == Some(80.0));
    assert_eq!(engine.sounding(), 2);
    render(&mut engine, &events, 100);
    assert!(!engine.voice(0, 1).expect("a voice").held());
}

#[test]
fn a_note_limit_caps_a_part_s_voices() {
    // A limit of 2 among 16 voices: the third note takes the part's own voice
    // that dx7 picks, the first key up after the one given last.
    let mut part = sensitive();
    part.note_limit = NonZeroU32::new(2);
    let mut engine = prepared(&[], &[part]);
    let events = [
        (0, on(0, 60, 100.0)),
        (10, on(0, 62, 100.0)),
        (20, off(0, 60)),
        (30, on(0, 64, 100.0)),
        (40, on(0, 65, 100.0)),
    ];
    render(&mut engine, &events, 41);
    assert_eq!(keyed(&engine, 0, 0), Some(64.0));
    assert_eq!(keyed(&engine, 0, 1), Some(62.0));
    assert!(!sounds(&engine, 0, 2));
    assert_eq!(engine.sounding(), 2);
}

#[test]
fn the_sustain_pedal_holds_the_notes_that_it_catches() {
    let sustain = |on| Kind::Sustain { channel: 0, on };
    let mut part = sensitive();
    part.omni = false;
    let mut engine = prepared(&[("voice.count", Value::Integer(2))], &[part]);
    let elsewhere = Kind::Sustain { channel: 1, on: false };
    render(
        &mut engine,
        &[
            (0, sustain(true)),
            (10, on(0, 60, 100.0)),
            (100, off(0, 60)),
            (200, elsewhere),
        ],
        400,
    );
    let voice = engine.voice(0, 0).expect("a voice");
    assert!(voice.held() && voice.stage(0) != Some(Stage::Segment(3)));
    render(&mut engine, &[(400, sustain(false))], 1);
    let voice = engine.voice(0, 0).expect("a voice");
    assert!(!voice.held() && voice.stage(0) == Some(Stage::Segment(3)));
    // A voice that the pedal alone holds counts as one whose key is up.
    let mut engine = prepared(&[("voice.count", Value::Integer(1))], &[]);
    let events = [
        (0, sustain(true)),
        (10, on(0, 60, 100.0)),
        (100, off(0, 60)),
        (200, on(0, 62, 100.0)),
    ];
    render(&mut engine, &events, 201);
    assert_eq!(keyed(&engine, 0, 0), Some(62.0));
    assert!(engine.voice(0, 0).expect("a voice").held());
    // The note that took it is its own: the pedal's release leaves it held.
    render(&mut engine, &[(201, sustain(false))], 1);
    assert!(engine.voice(0, 0).expect("a voice").held());
}

/// Two parts, the first on channel 0 and the second on channel 1, the first's
/// operators falling slowly after their key-off.
fn two_parts() -> [Part; 2] {
    let mut first = sensitive();
    first.omni = false;
    let mut second = Part::initial(true);
    second.omni = false;
    second.channel = Index::new(1).expect("a channel");
    [first, second]
}

#[test]
fn a_voice_that_another_part_takes_fades_out() {
    // One voice: the first part's note sounds released when the second
    // part's note takes its voice, which fades out over 96 samples while the
    // second's starts in a slot of its own; `output.gain` at 1, the output is
    // the first part's voice alone times the fade plus the second's alone.
    let changes = [("voice.count", Value::Integer(1)), ("output.gain", Value::Number(1.0))];
    let first = [(0, on(0, 60, 100.0)), (50, off(0, 60))];
    let second = |channel| (100, on(channel, 64, 100.0));
    let samples = 400;
    let mut both = prepared(&changes, &two_parts());
    let [mixed, _] = render(&mut both, &[first[0], first[1], second(1)], 101);
    assert!(both.fading(0, 0) && sounds(&both, 1, 0) && both.sounding() == 1);
    let [rest, _] = render(&mut both, &[], samples - 101);
    let mut alone = prepared(&changes, &two_parts());
    let [first_alone, _] = render(&mut alone, &[first[0], first[1], second(2)], samples);
    let mut alone = prepared(&changes, &two_parts());
    let [second_alone, _] = render(&mut alone, &[second(1)], samples);
    let glide = 0.002 * RATE;
    let mixed: Vec<f64> = mixed.into_iter().chain(rest).collect();
    for n in 0..index(samples) {
        let fade = if n < 100 {
            1.0
        } else {
            (1.0 - f64::from(u32::try_from(n - 100).expect("a short test")) / glide).max(0.0)
        };
        let expected = if n < 196 {
            first_alone[n] * fade + second_alone[n]
        } else {
            0.0 + second_alone[n]
        };
        assert_eq!(mixed[n].to_bits(), expected.to_bits(), "sample {n}");
    }
    assert!(!both.fading(0, 0) && !sounds(&both, 0, 0) && both.sounding() == 1);
    // Under oldest, of the first part's two voices the older fades; with no
    // glide it ends at once, its output 0 from the note's instant.
    let changes = [
        ("voice.count", Value::Integer(2)),
        ("voice.steal", Value::Choice(1)),
        ("param.level_glide_s", Value::Number(0.0)),
    ];
    let mut both = prepared(&changes, &two_parts());
    render(
        &mut both,
        &[(0, on(0, 60, 100.0)), (10, on(0, 62, 100.0)), (20, on(1, 64, 100.0))],
        21,
    );
    assert!(!sounds(&both, 0, 0) && sounds(&both, 0, 1) && sounds(&both, 1, 0));
    assert!(!both.fading(0, 0) && both.sounding() == 2);
    // A fading voice is no longer its note's: the note's release, which comes
    // during the fade, leaves it as it was, the output the first part's voice
    // rendered alone and never released, times the fade, plus the second's.
    let changes = [
        ("voice.count", Value::Integer(1)),
        ("voice.steal", Value::Choice(1)),
        ("output.gain", Value::Number(1.0)),
    ];
    let mut both = prepared(&changes, &two_parts());
    let [mixed, _] = render(
        &mut both,
        &[(0, on(0, 60, 100.0)), (100, on(1, 64, 100.0)), (150, off(0, 60))],
        samples,
    );
    let [first_alone, _] = render(&mut prepared(&changes, &two_parts()), &[(0, on(0, 60, 100.0))], samples);
    let [second_alone, _] = render(&mut prepared(&changes, &two_parts()), &[second(1)], samples);
    for n in 100..196 {
        let fade = (1.0 - f64::from(u32::try_from(n - 100).expect("a short test")) / glide).max(0.0);
        let expected = first_alone[n] * fade + second_alone[n];
        assert_eq!(mixed[n].to_bits(), expected.to_bits(), "sample {n}");
    }
}

#[test]
fn parts_take_the_notes_of_their_channels_and_keys() {
    // The first part on channel 0 below key 60; the second on channel 1 from
    // key 60 up, its odd keys alone, shifted by an octave; a third, omni, off.
    let [mut first, mut second] = two_parts();
    first.highest_key = Index::new(59).expect("a key");
    second.lowest_key = Index::new(60).expect("a key");
    second.key_filter = KeyFilter::Odd;
    second.note_shift = NoteShift::new(12.0).expect("a shift");
    let third = Part::initial(false);
    let mut engine = prepared(&[("part.count", Value::Integer(3))], &[first, second, third]);
    let events = [
        (0, on(0, 50, 100.0)),
        (1, on(0, 70, 100.0)),
        (2, on(1, 61, 100.0)),
        (3, on(1, 62, 100.0)),
        (4, on(1, 50, 100.0)),
        (5, on(5, 61, 100.0)),
    ];
    render(&mut engine, &events, 6);
    assert_eq!(keyed(&engine, 0, 0), Some(50.0));
    assert!(!sounds(&engine, 0, 1));
    assert_eq!(keyed(&engine, 1, 0), Some(61.0));
    assert_eq!(engine.voice(1, 0).map(Voice::transposed), Some(73.0));
    assert!(!sounds(&engine, 1, 1) && !sounds(&engine, 2, 0));
    assert_eq!(engine.sounding(), 2);
    // An omni part takes every channel; a part of MIDI 2.0's group 1 and its
    // channel 2 receives the engine's channel 18; and each part has the
    // random streams of its index (MATH §6.3).
    let mut engine = prepared(&[], &[]);
    render(&mut engine, &[(0, on(9, 61, 100.0)), (1, on(200, 62, 100.0))], 2);
    assert_eq!(engine.sounding(), 2);
    let mut grouped = Part::initial(true);
    grouped.omni = false;
    grouped.group = Index::new(1).expect("a group");
    grouped.channel = Index::new(2).expect("a channel");
    let mut engine = prepared(&[], &[Part::initial(false), grouped]);
    render(&mut engine, &[(0, on(2, 60, 100.0)), (1, on(18, 62, 100.0))], 2);
    assert_eq!((keyed(&engine, 1, 0), keyed(&engine, 1, 1)), (Some(62.0), None));
    let seed = engine.configuration().lfo_seed;
    for part in 0..2 {
        let streams = engine.context(part).expect("a part").streams;
        assert_eq!(streams, Streams::of(seed, u64::try_from(part).expect("a part")));
    }
}

#[test]
fn each_note_of_a_part_takes_its_own_random_phases() {
    // Without OSC KEY SYNC, under osc.free_phase_start = random, the m-th note
    // of a part takes operator k's phase at index 8m + k of the part's stream
    // of phases (MATH §6.3), at its key-on.
    let mut part = Part::initial(true);
    part.patch.voice.oscillator_sync = false;
    let mut engine = prepared(&[("osc.free_phase_start", Value::Choice(2))], &[part]);
    for nth in 0..3_u64 {
        let number = u8::try_from(60 + 2 * nth).expect("a key");
        render(&mut engine, &[(nth, on(0, number, 100.0))], 1);
        let context = engine.context(0).expect("a part");
        let voice = engine.voice(0, index(nth)).expect("a voice");
        for operator in 0..6_u64 {
            let probe = voice
                .probe(index(operator), Instant::at(nth), &context)
                .expect("a probe");
            let drawn = random::value(context.streams.phases, Streams::phase_index(nth, operator + 1));
            assert_eq!(
                probe.phase.to_bits(),
                random::phase(drawn).to_bits(),
                "note {nth}, operator {operator}"
            );
        }
    }
}

#[test]
fn controllers_the_bend_and_the_tune_reach_the_voices() {
    // The wheel to the pitch and the amplitude, the foot controller to the EG
    // bias, the breath controller to the amplitude and aftertouch to the
    // pitch; a bend of 7 semitones; each as the laws of MATH §2.4 and §7.1
    // compose them, on the channel the part receives alone.
    let mut part = Part::initial(true);
    part.omni = false;
    let function = &mut part.function;
    function.pitch_bend_range = value(7.0);
    function.modulation_wheel.range = value(80.0);
    function.modulation_wheel.assign.amplitude = true;
    function.foot_controller.range = value(60.0);
    function.foot_controller.assign.eg_bias = true;
    function.breath_controller.range = value(50.0);
    function.breath_controller.assign.amplitude = true;
    function.aftertouch.range = value(40.0);
    function.aftertouch.assign.pitch = true;
    let mut engine = prepared(&[], &[part]);
    let controller = |channel, controller, position| Kind::Controller {
        channel,
        controller,
        position,
    };
    let events = [
        (0, controller(0, Controller::Wheel, 0.5)),
        (0, controller(0, Controller::Foot, 0.25)),
        (0, controller(0, Controller::Breath, 1.0)),
        (0, controller(0, Controller::Aftertouch, 0.75)),
        (0, controller(3, Controller::Wheel, 1.0)),
        (
            0,
            Kind::Bend {
                channel: 0,
                position: 0.3,
            },
        ),
        (
            0,
            Kind::Bend {
                channel: 3,
                position: -1.0,
            },
        ),
        (0, Kind::MasterTune { cents: 30.0 }),
        (1, on(0, 60, 100.0)),
        (
            2,
            Kind::PerNote {
                channel: 0,
                key: key(60),
                id: None,
                keys: 0.5,
            },
        ),
    ];
    render(&mut engine, &events, 3);
    let source = |position, range| Source { position, range };
    let context = engine.context(0).expect("a part");
    assert_eq!(
        context.pitch_controllers,
        control::controllers(&[source(0.5, 80.0), source(0.75, 40.0)])
    );
    assert_eq!(
        context.amplitude_controllers,
        control::controllers(&[source(0.5, 80.0), source(1.0, 50.0)])
    );
    assert_eq!(context.bias, control::bias(&[source(0.25, 60.0)]));
    assert_eq!(context.bend, control::bend(0.3, 7.0, 0.0));
    assert_eq!(context.master_tune, 30.0 / 1200.0);
    assert_eq!(
        engine.voice(0, 0).and_then(Voice::note).map(|note| note.per_note),
        Some(0.5)
    );
    // Under lfo.scope = part a key-on restarts the part's LFO, its phase at 0
    // with LFO SYNC; under voice it leaves it.
    let mut part = Part::initial(true);
    part.patch.voice.lfo.wave = Wave::Sine;
    for (scope, restarted) in [(0, true), (1, false)] {
        let mut engine = prepared(&[("lfo.scope", Value::Choice(scope))], &[part.clone()]);
        render(&mut engine, &[(1234, on(0, 60, 100.0))], 1235);
        let turns = engine.context(0).expect("a part").lfo.turns(Instant::at(1234));
        assert_eq!(turns == 0.0, restarted, "scope {scope}");
    }
}

#[test]
fn the_output_adds_the_parts_in_the_channels_it_has() {
    // Two parts at `output.gain` 0.7: the output is each part's sum times the
    // gain, added in the parts' order; under mono the second channel is left
    // as it was.
    let changes = [("output.gain", Value::Number(0.7))];
    let first = [(0, on(0, 60, 100.0))];
    let second = [(30, on(1, 67, 90.0))];
    let samples = 3000;
    let mut both = prepared(&changes, &two_parts());
    let [left, right] = render(&mut both, &[first[0], second[0]], samples);
    let [first_alone, _] = render(&mut prepared(&changes, &two_parts()), &first, samples);
    let [second_alone, _] = render(&mut prepared(&changes, &two_parts()), &second, samples);
    for n in 0..index(samples) {
        let expected = first_alone[n] + second_alone[n];
        assert_eq!(left[n].to_bits(), expected.to_bits(), "sample {n}");
        assert_eq!(right[n].to_bits(), expected.to_bits(), "sample {n}");
    }
    let mono = [
        ("output.gain", Value::Number(0.7)),
        ("output.channels", Value::Choice(1)),
    ];
    let mut alone = prepared(&mono, &two_parts());
    let [channel, untouched] = render_with(&mut alone, &[first[0], second[0]], samples, 7.0, || samples);
    assert!(channel.iter().zip(&left).all(|(a, b)| a.to_bits() == b.to_bits()));
    assert!(untouched.iter().all(|value| value.to_bits() == 7.0_f64.to_bits()));
}

#[test]
fn events_act_at_their_samples_within_the_block() {
    // An event beyond its block acts at the block's last sample, and one that
    // comes after an event of a later sample at that later sample.
    let block = |events: &[Event]| {
        let mut engine = prepared(&[], &[]);
        let [mut left, mut right] = [vec![0.0; 10], vec![0.0; 10]];
        engine.render(events, &mut [&mut left, &mut right]);
        let [rest, _] = render(&mut engine, &[], 500);
        left.into_iter().chain(rest).collect::<Vec<_>>()
    };
    let event = |offset, kind| Event { offset, kind };
    let beyond = block(&[event(50, on(0, 60, 100.0))]);
    let last = block(&[event(9, on(0, 60, 100.0))]);
    assert_eq!(beyond, last);
    let late = block(&[event(7, on(0, 60, 100.0)), event(3, on(0, 64, 100.0))]);
    let at_seven = block(&[event(7, on(0, 60, 100.0)), event(7, on(0, 64, 100.0))]);
    assert_eq!(late, at_seven);
    assert!(beyond.iter().any(|value| *value != 0.0));
}

#[test]
fn the_engine_refuses_what_it_cannot_prepare() {
    let refused = Engine::new(&settings(&[("engine.oversampling", Value::Integer(2))]), RATE, &[]);
    assert!(matches!(
        refused,
        Err(Refusal::Setting {
            name: "engine.oversampling",
            ..
        })
    ));
    let huge = Engine::new(&settings(&[("voice.count", Value::Integer(1 << 50))]), RATE, &[]);
    assert!(matches!(huge, Err(Refusal::Memory)));
}

/// `SplitMix64`, the tests' random source.
struct Random(u64);

impl Random {
    /// The next output.
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let z = (self.0 ^ (self.0 >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        let z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number from 0 up to 1.
    fn unit(&mut self) -> f64 {
        f64::from(u32::try_from(self.next() >> 40).expect("24 bits")) / 16_777_216.0
    }

    /// A number below a count.
    fn below(&mut self, count: u64) -> u64 {
        self.next() % count
    }
}

/// Random input over so many samples: notes on two channels, their releases,
/// the four controllers, the bend, the sustain pedal, per-note pitches and
/// the master tune, about one event every 40 samples.
fn random_events(random: &mut Random, samples: u64) -> Vec<(u64, Kind)> {
    let mut events = Vec::new();
    let mut held: Vec<(u8, u8)> = Vec::new();
    let controllers = [
        Controller::Wheel,
        Controller::Foot,
        Controller::Breath,
        Controller::Aftertouch,
    ];
    for at in 0..samples {
        if random.below(40) != 0 {
            continue;
        }
        let channel = u8::try_from(random.below(2)).expect("a channel");
        let kind = match random.below(10) {
            0..=2 => {
                let number = u8::try_from(36 + random.below(48)).expect("a key");
                held.push((channel, number));
                on(channel, number, 1.0 + 126.0 * random.unit())
            }
            3 | 4 if !held.is_empty() => {
                let (channel, number) =
                    held.swap_remove(index(random.below(u64::try_from(held.len()).expect("a short test"))));
                off(channel, number)
            }
            5 => Kind::Controller {
                channel,
                controller: controllers[index(random.below(4))],
                position: random.unit(),
            },
            6 => Kind::Bend {
                channel,
                position: 2.0 * random.unit() - 1.0,
            },
            7 => Kind::Sustain {
                channel,
                on: random.below(2) == 0,
            },
            8 if !held.is_empty() => {
                let (channel, number) = held[index(random.below(u64::try_from(held.len()).expect("a short test")))];
                Kind::PerNote {
                    channel,
                    key: key(number),
                    id: None,
                    keys: 4.0 * random.unit() - 2.0,
                }
            }
            _ => Kind::MasterTune {
                cents: 100.0 * random.unit() - 50.0,
            },
        };
        events.push((at, kind));
    }
    events
}

/// Parts for the random renders: the first, omni, plays an LFO that modulates
/// its pitch and its amplitude and a feedback at FB 7 on algorithm 32; the
/// second, on channel 1 from key 48 up, a note shift and a note limit of 2.
fn random_parts() -> [Part; 2] {
    let mut first = Part::initial(true);
    let voice = &mut first.patch.voice;
    voice.algorithm = Algorithm::new(32).expect("an algorithm");
    voice.feedback = value(7.0);
    voice.lfo.speed = value(80.0);
    voice.lfo.pitch_depth = value(30.0);
    voice.lfo.amplitude_depth = value(60.0);
    voice.lfo.wave = Wave::SampleAndHold;
    for operator in &mut voice.operators {
        operator.output_level = value(90.0);
        operator.amplitude_sensitivity = value(2.0);
        operator.envelope.rates = [value(90.0), value(60.0), value(50.0), value(70.0)];
    }
    first.function.foot_controller.range = value(70.0);
    first.function.foot_controller.assign.eg_bias = true;
    first.function.breath_controller.range = value(99.0);
    first.function.breath_controller.assign.amplitude = true;
    let mut second = Part::initial(true);
    second.omni = false;
    second.channel = Index::new(1).expect("a channel");
    second.lowest_key = Index::new(48).expect("a key");
    second.note_shift = NoteShift::new(-5.0).expect("a shift");
    second.note_limit = NonZeroU32::new(2);
    [first, second]
}

#[test]
fn a_render_is_the_same_however_its_samples_are_split_into_blocks() {
    // Each policy and retrigger, both channels' forms and both LFO scopes and
    // three feedback models, three voices for two parts: random input
    // rendered in one block and in blocks of 1 to 700 samples, bit for bit.
    let samples = 6000;
    let configurations = [(0, 1, 0, 0, 0), (1, 0, 1, 1, 1), (2, 1, 0, 1, 2), (3, 0, 0, 0, 0)];
    for (seed, &(steal, retrigger, channels, scope, model)) in (1_u64..).zip(&configurations) {
        let changes = [
            ("voice.count", Value::Integer(3)),
            ("voice.steal", Value::Choice(steal)),
            ("voice.retrigger", Value::Choice(retrigger)),
            ("output.channels", Value::Choice(channels)),
            ("lfo.scope", Value::Choice(scope)),
            ("feedback.model", Value::Choice(model)),
        ];
        let events = random_events(&mut Random(seed), samples);
        let whole = render(&mut prepared(&changes, &random_parts()), &events, samples);
        let mut random = Random(seed.wrapping_mul(7919));
        let split = render_with(&mut prepared(&changes, &random_parts()), &events, samples, 0.0, || {
            1 + random.below(700)
        });
        for (channel, (a, b)) in whole.iter().zip(&split).enumerate() {
            for (n, (x, y)) in a.iter().zip(b).enumerate() {
                assert_eq!(
                    x.to_bits(),
                    y.to_bits(),
                    "configuration {seed}, channel {channel}, sample {n}"
                );
            }
        }
        assert!(whole[0].iter().any(|value| value.abs() > 0.1), "configuration {seed}");
    }
}
