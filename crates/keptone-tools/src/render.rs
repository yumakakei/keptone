// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The offline renderer, `keptone-tools render` (docs/DESIGN.md §11.5): one
//! note of a patch rendered to a WAV file, offline, through the engine that the
//! plugin runs.
//!
//! [`parse`] reads the command's arguments into [`Options`], and [`render`]
//! renders a `.syx` file's voice or a native state's parts as they say into a
//! WAV file's bytes ([`crate::wav`]). The note sounds on channel 0 from the
//! render's first sample, and is released at the first sample at or after
//! `--hold` seconds; the render goes on in the blocks that `--blocks` gives
//! until no voice sounds once the key has been released, or `--max-seconds` at
//! most, and the frames at its end that are 0 in every channel are left out,
//! so that the bytes are the same whatever the blocks (docs/MATH.md §0.3).

use std::ffi::OsString;
use std::fmt;

use keptone_engine::configuration::{Channels, Refusal};
use keptone_engine::engine::{Engine, Event, Key, Kind};
use keptone_engine::random;
use keptone_model::part::Part;
use keptone_model::settings::{self, Kind as Takes, SPECS, Settings, Value};
use keptone_model::state;
use keptone_model::syx::{self, Content, OutOfRange, Sysex};

use crate::wav::{self, Format};

/// The file that holds the patch, and what kind of file it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// A `.syx` file, whose first dump of a voice or of a bank it plays.
    Syx(OsString),
    /// A native state, whose settings and parts it plays (`--state`).
    State(OsString),
}

/// The blocks into which a render is split (`--blocks`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocks {
    /// Blocks of so many samples, 1 or more.
    Fixed(u64),
    /// Blocks of 1 to [`LONGEST`] samples that `SplitMix64` draws from a seed,
    /// its i-th output from 1 on giving the i-th block 1 more than its
    /// remainder by [`LONGEST`] (docs/MATH.md §6.3).
    Random(u64),
}

/// The longest block of `--blocks random:SEED`.
pub const LONGEST: u64 = 4096;

impl Blocks {
    /// The length of the i-th block, from 0.
    fn length(self, block: u64) -> u64 {
        match self {
            Self::Fixed(length) => length,
            Self::Random(seed) => 1 + random::value(seed, block.saturating_add(1)) % LONGEST,
        }
    }

    /// The longest block that it gives.
    const fn longest(self) -> u64 {
        match self {
            Self::Fixed(length) => length,
            Self::Random(_) => LONGEST,
        }
    }
}

/// What a render is to be, as `keptone-tools render`'s arguments give it
/// (docs/DESIGN.md §11.5).
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// The file that holds the patch.
    pub input: Input,
    /// The voice of a bank that plays, 1 to 32 (`--voice`).
    pub voice: usize,
    /// The note's key (`--note`).
    pub note: Key,
    /// Its velocity, on the scale of 0 to 127 (`--velocity`).
    pub velocity: f64,
    /// The seconds from its start to its release, 0 or more (`--hold`).
    pub hold: f64,
    /// The host's rate in samples a second (`--rate`).
    pub rate: u32,
    /// The longest render in seconds, above 0 (`--max-seconds`).
    pub max_seconds: f64,
    /// The settings that `--set` gives, by their numbers, in their order.
    pub settings: Vec<(u16, Value)>,
    /// The blocks into which it is split.
    pub blocks: Blocks,
    /// The width of the WAV file's samples (`--format`).
    pub format: Format,
    /// The WAV file to write (`-o`).
    pub output: OsString,
}

/// The settings that milestone M3 does not compute yet, at the values that it
/// computes, which a render takes unless `--set` gives others (docs/DESIGN.md
/// §16).
pub const M3: [(&str, Value); 3] = [
    ("engine.oversampling", Value::Integer(1)),
    ("adaa.order", Value::Choice(0)),
    ("feedback.series_beta_max", Value::Number(0.0)),
];

/// The value that a text gives a setting: a switch's `true` or `false`, a
/// choice's name, a number's binary64, `auto` for a number that MATH gives as
/// a formula, and an integer's decimal digits; none where the text gives none
/// that the setting takes.
#[must_use]
pub fn setting_value(spec: &settings::Spec, text: &str) -> Option<Value> {
    let value = match spec.kind {
        Takes::Switch => match text {
            "true" => Value::Switch(true),
            "false" => Value::Switch(false),
            _ => return None,
        },
        Takes::Choice(names) => Value::Choice(u8::try_from(names.iter().position(|name| *name == text)?).ok()?),
        Takes::NumberOrAutomatic(_) if text == "auto" => Value::Automatic,
        Takes::Number(_) | Takes::NumberOrAutomatic(_) => Value::Number(text.parse().ok()?),
        Takes::Count(_) | Takes::PowerOfTwo | Takes::Seed => Value::Integer(decimal(text)?),
    };
    spec.takes(value).then_some(value)
}

/// An integer of decimal digits alone, within 64 bits.
fn decimal(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// The command's usage, which an error in its arguments prints.
pub const USAGE: &str = "\
usage: keptone-tools render (<voice.syx> | --state <state.json>) -o <out.wav> [options]

  --voice N            the voice of a bank of 32, 1 to 32 (1)
  --note K             the key, 0 to 127 (60)
  --velocity V         the velocity, on the scale of 0 to 127 (100)
  --hold S             the seconds before the key's release (1)
  --rate HZ            the rate, a whole number of samples a second (48000)
  --max-seconds S      the longest render, in seconds (60)
  --set NAME=VALUE     a setting of docs/MATH.md §17, again for each
  --blocks N | random:SEED
                       blocks of N samples (512), or of random lengths
  --format f64 | f32   the WAV file's samples (f64)";

/// The options that `keptone-tools render`'s arguments give, those after the
/// command's name (docs/DESIGN.md §11.5).
///
/// # Errors
///
/// A message naming what the arguments lack or what of theirs does not read:
/// an option that is not one, a value that is missing or out of its range, a
/// setting that has no such name or does not take the value, and an input
/// given twice or not at all, or no output.
pub fn parse(arguments: &[OsString]) -> Result<Options, String> {
    let mut input = None;
    let mut output = None;
    let mut options = Options {
        input: Input::Syx(OsString::new()),
        voice: 1,
        note: Key::new(60).ok_or("no key 60")?,
        velocity: 100.0,
        hold: 1.0,
        rate: 48_000,
        max_seconds: 60.0,
        settings: Vec::new(),
        blocks: Blocks::Fixed(512),
        format: Format::F64,
        output: OsString::new(),
    };
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        let mut value = |name: &str| rest.next().ok_or_else(|| format!("{name} wants a value"));
        match argument.to_str() {
            Some("--state") => {
                once(input.as_ref())?;
                input = Some(Input::State(value("--state")?.clone()));
            }
            Some("-o") => {
                once(output.as_ref())?;
                output = Some(value("-o")?.clone());
            }
            Some(name @ "--voice") => options.voice = number(name, text(value(name)?)?, 1..=32)?,
            Some(name @ "--note") => {
                let key = number(name, text(value(name)?)?, 0..=127)?;
                options.note = u8::try_from(key)
                    .ok()
                    .and_then(Key::new)
                    .ok_or("--note is out of range")?;
            }
            Some(name @ "--velocity") => options.velocity = real(name, text(value(name)?)?, |_| true)?,
            Some(name @ "--hold") => options.hold = real(name, text(value(name)?)?, |hold| hold >= 0.0)?,
            Some(name @ "--rate") => {
                let rate = number(
                    name,
                    text(value(name)?)?,
                    1..=usize::try_from(u32::MAX).unwrap_or(usize::MAX),
                )?;
                options.rate = u32::try_from(rate).map_err(|_| "--rate is out of range")?;
            }
            Some(name @ "--max-seconds") => {
                options.max_seconds = real(name, text(value(name)?)?, |seconds| seconds > 0.0)?;
            }
            Some("--set") => options.settings.push(set(text(value("--set")?)?)?),
            Some("--blocks") => options.blocks = blocks(text(value("--blocks")?)?)?,
            Some("--format") => {
                options.format = match text(value("--format")?)? {
                    "f64" => Format::F64,
                    "f32" => Format::F32,
                    other => return Err(format!("--format takes f64 or f32, not {other}")),
                };
            }
            Some(other) if other.starts_with('-') => return Err(format!("no option {other}")),
            _ => {
                once(input.as_ref())?;
                input = Some(Input::Syx(argument.clone()));
            }
        }
    }
    options.input = input.ok_or("no input: a .syx file, or --state with a state")?;
    options.output = output.ok_or("no output: -o with a WAV file")?;
    Ok(options)
}

/// An error where the input or the output was given before.
fn once<T>(before: Option<&T>) -> Result<(), String> {
    match before {
        Some(_) => Err("the input or the output is given twice".to_owned()),
        None => Ok(()),
    }
}

/// An argument as text, which every option's value but a file's is.
fn text(argument: &OsString) -> Result<&str, String> {
    argument
        .to_str()
        .ok_or_else(|| format!("{} is not text", argument.to_string_lossy()))
}

/// An option's whole number within a range.
fn number(name: &str, text: &str, range: core::ops::RangeInclusive<usize>) -> Result<usize, String> {
    decimal(text)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| range.contains(value))
        .ok_or_else(|| format!("{name} takes a whole number from {} to {}", range.start(), range.end()))
}

/// An option's finite binary64 that a test takes.
fn real(name: &str, text: &str, takes: impl Fn(f64) -> bool) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && takes(*value))
        .ok_or_else(|| format!("{name} does not take {text}"))
}

/// A setting's number and value from `NAME=VALUE`.
fn set(text: &str) -> Result<(u16, Value), String> {
    let (name, value) = text.split_once('=').ok_or("--set takes NAME=VALUE")?;
    let number = settings::number_of(name).ok_or_else(|| format!("no setting {name}"))?;
    let spec = SPECS
        .get(usize::from(number))
        .ok_or_else(|| format!("no setting {name}"))?;
    let value = setting_value(spec, value).ok_or_else(|| format!("{name} does not take {value}"))?;
    Ok((number, value))
}

/// The blocks of `N` or `random:SEED`.
fn blocks(text: &str) -> Result<Blocks, String> {
    if let Some(seed) = text.strip_prefix("random:") {
        return decimal(seed)
            .map(Blocks::Random)
            .ok_or_else(|| format!("--blocks does not take {text}"));
    }
    decimal(text)
        .filter(|&length| length >= 1)
        .map(Blocks::Fixed)
        .ok_or_else(|| format!("--blocks does not take {text}"))
}

/// The bytes of the file that holds the patch, by its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source<'a> {
    /// A `.syx` file's.
    Syx(&'a [u8]),
    /// A native state's.
    State(&'a [u8]),
}

/// A render: the WAV file's bytes, and what reading the patch found that it
/// read all the same.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Render {
    /// The WAV file's bytes.
    pub wav: Vec<u8>,
    /// What reading the patch found, each as text.
    pub findings: Vec<String>,
}

/// Why a render was not made.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// The `.syx` file holds no dump of a voice or of a bank.
    NoVoice,
    /// `--voice` asks a single voice's dump for another than its one.
    Single(usize),
    /// The state does not read.
    State(state::Error),
    /// A setting does not take its value.
    Setting(&'static str),
    /// The engine refuses the settings or the rate.
    Refused(Refusal),
    /// The render does not make a WAV file.
    Wav(wav::Error),
    /// The memory for the render's blocks or its samples cannot be had.
    Memory,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoVoice => write!(formatter, "the .syx file holds no dump of a voice or of a bank of 32"),
            Self::Single(voice) => write!(formatter, "--voice {voice}: the dump holds one voice"),
            Self::State(error) => write!(formatter, "the state does not read: {error}"),
            Self::Setting(name) => write!(formatter, "{name} does not take its value"),
            Self::Refused(Refusal::Setting { name, value }) => {
                write!(formatter, "milestone M3 does not compute {name} at {value:?}")
            }
            Self::Refused(Refusal::HostRate(rate)) => write!(formatter, "the engine does not take the rate {rate}"),
            Self::Refused(Refusal::Memory) => write!(formatter, "the memory for the voices cannot be had"),
            Self::Wav(wav::Error::Channels) => write!(formatter, "the samples make no whole frames"),
            Self::Wav(wav::Error::Size) => write!(formatter, "the render does not fit a WAV file's 32-bit sizes"),
            Self::Memory => write!(formatter, "the memory for the render cannot be had"),
        }
    }
}

impl std::error::Error for Error {}

/// Settings with milestone M3's values of what it does not compute yet, then
/// the changes that `--set` gives, in their order.
fn configured(mut settings: Settings, changes: &[(u16, Value)]) -> Result<Settings, Error> {
    let fixed = M3
        .iter()
        .filter_map(|&(name, value)| Some((settings::number_of(name)?, value)));
    for (number, value) in fixed.chain(changes.iter().copied()) {
        if !settings.set(number, value) {
            let name = SPECS.get(usize::from(number)).map_or("a setting", |spec| spec.name);
            return Err(Error::Setting(name));
        }
    }
    Ok(settings)
}

/// A setting's value by its name, if there is one.
fn value_of(settings: &Settings, name: &str) -> Option<Value> {
    settings.get(settings::number_of(name)?)
}

/// The options with which a `.syx` file is read: `syx.accept_bad_checksum` and
/// `syx.out_of_range`.
fn syx_options(settings: &Settings) -> syx::Options {
    syx::Options {
        accept_bad_checksum: value_of(settings, "syx.accept_bad_checksum") != Some(Value::Switch(false)),
        out_of_range: match value_of(settings, "syx.out_of_range") {
            Some(Value::Choice(1)) => OutOfRange::Reject,
            _ => OutOfRange::Clamp,
        },
    }
}

/// The parts that play a `.syx` file's first dump of a voice or of a bank, its
/// voice as the first part, and what reading it found.
fn syx_parts(bytes: &[u8], voice: usize, options: syx::Options) -> Result<(Vec<Part>, Vec<String>), Error> {
    for message in syx::read(bytes, options) {
        let Ok(Sysex::Dump(dump)) = message.result else {
            continue;
        };
        let played = match dump.content {
            Content::Voice(single) if voice == 1 => *single,
            Content::Voice(_) => return Err(Error::Single(voice)),
            Content::Bank(bank) => bank.get(voice - 1).cloned().ok_or(Error::Single(voice))?,
            _ => continue,
        };
        let mut part = Part::initial(true);
        part.patch.voice = played;
        let findings = dump.findings.iter().map(ToString::to_string).collect();
        return Ok((vec![part], findings));
    }
    Err(Error::NoVoice)
}

/// A whole count of samples of 0 or more as a u64, binary64's 2⁶⁴ and beyond
/// taken at u64's largest.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole binary64 of [0, 2⁶⁴) converts exactly, and Rust's conversion saturates beyond"
)]
fn whole(count: f64) -> u64 {
    count.max(0.0) as u64
}

/// The first sample at or after so many seconds at a rate.
fn sample_at(seconds: f64, rate: u32) -> u64 {
    whole((seconds * f64::from(rate)).ceil())
}

/// One note of a patch rendered as options say into a WAV file's bytes
/// (docs/DESIGN.md §11.5).
///
/// # Errors
///
/// Where the `.syx` file holds no voice that `--voice` names or the state does
/// not read, where a setting does not take its value, where the engine refuses
/// the settings or the rate, and where the render does not fit a WAV file.
pub fn render(source: Source<'_>, options: &Options) -> Result<Render, Error> {
    let defaults = configured(Settings::new(), &options.settings)?;
    let (settings, parts, findings) = match source {
        Source::Syx(bytes) => {
            let (parts, findings) = syx_parts(bytes, options.voice, syx_options(&defaults))?;
            (defaults, parts, findings)
        }
        Source::State(bytes) => {
            let depth = match value_of(&defaults, "json.max_depth") {
                Some(Value::Integer(depth)) => usize::try_from(depth).unwrap_or(usize::MAX),
                _ => 64,
            };
            let (state, findings) = state::read(bytes, depth).map_err(Error::State)?;
            let settings = configured(state.settings().clone(), &options.settings)?;
            let findings = findings.iter().map(|finding| format!("{finding:?}")).collect();
            (settings, state.parts().to_vec(), findings)
        }
    };
    let mut engine = Engine::new(&settings, f64::from(options.rate), &parts).map_err(Error::Refused)?;
    let stereo = engine.configuration().channels == Channels::Stereo;
    let channels: u16 = if stereo { 2 } else { 1 };
    let total = sample_at(options.max_seconds, options.rate).max(1);
    let release = sample_at(options.hold, options.rate);
    // The most frames that the chunks of a WAV file hold within RIFF's
    // 32-bit sizes: its headers' 50 bytes, then the frames.
    let width = u64::from(channels) * if options.format == Format::F64 { 8 } else { 4 };
    let room = (u64::from(u32::MAX) - 50) / width;
    let longest = usize::try_from(options.blocks.longest().min(total).min(room)).map_err(|_| Error::Memory)?;
    let (mut left, mut right) = (Vec::new(), Vec::new());
    left.try_reserve_exact(longest).map_err(|_| Error::Memory)?;
    right.try_reserve_exact(longest).map_err(|_| Error::Memory)?;
    left.resize(longest, 0.0);
    right.resize(longest, 0.0);
    let mut samples: Vec<f64> = Vec::new();
    let (mut done, mut block) = (0, 0);
    while done < total && !(done > release && engine.sounding() == 0) {
        let length = options.blocks.length(block).min(total - done);
        block += 1;
        if done + length > room {
            return Err(Error::Wav(wav::Error::Size));
        }
        let mut events = Vec::with_capacity(2);
        let at = |sample: u64| u32::try_from(sample - done).unwrap_or(u32::MAX);
        if done == 0 {
            events.push(Event {
                offset: 0,
                kind: Kind::NoteOn {
                    channel: 0,
                    key: options.note,
                    velocity: options.velocity,
                    id: None,
                },
            });
        }
        if (done..done + length).contains(&release) {
            events.push(Event {
                offset: at(release),
                kind: Kind::NoteOff {
                    channel: 0,
                    key: options.note,
                    id: None,
                },
            });
        }
        let span = usize::try_from(length).map_err(|_| Error::Memory)?;
        samples
            .try_reserve(span * usize::from(channels))
            .map_err(|_| Error::Memory)?;
        let (first, second) = (&mut left[..span], &mut right[..span]);
        engine.render(&events, &mut [], &mut [first, second]);
        for (index, &sample) in left[..span].iter().enumerate() {
            samples.push(sample);
            if stereo {
                samples.push(right[index]);
            }
        }
        done += length;
    }
    let width = usize::from(channels);
    while samples.len() >= width && samples[samples.len() - width..].iter().all(|&sample| sample == 0.0) {
        samples.truncate(samples.len() - width);
    }
    let wav = wav::write(&samples, channels, options.rate, options.format).map_err(Error::Wav)?;
    Ok(Render { wav, findings })
}
