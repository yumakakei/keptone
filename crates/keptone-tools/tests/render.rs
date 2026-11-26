// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The offline renderer (docs/DESIGN.md §11.5): a voice's note rendered to a
//! WAV file against the engine's own render of it, bit for bit; the same bytes
//! whatever the blocks; the header's every field; the 32-bit samples, one
//! channel under mono, a bank's voice, a state's parts, the settings and the
//! engine's refusals; the arguments that do not read; and the binary's exit
//! codes, the file that it writes and the findings that it reports.

use std::ffi::OsString;

use keptone_engine::engine::{Engine, Event, Key, Kind};
use keptone_model::part::Part;
use keptone_model::settings::{self, Settings, Value};
use keptone_model::state::{self, State};
use keptone_model::syx::{self, Channel};
use keptone_model::voice::{Rounding, Value as Parameter, Voice};
use keptone_tools::render::{self, Blocks, Error, M3, Options, Source};
use keptone_tools::wav::{self, Format};

/// Arguments as the binary takes them.
fn arguments(words: &[&str]) -> Vec<OsString> {
    words.iter().map(OsString::from).collect()
}

/// The options of a render of `in.syx` to `out.wav`, with more arguments.
fn options(more: &[&str]) -> Options {
    render::parse(&arguments(&[&["in.syx", "-o", "out.wav"], more].concat())).expect("options")
}

/// A single voice's dump.
fn single(voice: &Voice) -> Vec<u8> {
    syx::write_voice(voice, Channel::new(0).expect("a channel"), Rounding::Nearest).0
}

/// VOICE INIT, every operator audible at OL 70, so that the note is heard
/// through its modulators, and sensitive to the velocity at KVS 3, so that the
/// note's velocity is heard too.
fn voice() -> Voice {
    let mut voice = Voice::init();
    for operator in &mut voice.operators {
        operator.output_level = Parameter::new(70.0).expect("a value");
        operator.velocity_sensitivity = Parameter::new(3.0).expect("a value");
    }
    voice
}

/// A render of a source, as options say.
fn rendered(source: Source<'_>, options: &Options) -> Vec<u8> {
    render::render(source, options).expect("a render").wav
}

/// A WAV file read back, every field of its header checked against the others
/// and against its length: its channels, rate, width and samples.
struct Wav {
    channels: u16,
    rate: u32,
    bits: u16,
    samples: Vec<f64>,
}

/// A little-endian u16 and u32 at an offset of bytes.
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().expect("two bytes"))
}

/// A little-endian u32 at an offset of bytes.
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"))
}

/// A u32 as a usize.
fn size(value: u32) -> usize {
    usize::try_from(value).expect("a size")
}

/// The fields of a WAV file that `crate::wav` writes, checked.
fn read(bytes: &[u8]) -> Wav {
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(size(u32_at(bytes, 4)), bytes.len() - 8);
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    assert_eq!(u32_at(bytes, 16), 18);
    assert_eq!(u16_at(bytes, 20), 3);
    let (channels, rate) = (u16_at(bytes, 22), u32_at(bytes, 24));
    let (block, bits) = (u16_at(bytes, 32), u16_at(bytes, 34));
    assert_eq!(u32_at(bytes, 28), rate * u32::from(block));
    assert_eq!(block, channels * bits / 8);
    assert_eq!(u16_at(bytes, 36), 0);
    assert_eq!(&bytes[38..42], b"fact");
    assert_eq!(u32_at(bytes, 42), 4);
    let frames = u32_at(bytes, 46);
    assert_eq!(&bytes[50..54], b"data");
    assert_eq!(u32_at(bytes, 54), frames * u32::from(block));
    assert_eq!(bytes.len(), 58 + size(frames) * usize::from(block));
    let samples = bytes[58..]
        .chunks_exact(usize::from(bits / 8))
        .map(|sample| match bits {
            64 => f64::from_le_bytes(sample.try_into().expect("eight bytes")),
            _ => f64::from(f32::from_le_bytes(sample.try_into().expect("four bytes"))),
        })
        .collect();
    Wav {
        channels,
        rate,
        bits,
        samples,
    }
}

/// The engine's render of the options' note in one block of so many samples,
/// in settings with milestone M3's values and those of `--set`, its parts
/// given, both channels interleaved, and its frames at the end that are 0 in
/// both left out.
fn reference(parts: &[Part], options: &Options, samples: u64) -> Vec<f64> {
    let mut settings = Settings::new();
    for (name, value) in M3 {
        assert!(settings.set(settings::number_of(name).expect("a setting"), value));
    }
    for &(number, value) in &options.settings {
        assert!(settings.set(number, value));
    }
    let mut engine = Engine::new(&settings, f64::from(options.rate), parts).expect("an engine");
    let length = usize::try_from(samples).expect("a length");
    let (mut left, mut right) = (vec![0.0; length], vec![0.0; length]);
    let key = options.note;
    let events = [
        Event {
            offset: 0,
            kind: Kind::NoteOn {
                channel: 0,
                key,
                velocity: options.velocity,
                id: None,
            },
        },
        Event {
            offset: hold_samples(options),
            kind: Kind::NoteOff {
                channel: 0,
                key,
                id: None,
            },
        },
    ];
    engine.render(&events, &mut [], &mut [&mut left, &mut right]);
    let mut frames: Vec<f64> = left.iter().zip(&right).flat_map(|(&l, &r)| [l, r]).collect();
    while frames.len() >= 2 && frames[frames.len() - 2..].iter().all(|&sample| sample == 0.0) {
        frames.truncate(frames.len() - 2);
    }
    frames
}

/// The sample of the options' release: the first at or after `--hold`, whole
/// here.
fn hold_samples(options: &Options) -> u32 {
    let samples = options.hold * f64::from(options.rate);
    assert_eq!(samples.fract(), 0.0, "a hold of whole samples");
    let mut whole = 0_u32;
    while f64::from(whole) < samples {
        whole += 1;
    }
    whole
}

/// The first part playing a voice, the others as they start.
fn played(voice: &Voice) -> Vec<Part> {
    let mut part = Part::initial(true);
    part.patch.voice = voice.clone();
    vec![part]
}

#[test]
fn a_voice_renders_as_the_engine_renders_its_note() {
    // A held note of half a second, its release ending it: the file's frames
    // the engine's render in one block, bit for bit, up to its last frame that
    // is not 0, which the file's last frame is.
    let soft = options(&["--hold", "0.5", "--note", "57", "--velocity", "90"]);
    let wav = read(&rendered(Source::Syx(&single(&voice())), &soft));
    assert_eq!((wav.channels, wav.rate, wav.bits), (2, 48_000, 64));
    let expected = reference(&played(&voice()), &soft, 96_000);
    assert!(expected.len() > 2 * 24_000 && expected.len() < 2 * 96_000);
    assert_eq!(wav.samples.len(), expected.len());
    for (n, (actual, expected)) in wav.samples.iter().zip(&expected).enumerate() {
        assert_eq!(actual.to_bits(), expected.to_bits(), "sample {n}");
    }
    assert!(wav.samples[wav.samples.len() - 2..].iter().any(|&sample| sample != 0.0));
    // The velocity is heard: at 100, the default, the note is another.
    let louder = read(&rendered(
        Source::Syx(&single(&voice())),
        &options(&["--hold", "0.5", "--note", "57"]),
    ));
    assert_ne!(louder.samples, wav.samples);
}

#[test]
fn the_blocks_do_not_change_the_bytes() {
    let source = single(&voice());
    let whole = rendered(Source::Syx(&source), &options(&["--hold", "0.2"]));
    for blocks in ["1", "7", "4096", "random:1", "random:2"] {
        let split = rendered(Source::Syx(&source), &options(&["--hold", "0.2", "--blocks", blocks]));
        assert!(split == whole, "--blocks {blocks}");
    }
    assert_eq!(options(&["--blocks", "random:9"]).blocks, Blocks::Random(9));
}

/// A binary64 rounded to the nearest binary32, as the renderer rounds it.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    reason = "Rust's conversion rounds to the nearest binary32, ties to even"
)]
fn narrowed(value: f64) -> f32 {
    value as f32
}

#[test]
fn the_formats_and_the_channels_follow_the_options() {
    // 32-bit samples, each the 64-bit one rounded to the nearest; one channel
    // under output.channels = mono, the stereo render's first; and output.gain
    // at 0.25, half its default, halving every sample.
    let source = single(&voice());
    let wide = read(&rendered(Source::Syx(&source), &options(&["--hold", "0.2"])));
    let narrow = read(&rendered(
        Source::Syx(&source),
        &options(&["--hold", "0.2", "--format", "f32"]),
    ));
    assert_eq!(narrow.bits, 32);
    assert_eq!(narrow.samples.len(), wide.samples.len());
    for (a, b) in narrow.samples.iter().zip(&wide.samples) {
        assert_eq!(a.to_bits(), f64::from(narrowed(*b)).to_bits());
    }
    let mono = read(&rendered(
        Source::Syx(&source),
        &options(&["--hold", "0.2", "--set", "output.channels=mono"]),
    ));
    assert_eq!(mono.channels, 1);
    let left: Vec<f64> = wide.samples.as_chunks::<2>().0.iter().map(|frame| frame[0]).collect();
    assert_eq!(mono.samples, left);
    let half = read(&rendered(
        Source::Syx(&source),
        &options(&["--hold", "0.2", "--set", "output.gain=0.25"]),
    ));
    for (a, b) in half.samples.iter().zip(&wide.samples) {
        assert_eq!(a.to_bits(), (0.5 * b).to_bits());
    }
    // A sample beyond binary32's largest is taken at it, with its sign.
    let saturated = wav::write(&[f64::MAX, -f64::MAX], 2, 1, Format::F32).expect("a file");
    assert_eq!(
        &saturated[58..],
        [f32::MAX.to_le_bytes(), (-f32::MAX).to_le_bytes()].concat()
    );
}

#[test]
fn a_bank_s_voice_and_a_state_s_parts_play() {
    // The fifth voice of a bank, its voices told apart by OP1's level, renders
    // as that voice's own dump does; a state whose first part plays the voice
    // renders as the voice's dump does, and its settings with it.
    let voices: [Voice; 32] = core::array::from_fn(|place| {
        let mut voice = voice();
        voice.operators[0].output_level =
            Parameter::new(60.0 + f64::from(u8::try_from(place).expect("a place"))).expect("a value");
        voice
    });
    let bank = syx::write_bank(&voices, Channel::new(0).expect("a channel"), Rounding::Nearest).0;
    let fifth = rendered(Source::Syx(&bank), &options(&["--hold", "0.2", "--voice", "5"]));
    assert!(fifth == rendered(Source::Syx(&single(&voices[4])), &options(&["--hold", "0.2"])));
    assert!(fifth != rendered(Source::Syx(&bank), &options(&["--hold", "0.2"])));
    let mut state = State::new();
    state.parts_mut()[0].patch.voice = voice();
    let text = state::write(&state);
    let of_state = rendered(Source::State(text.as_bytes()), &options(&["--hold", "0.2"]));
    assert!(of_state == rendered(Source::Syx(&single(&voice())), &options(&["--hold", "0.2"])));
    let mono = settings::number_of("output.channels").expect("a setting");
    assert!(state.set_setting(mono, Value::Choice(1)));
    let text = state::write(&state);
    assert_eq!(
        read(&rendered(Source::State(text.as_bytes()), &options(&["--hold", "0.2"]))).channels,
        1
    );
}

#[test]
fn a_held_note_ends_at_the_longest_render() {
    // Held for ten seconds, rendered for a tenth: 4800 frames.
    let options = options(&["--hold", "10", "--max-seconds", "0.1"]);
    let wav = read(&rendered(Source::Syx(&single(&voice())), &options));
    assert_eq!(wav.samples.len(), 2 * 4800);
}

#[test]
fn what_cannot_render_is_refused() {
    // The engine refuses what milestone M3 does not compute; a file without a
    // voice, and a voice that the dump does not hold, give no render; a state
    // that does not read neither.
    let source = single(&voice());
    let refused = |more: &[&str]| render::render(Source::Syx(&source), &options(more)).err();
    assert!(matches!(
        refused(&["--set", "engine.oversampling=2"]),
        Some(Error::Refused(keptone_engine::configuration::Refusal::Setting {
            name: "engine.oversampling",
            ..
        }))
    ));
    assert!(matches!(
        refused(&["--set", "tuning.source=scala"]),
        Some(Error::Refused(_))
    ));
    assert_eq!(refused(&["--voice", "2"]), Some(Error::Single(2)));
    assert_eq!(
        render::render(Source::Syx(&[0xF0, 0x43, 0x00, 0x7F, 0xF7]), &options(&[])).err(),
        Some(Error::NoVoice)
    );
    assert!(matches!(
        render::render(Source::State(b"{\"format\": 1}"), &options(&[])).err(),
        Some(Error::State(_))
    ));
    for error in [
        Error::NoVoice,
        Error::Single(3),
        Error::Memory,
        Error::Wav(wav::Error::Size),
    ] {
        assert_ne!(error.to_string(), "");
    }
    assert_eq!(wav::write(&[0.0; 3], 2, 48_000, Format::F64), Err(wav::Error::Channels));
    assert_eq!(wav::write(&[], 0, 48_000, Format::F64), Err(wav::Error::Channels));
    assert_eq!(wav::write(&[0.0; 2], 2, u32::MAX, Format::F64), Err(wav::Error::Size));
}

#[test]
fn the_arguments_that_do_not_read_are_refused() {
    let refused = |words: &[&str]| render::parse(&arguments(words)).is_err();
    for words in [
        &["in.syx"][..],
        &["-o", "out.wav"],
        &["a.syx", "b.syx", "-o", "out.wav"],
        &["in.syx", "--state", "s.json", "-o", "out.wav"],
        &["in.syx", "-o", "a.wav", "-o", "b.wav"],
        &["in.syx", "-o"],
        &["in.syx", "-o", "out.wav", "--bogus"],
        &["in.syx", "-o", "out.wav", "--note", "128"],
        &["in.syx", "-o", "out.wav", "--note", "-1"],
        &["in.syx", "-o", "out.wav", "--voice", "0"],
        &["in.syx", "-o", "out.wav", "--voice", "33"],
        &["in.syx", "-o", "out.wav", "--velocity", "inf"],
        &["in.syx", "-o", "out.wav", "--hold", "-1"],
        &["in.syx", "-o", "out.wav", "--max-seconds", "0"],
        &["in.syx", "-o", "out.wav", "--rate", "0"],
        &["in.syx", "-o", "out.wav", "--rate", "44100.5"],
        &["in.syx", "-o", "out.wav", "--rate", "4294967296"],
        &["in.syx", "-o", "out.wav", "--set", "output.gain"],
        &["in.syx", "-o", "out.wav", "--set", "no.such=1"],
        &["in.syx", "-o", "out.wav", "--set", "output.channels=quad"],
        &["in.syx", "-o", "out.wav", "--set", "output.gain=-1"],
        &["in.syx", "-o", "out.wav", "--set", "voice.count=+3"],
        &["in.syx", "-o", "out.wav", "--blocks", "0"],
        &["in.syx", "-o", "out.wav", "--blocks", "random:x"],
        &["in.syx", "-o", "out.wav", "--format", "f16"],
    ] {
        assert!(refused(words), "{words:?}");
    }
    // What reads: every option at once.
    let all = options(&[
        "--voice",
        "3",
        "--note",
        "0",
        "--velocity",
        "-5",
        "--hold",
        "0",
        "--rate",
        "96000",
        "--max-seconds",
        "2",
        "--set",
        "voice.count=8",
        "--set",
        "syx.accept_bad_checksum=false",
        "--set",
        "adaa.droop_eq_hz=auto",
        "--blocks",
        "64",
        "--format",
        "f32",
    ]);
    assert_eq!(all.voice, 3);
    assert_eq!(all.note, Key::new(0).expect("a key"));
    assert_eq!(
        (all.velocity, all.hold, all.rate, all.max_seconds),
        (-5.0, 0.0, 96_000, 2.0)
    );
    let number = |name| settings::number_of(name).expect("a setting");
    assert_eq!(
        all.settings,
        [
            (number("voice.count"), Value::Integer(8)),
            (number("syx.accept_bad_checksum"), Value::Switch(false)),
            (number("adaa.droop_eq_hz"), Value::Automatic),
        ]
    );
    assert_eq!((all.blocks, all.format), (Blocks::Fixed(64), Format::F32));
}

#[test]
fn the_binary_writes_the_file_and_exits_as_it_says() {
    // Its usage exits with 2, its help with 0, a render with 0, the file of the
    // library's bytes written and what reading the patch found on the standard
    // error; a missing input, a render that is not made and a file that is not
    // written with 1.
    let binary = env!("CARGO_BIN_EXE_keptone-tools");
    let directory = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    let input = directory.join("render-test-voice.syx");
    let output = directory.join("render-test-voice.wav");
    std::fs::write(&input, single(&voice())).expect("a file");
    let output_of = |words: &[&OsString]| {
        std::process::Command::new(binary)
            .args(words)
            .output()
            .expect("the binary")
    };
    let run = |words: &[&OsString]| output_of(words).status.code();
    let word = OsString::from;
    assert_eq!(run(&[]), Some(2));
    assert_eq!(run(&[&word("--help")]), Some(0));
    assert_eq!(run(&[&word("render"), &word("--help")]), Some(0));
    assert_eq!(run(&[&word("render"), &word("--bogus")]), Some(2));
    let (input_word, output_word) = (input.clone().into_os_string(), output.clone().into_os_string());
    let hold = [word("--hold"), word("0.2")];
    assert_eq!(
        run(&[
            &word("render"),
            &input_word,
            &word("-o"),
            &output_word,
            &hold[0],
            &hold[1]
        ]),
        Some(0)
    );
    let written = std::fs::read(&output).expect("the file");
    assert!(written == rendered(Source::Syx(&single(&voice())), &options(&["--hold", "0.2"])));
    let missing = directory.join("render-test-missing.syx").into_os_string();
    assert_eq!(run(&[&word("render"), &missing, &word("-o"), &output_word]), Some(1));
    let voice_2 = [word("--voice"), word("2")];
    assert_eq!(
        run(&[
            &word("render"),
            &input_word,
            &word("-o"),
            &output_word,
            &voice_2[0],
            &voice_2[1]
        ]),
        Some(1)
    );
    let nowhere = directory
        .join("render-test-no-such-directory")
        .join("out.wav")
        .into_os_string();
    assert_eq!(run(&[&word("render"), &input_word, &word("-o"), &nowhere]), Some(1));
    // A dump whose checksum is wrong: each finding on a line of its own, after
    // the file's name.
    let mut bad = single(&voice());
    let checksum = bad.len() - 2;
    bad[checksum] ^= 1;
    let bad_input = directory.join("render-test-bad.syx");
    std::fs::write(&bad_input, &bad).expect("a file");
    let reported = output_of(&[
        &word("render"),
        &bad_input.clone().into_os_string(),
        &word("-o"),
        &output_word,
        &hold[0],
        &hold[1],
    ]);
    assert_eq!(reported.status.code(), Some(0));
    let findings = render::render(Source::Syx(&bad), &options(&["--hold", "0.2"]))
        .expect("a render")
        .findings;
    assert_ne!(findings.len(), 0);
    let lines = String::from_utf8(reported.stderr).expect("text");
    let expected: Vec<String> = findings
        .iter()
        .map(|finding| format!("keptone-tools render: {}: {finding}", bad_input.to_string_lossy()))
        .collect();
    assert_eq!(lines.lines().collect::<Vec<_>>(), expected);
}

#[test]
fn the_syx_settings_and_the_release_s_sample_hold() {
    // A dump whose checksum is wrong renders, the finding reported, unless
    // syx.accept_bad_checksum is false; one whose first byte lies beyond its
    // range renders, clamped, unless syx.out_of_range is reject.
    let good = single(&voice());
    let mut bad = good.clone();
    let checksum = bad.len() - 2;
    bad[checksum] ^= 1;
    let render = render::render(Source::Syx(&bad), &options(&["--hold", "0.1"])).expect("a render");
    assert_ne!(render.findings.len(), 0);
    let strict = options(&["--hold", "0.1", "--set", "syx.accept_bad_checksum=false"]);
    assert_eq!(render::render(Source::Syx(&bad), &strict).err(), Some(Error::NoVoice));
    // The settings of --set are taken in their order, the last of a name
    // counting.
    let again = options(&[
        "--hold",
        "0.1",
        "--set",
        "syx.accept_bad_checksum=false",
        "--set",
        "syx.accept_bad_checksum=true",
    ]);
    assert!(render::render(Source::Syx(&bad), &again).is_ok());
    let mut wide = good.clone();
    wide[6] = 127;
    let render = render::render(Source::Syx(&wide), &options(&["--hold", "0.1"])).expect("a render");
    assert_ne!(render.findings.len(), 0);
    let reject = options(&["--hold", "0.1", "--set", "syx.out_of_range=reject"]);
    assert_eq!(render::render(Source::Syx(&wide), &reject).err(), Some(Error::NoVoice));
    // The release falls at the first sample at or after --hold: 0.48 and
    // 0.99 of a sample at 48 kHz both give sample 1, and 0 gives sample 0.
    let at = |hold: &str| rendered(Source::Syx(&good), &options(&["--hold", hold]));
    assert!(at("0.00001") == at("0.0000206"));
    assert!(at("0.00001") != at("0"));
}
