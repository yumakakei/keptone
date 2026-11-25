// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The engine (docs/MATH.md §0.3, §13, §14; DESIGN §5, §6): its parts, each
//! with slots of its own that hold its voices, the stream of timed events that
//! is its input, and its output, rendered an engine sample at a time.
//!
//! Each engine sample n is taken in the order of MATH §0.3. Every voice that
//! sounds integrates the sample from n − 1 by the laws that held over it; the
//! events of sample n act, in their order; every part sums its voices in the
//! order of its slots and the output adds the parts in the order of their
//! indices (§14); then each voice's end is tested (§13.3), and at every 64th
//! sample, counted from the engine's first, its feedback's tiny states are
//! set to 0 (§13.4). Nothing happens at a block's boundary, so that a render
//! does not depend on how the host splits its samples into blocks.
//!
//! A part takes the notes of the channels it receives within its keys and its
//! key filter (§13.5), each into one of its slots as §13.2 allocates them,
//! under `voice.steal` and `voice.retrigger`, within its note limit and the
//! pool of `voice.count` voices that sound; its controllers, its bend and its
//! sustain pedal act on its voices (§2.4, §7.1, §4.5), and its LFO, under
//! `lfo.scope = part`, restarts at each key-on (§6.5). Milestone M3 renders at
//! the host's rate, `engine.oversampling` at 1. Until the next commit gives
//! them their laws (§7.3), the portamento's foot switch stays off, as it
//! starts, so that no note glides, and a mono part plays as a poly one.

use keptone_model::dx7ii::Index;
use keptone_model::part;
use keptone_model::settings::Settings;

use crate::binary64::finite;
use crate::compile::CompiledPart;
use crate::configuration::{Channels, Configuration, LfoScope, Refusal, Retrigger, Steal};
use crate::control::{self, Limited, Source};
use crate::lfo::Lfo;
use crate::pieces::Portamento;
use crate::pitch::TwelveTet;
use crate::random::Streams;
use crate::time::Instant;
use crate::voice::{Context, Note, Voice};

/// One of MIDI's 128 keys.
pub type Key = Index<127>;

/// A controller of MATH §7.1, in the order in which their depths add.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Controller {
    /// The modulation wheel.
    Wheel,
    /// The foot controller.
    Foot,
    /// The breath controller.
    Breath,
    /// Aftertouch, the channel's pressure.
    Aftertouch,
}

impl Controller {
    /// Its index in the function data's order.
    const fn index(self) -> usize {
        match self {
            Self::Wheel => 0,
            Self::Foot => 1,
            Self::Breath => 2,
            Self::Aftertouch => 3,
        }
    }
}

/// What an event of the engine's input does (MATH §0.3). A channel is numbered
/// across MIDI 2.0's groups, sixteen for each group before its own and then
/// its channel within its group, 0 to 255; a note is known by its channel and
/// its key before the note shift, or by its identifier where the host gives
/// one (§13.2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// A note's start, at a velocity on the scale of 0 to 127.
    NoteOn {
        /// Its channel.
        channel: u8,
        /// Its key.
        key: Key,
        /// Its velocity.
        velocity: f64,
        /// Its identifier, where the host gives one.
        id: Option<u64>,
    },
    /// A note's release, which goes to the voices that its note-on started.
    NoteOff {
        /// Its channel.
        channel: u8,
        /// Its key.
        key: Key,
        /// Its identifier, where the host gives one; none names every note of
        /// the channel and the key.
        id: Option<u64>,
    },
    /// A controller's position, from 0 at rest to 1 at its far end (§7.1).
    Controller {
        /// Its channel.
        channel: u8,
        /// The controller.
        controller: Controller,
        /// Its position.
        position: f64,
    },
    /// The bend's position, from −1 to 1, 0 at its centre (§2.4).
    Bend {
        /// Its channel.
        channel: u8,
        /// Its position.
        position: f64,
    },
    /// The sustain pedal pressed or released (§4.5).
    Sustain {
        /// Its channel.
        channel: u8,
        /// Whether it is pressed.
        on: bool,
    },
    /// A note's per-note pitch kₙₒₜₑ in keys (DESIGN §7.3).
    PerNote {
        /// Its channel.
        channel: u8,
        /// Its key.
        key: Key,
        /// Its identifier, where the host gives one.
        id: Option<u64>,
        /// The pitch, in keys.
        keys: f64,
    },
    /// The master tune in cents (§2.4), the same for every part.
    MasterTune {
        /// The cents.
        cents: f64,
    },
}

/// An event of the engine's input (MATH §0.3), at a sample of the block that
/// it comes with, counted from the block's first, 0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    /// Its sample within its block.
    pub offset: u32,
    /// What it does.
    pub kind: Kind,
}

/// What a note is known by (MATH §13.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Identity {
    /// Its channel.
    channel: u8,
    /// Its key, before the note shift.
    key: u8,
    /// Its identifier, where the host gave one.
    id: Option<u64>,
}

impl Identity {
    /// Whether an event names this note: by its identifier where the event
    /// gives one, and otherwise by its channel and its key.
    fn named(self, channel: u8, key: u8, id: Option<u64>) -> bool {
        match id {
            Some(id) => self.id == Some(id),
            None => self.channel == channel && self.key == key,
        }
    }
}

/// A part's slot (MATH §13.2): its voice, and what the part keeps of the note
/// that the voice plays.
#[derive(Debug, Clone)]
struct Slot {
    /// Its voice.
    voice: Voice,
    /// What the voice's note is known by.
    identity: Option<Identity>,
    /// Whether the note's key is down.
    down: bool,
    /// Whether the sustain pedal holds the note after its key's release.
    sustained: bool,
    /// When the note started, in the engine's order of events.
    started: u64,
    /// When its key was released, in that order.
    released: u64,
    /// Where another part took its voice, from which the voice fades out.
    fading: Option<Instant>,
}

impl Slot {
    /// Whether its voice plays a note that has not ended and that no other
    /// part has taken: one that the pool counts.
    fn playing(&self) -> bool {
        self.voice.note().is_some() && self.voice.ending().is_none() && self.fading.is_none()
    }

    /// Whether it is free: its voice's last note has ended, or it has had none.
    fn free(&self) -> bool {
        self.voice.note().is_none() || self.voice.ending().is_some()
    }

    /// Whether its voice plays a note whose key is up: released, or held by
    /// the sustain pedal alone.
    fn up(&self) -> bool {
        self.playing() && !self.down
    }
}

/// What a part keeps beside its slots: its compiled part, its LFO and its
/// random streams, its controllers, its bend and its sustain pedal, the slot
/// that it gave last and how many notes it has started.
#[derive(Debug, Clone)]
struct Controls {
    /// The part as the engine takes it.
    compiled: CompiledPart,
    /// Its LFO (MATH §6.5).
    lfo: Lfo,
    /// Its random streams (MATH §6.3).
    streams: Streams,
    /// The controllers' positions, in the function data's order.
    positions: [f64; 4],
    /// The depth of the controllers assigned to the pitch (MATH §7.1).
    pitch: Limited,
    /// The depth of those assigned to the amplitude.
    amplitude: Limited,
    /// The EG bias of those assigned to it.
    bias: Limited,
    /// The bend's position.
    position: f64,
    /// The bend in keys (MATH §2.4).
    bend: f64,
    /// Whether its sustain pedal is pressed.
    sustain: bool,
    /// The slot that it gave last.
    last: Option<usize>,
    /// How many notes it has started, the m of MATH §6.3.
    notes: u64,
}

impl Controls {
    /// What its voices take from it.
    fn context<'a>(&'a self, configuration: &'a Configuration, tuning: &'a TwelveTet, master_tune: f64) -> Context<'a> {
        Context {
            patch: &self.compiled.patch,
            configuration,
            tuning,
            lfo: &self.lfo,
            note_shift: self.compiled.note_shift,
            pitch_controllers: self.pitch,
            amplitude_controllers: self.amplitude,
            bias: self.bias,
            bend: self.bend,
            master_tune,
            streams: self.streams,
        }
    }

    /// The controllers' depths and bias from their positions and the function
    /// data's ranges and assignments (MATH §7.1), and the bend from its
    /// position (§2.4).
    fn settle(&mut self) {
        let function = &self.compiled.patch.function;
        let assigned = |pick: fn(&crate::compile::CompiledController) -> bool| {
            let mut sources = [Source {
                position: 0.0,
                range: 0.0,
            }; 4];
            let mut count = 0;
            for (controller, &position) in function.controllers.iter().zip(&self.positions) {
                if pick(controller)
                    && let Some(source) = sources.get_mut(count)
                {
                    *source = Source {
                        position,
                        range: controller.range,
                    };
                    count += 1;
                }
            }
            (sources, count)
        };
        let (pitch, count) = assigned(|controller| controller.pitch);
        self.pitch = control::controllers(&pitch[..count]);
        let (amplitude, count) = assigned(|controller| controller.amplitude);
        self.amplitude = control::controllers(&amplitude[..count]);
        let (bias, count) = assigned(|controller| controller.eg_bias);
        self.bias = control::bias(&bias[..count]);
        self.bend = control::bend(self.position, function.bend_range, function.bend_step);
    }
}

/// A part of the engine (MATH §13.5).
#[derive(Debug, Clone)]
struct PartState {
    /// What it keeps beside its slots.
    controls: Controls,
    /// Its slots, `voice.count` of them.
    slots: Vec<Slot>,
}

impl PartState {
    /// The first of its slots after the one that it gave last, in their order
    /// from there round to it, that meets a test.
    fn after_last(&self, test: impl Fn(&Slot) -> bool) -> Option<usize> {
        let count = self.slots.len();
        let first = self.controls.last.map_or(0, |last| last + 1);
        (0..count)
            .map(|step| (first + step) % count.max(1))
            .find(|&k| self.slots.get(k).is_some_and(&test))
    }

    /// How many of its voices the pool counts.
    fn playing(&self) -> usize {
        self.slots.iter().filter(|slot| slot.playing()).count()
    }
}

/// The fade of a voice that another part took, at an instant (MATH §13.2):
/// from 1 at the instant at which it was taken down to 0 over
/// `param.level_glide_s`, linearly, as a switch moves its operator's factor
/// (§8.5), and 0 at once where that is 0.
fn faded(start: Instant, at: Instant, glide: f64) -> f64 {
    if glide <= 0.0 {
        return 0.0;
    }
    (1.0 - at.samples_since(start).max(0.0) / glide).max(0.0)
}

/// The engine (MATH §0.3, §13, §14): the configuration that it was prepared
/// with, its parts, and the sample that it computes next.
#[derive(Debug, Clone)]
pub struct Engine {
    /// Its configuration, fixed when it is prepared.
    configuration: Configuration,
    /// Its tuning, 12-TET in milestone M3.
    tuning: TwelveTet,
    /// Its parts, `part.count` of them.
    parts: Vec<PartState>,
    /// The engine sample that it computes next, from 0.
    sample: u64,
    /// The master tune in octaves (MATH §2.4).
    master_tune: f64,
    /// How many voices sound, as the pool of MATH §13.1 counts them.
    pool: usize,
    /// The order of the notes that have started and the keys that have been
    /// released, which the stealing policies compare.
    order: u64,
}

impl Engine {
    /// An engine prepared in settings at a host rate, its parts from those
    /// given in their order, those not given as they start (MATH §13.5), the
    /// first on and every other off, and those beyond `part.count` left out:
    /// every part's `voice.count` slots and their voices taken now, as the
    /// engine is prepared and never on the audio thread (MATH §13.1).
    ///
    /// # Errors
    ///
    /// The configuration's refusal (`Configuration::new`), and
    /// [`Refusal::Memory`] where the memory cannot be had.
    pub fn new(settings: &Settings, host_rate: f64, parts: &[part::Part]) -> Result<Self, Refusal> {
        let configuration = Configuration::new(settings, host_rate)?;
        let mut built = Vec::new();
        built
            .try_reserve_exact(configuration.part_count)
            .map_err(|_| Refusal::Memory)?;
        for index in 0..configuration.part_count {
            let compiled = match parts.get(index) {
                Some(given) => CompiledPart::new(given, &configuration),
                None => CompiledPart::new(&part::Part::initial(index == 0), &configuration),
            };
            let streams = Streams::of(configuration.lfo_seed, u64::try_from(index).unwrap_or(u64::MAX));
            let lfo = Lfo::new(
                Instant::at(0),
                compiled.patch.lfo.speed,
                configuration.sample_rate,
                streams.part,
                0,
            );
            let mut slots = Vec::new();
            slots
                .try_reserve_exact(configuration.voice_count)
                .map_err(|_| Refusal::Memory)?;
            for _ in 0..configuration.voice_count {
                let voice = Voice::new(configuration.operators, configuration.max_pieces, Instant::at(0))
                    .ok_or(Refusal::Memory)?;
                slots.push(Slot {
                    voice,
                    identity: None,
                    down: false,
                    sustained: false,
                    started: 0,
                    released: 0,
                    fading: None,
                });
            }
            let none = Limited {
                value: 0.0,
                on_scale: 0.0,
            };
            let mut controls = Controls {
                compiled,
                lfo,
                streams,
                positions: [0.0; 4],
                pitch: none,
                amplitude: none,
                bias: none,
                position: 0.0,
                bend: 0.0,
                sustain: false,
                last: None,
                notes: 0,
            };
            controls.settle();
            built.push(PartState { controls, slots });
        }
        Ok(Self {
            tuning: TwelveTet::new(configuration.a4_hz),
            configuration,
            parts: built,
            sample: 0,
            master_tune: 0.0,
            pool: 0,
            order: 0,
        })
    }

    /// Its configuration.
    #[must_use]
    pub const fn configuration(&self) -> &Configuration {
        &self.configuration
    }

    /// The engine sample that it computes next, from 0.
    #[must_use]
    pub const fn sample(&self) -> u64 {
        self.sample
    }

    /// How many voices sound, as the pool of MATH §13.1 counts them: those
    /// whose notes have not ended and that no other part has taken.
    #[must_use]
    pub const fn sounding(&self) -> usize {
        self.pool
    }

    /// The voice in a part's slot, none beyond them.
    #[must_use]
    pub fn voice(&self, part: usize, slot: usize) -> Option<&Voice> {
        self.parts.get(part)?.slots.get(slot).map(|slot| &slot.voice)
    }

    /// Whether the voice in a part's slot fades out, taken by another part
    /// (MATH §13.2).
    #[must_use]
    pub fn fading(&self, part: usize, slot: usize) -> bool {
        self.parts
            .get(part)
            .and_then(|part| part.slots.get(slot))
            .is_some_and(|slot| slot.fading.is_some())
    }

    /// What a part's voices take from it now, as its voices' probes read
    /// them; none beyond the parts.
    #[must_use]
    pub fn context(&self, part: usize) -> Option<Context<'_>> {
        let part = self.parts.get(part)?;
        Some(
            part.controls
                .context(&self.configuration, &self.tuning, self.master_tune),
        )
    }

    /// Renders a block of samples into the output's channels, the events that
    /// come with it acting each at its sample, in their order (MATH §0.3):
    /// under `output.channels = stereo` each part's sum at the centre goes to
    /// both of the first two channels given, and under `mono` to the first
    /// (§14). The block runs as long as the shortest channel given; an event
    /// whose sample lies beyond the block acts at its last sample, and one that
    /// comes after an event of a later sample acts at that later sample. A
    /// block of no samples does nothing.
    pub fn render(&mut self, events: &[Event], outputs: &mut [&mut [f64]]) {
        let length = outputs.iter().map(|channel| channel.len()).min().unwrap_or(0);
        let mut next = 0;
        for index in 0..length {
            let last = index + 1 == length;
            let at = Instant::at(self.sample);
            if let Some(from) = self.sample.checked_sub(1) {
                self.advance(Instant::at(from));
            }
            while let Some(event) = events.get(next)
                && (last || usize::try_from(event.offset).is_ok_and(|offset| offset <= index))
            {
                self.act(&event.kind, at);
                next += 1;
            }
            let value = self.output(at);
            let channels = match self.configuration.channels {
                Channels::Stereo => 2,
                Channels::Mono => 1,
            };
            for channel in outputs.iter_mut().take(channels) {
                if let Some(slot) = channel.get_mut(index) {
                    *slot = value;
                }
            }
            self.close(at);
            self.sample = self.sample.saturating_add(1);
        }
    }

    /// Every voice that sounds, its fading ones included, integrates the
    /// sample that runs one engine sample from an instant.
    fn advance(&mut self, from: Instant) {
        let Self {
            configuration,
            tuning,
            parts,
            master_tune,
            ..
        } = self;
        for part in parts.iter_mut() {
            let PartState { controls, slots } = part;
            let context = controls.context(configuration, tuning, *master_tune);
            for slot in slots.iter_mut() {
                slot.voice.advance(from, &context);
            }
        }
    }

    /// An event acting at an instant.
    fn act(&mut self, kind: &Kind, at: Instant) {
        match *kind {
            Kind::NoteOn {
                channel,
                key,
                velocity,
                id,
            } => {
                let identity = Identity {
                    channel,
                    key: key.get(),
                    id,
                };
                for part in 0..self.parts.len() {
                    if self.parts[part].controls.compiled.takes(channel, key.get()) {
                        self.note_on(part, identity, velocity, at);
                    }
                }
            }
            Kind::NoteOff { channel, key, id } => {
                for part in 0..self.parts.len() {
                    for slot in 0..self.parts[part].slots.len() {
                        let held = &self.parts[part].slots[slot];
                        if held.playing()
                            && held.down
                            && held.identity.is_some_and(|note| note.named(channel, key.get(), id))
                        {
                            self.release(part, slot, at);
                        }
                    }
                }
            }
            Kind::Controller {
                channel,
                controller,
                position,
            } => {
                for part in &mut self.parts {
                    if part.controls.compiled.receives(channel)
                        && let Some(held) = part.controls.positions.get_mut(controller.index())
                    {
                        *held = position;
                        part.controls.settle();
                    }
                }
            }
            Kind::Bend { channel, position } => {
                for part in &mut self.parts {
                    if part.controls.compiled.receives(channel) {
                        part.controls.position = position;
                        part.controls.settle();
                    }
                }
            }
            Kind::Sustain { channel, on } => {
                for part in 0..self.parts.len() {
                    if self.parts[part].controls.compiled.receives(channel) {
                        self.sustain(part, on, at);
                    }
                }
            }
            Kind::PerNote { channel, key, id, keys } => {
                let Self {
                    configuration,
                    tuning,
                    parts,
                    master_tune,
                    ..
                } = self;
                for part in parts.iter_mut() {
                    let PartState { controls, slots } = part;
                    let context = controls.context(configuration, tuning, *master_tune);
                    for slot in slots.iter_mut() {
                        if slot.playing() && slot.identity.is_some_and(|note| note.named(channel, key.get(), id)) {
                            slot.voice.set_per_note(keys, &context);
                        }
                    }
                }
            }
            Kind::MasterTune { cents } => self.master_tune = finite(cents / 1200.0),
        }
    }

    /// A note's start in a part at an instant (MATH §13.2): a key pressed
    /// again while its voice sounds, of the same channel, whatever identifiers
    /// the host gives, restarts that voice under `voice.retrigger =
    /// same-voice`, the one that started last where there are several, and
    /// under `new-voice` releases it if its key was still down and takes a
    /// voice as any note does.
    fn note_on(&mut self, part: usize, identity: Identity, velocity: f64, at: Instant) {
        let again = {
            let slots = &self.parts[part].slots;
            (0..slots.len())
                .filter(|&k| {
                    slots[k].playing()
                        && slots[k]
                            .identity
                            .is_some_and(|note| note.channel == identity.channel && note.key == identity.key)
                })
                .max_by_key(|&k| slots[k].started)
        };
        if let Some(slot) = again {
            match self.configuration.retrigger {
                Retrigger::SameVoice => {
                    self.start(part, slot, identity, velocity, at);
                    return;
                }
                Retrigger::NewVoice => {
                    if self.parts[part].slots[slot].down {
                        self.release(part, slot, at);
                    }
                }
            }
        }
        if let Some(slot) = self.take(part, at) {
            self.start(part, slot, identity, velocity, at);
        }
    }

    /// Starts a note in a part's slot at an instant (MATH §4.5, §6.5): the
    /// part's LFO restarted under `lfo.scope = part`, and the voice keyed on as
    /// the part's next note, a voice taken while it sounds going on from where
    /// it is.
    fn start(&mut self, part: usize, slot: usize, identity: Identity, velocity: f64, at: Instant) {
        let fresh = !self.parts[part].slots[slot].playing();
        let Self {
            configuration,
            tuning,
            parts,
            master_tune,
            order,
            pool,
            ..
        } = self;
        let PartState { controls, slots } = &mut parts[part];
        if configuration.lfo_scope == LfoScope::Part {
            let lfo = &controls.compiled.patch.lfo;
            let (sync, delay) = (lfo.sync, lfo.delay);
            controls.lfo.key_on(at, sync, delay, configuration.sample_rate);
        }
        let nth = controls.notes;
        let context = controls.context(configuration, tuning, *master_tune);
        let held = &mut slots[slot];
        let note = Note {
            key: f64::from(identity.key),
            velocity,
            per_note: 0.0,
        };
        held.voice.key_on(at, note, Portamento::Still, nth, &context);
        held.identity = Some(identity);
        held.down = true;
        held.sustained = false;
        held.started = *order;
        held.fading = None;
        *order = order.saturating_add(1);
        controls.notes = nth.saturating_add(1);
        controls.last = Some(slot);
        if fresh {
            *pool = pool.saturating_add(1);
        }
    }

    /// The slot that a part's note takes at an instant (MATH §13.2, §13.5):
    /// beyond its note limit, the voice of its own that `voice.steal` picks;
    /// while the pool has a voice to spare, its first free slot after the one
    /// that it gave last; otherwise the voice of its own that `voice.steal`
    /// picks, or a free slot of its own where it picks one of another part's,
    /// which then fades out. None where nothing can be taken.
    fn take(&mut self, part: usize, at: Instant) -> Option<usize> {
        let state = &self.parts[part];
        let limited = state
            .controls
            .compiled
            .note_limit
            .is_some_and(|limit| state.playing() >= usize::try_from(limit.get()).unwrap_or(usize::MAX));
        if limited {
            return self.own(part, at);
        }
        if self.pool < self.configuration.voice_count
            && let Some(slot) = state.after_last(Slot::free)
        {
            return Some(slot);
        }
        if let Some(slot) = self.own(part, at) {
            return Some(slot);
        }
        let free = self.parts[part].after_last(Slot::free)?;
        let (other, taken) = self.others(part, at)?;
        self.parts[other].slots[taken].fading = Some(at);
        self.pool = self.pool.saturating_sub(1);
        Some(free)
    }

    /// The loudest carrier's level of the voice in a part's slot at an instant,
    /// as `voice.steal = quietest` compares them.
    fn loudest(&self, part: usize, slot: usize, at: Instant) -> f64 {
        let state = &self.parts[part];
        let context = state
            .controls
            .context(&self.configuration, &self.tuning, self.master_tune);
        state.slots[slot].voice.loudest(at, &context)
    }

    /// The candidate of a set of slots that a stealing policy picks at an
    /// instant, the slots given as pairs of part and slot in the order in
    /// which `dx7` takes them (MATH §13.2): under `dx7` the first whose key is
    /// up; under `oldest` the one whose note started first; under `quietest`
    /// the one whose loudest carrier is the quietest, the oldest breaking a
    /// tie; under `released-first`, of those whose key is up, the one released
    /// first, and the oldest where every key is held.
    fn pick(&self, candidates: &(impl Iterator<Item = (usize, usize)> + Clone), at: Instant) -> Option<(usize, usize)> {
        let slot = |(part, slot): (usize, usize)| &self.parts[part].slots[slot];
        let oldest = || candidates.clone().min_by_key(|&pair| slot(pair).started);
        match self.configuration.steal {
            Steal::Dx7 => candidates.clone().find(|&pair| slot(pair).up()),
            Steal::Oldest => oldest(),
            Steal::Quietest => candidates.clone().reduce(|best, pair| {
                let (level, least) = (self.loudest(pair.0, pair.1, at), self.loudest(best.0, best.1, at));
                let order = level
                    .total_cmp(&least)
                    .then(slot(pair).started.cmp(&slot(best).started));
                if order == core::cmp::Ordering::Less { pair } else { best }
            }),
            Steal::ReleasedFirst => candidates
                .clone()
                .filter(|&pair| slot(pair).up())
                .min_by_key(|&pair| slot(pair).released)
                .or_else(oldest),
        }
    }

    /// The slots of a part that the pool counts, in the order in which `dx7`
    /// takes them: from the one after the slot that it gave last, round to it.
    fn candidates(&self, part: usize) -> impl Iterator<Item = (usize, usize)> + Clone + '_ {
        let state = &self.parts[part];
        let count = state.slots.len();
        let first = state.controls.last.map_or(0, |last| last + 1);
        (0..count)
            .map(move |step| (part, (first + step) % count.max(1)))
            .filter(|&(part, slot)| self.parts[part].slots[slot].playing())
    }

    /// The voice of a part's own that its policy picks at an instant.
    fn own(&self, part: usize, at: Instant) -> Option<usize> {
        self.pick(&self.candidates(part), at).map(|(_, slot)| slot)
    }

    /// The voice of another part that a part's policy picks at an instant, the
    /// parts taken in the order of their indices.
    fn others(&self, part: usize, at: Instant) -> Option<(usize, usize)> {
        let others = (0..self.parts.len())
            .filter(move |&other| other != part)
            .flat_map(move |other| self.candidates(other));
        self.pick(&others, at)
    }

    /// A key's release in a part's slot at an instant (MATH §4.5): the voice's
    /// key-off, or where the sustain pedal is pressed its hold by the pedal.
    fn release(&mut self, part: usize, slot: usize, at: Instant) {
        let Self {
            configuration,
            tuning,
            parts,
            master_tune,
            order,
            ..
        } = self;
        let PartState { controls, slots } = &mut parts[part];
        let held = &mut slots[slot];
        held.down = false;
        held.released = *order;
        *order = order.saturating_add(1);
        if controls.sustain {
            held.sustained = true;
        } else {
            let context = controls.context(configuration, tuning, *master_tune);
            held.voice.key_off(at, &context);
        }
    }

    /// The sustain pedal pressed or released in a part at an instant (MATH
    /// §4.5): released, the notes that it held are keyed off.
    fn sustain(&mut self, part: usize, on: bool, at: Instant) {
        let Self {
            configuration,
            tuning,
            parts,
            master_tune,
            ..
        } = self;
        let PartState { controls, slots } = &mut parts[part];
        controls.sustain = on;
        if on {
            return;
        }
        let context = controls.context(configuration, tuning, *master_tune);
        for held in slots.iter_mut().filter(|slot| slot.sustained) {
            held.sustained = false;
            held.voice.key_off(at, &context);
        }
    }

    /// The output at an instant after its events (MATH §14):
    /// `output.gain` times each part's sum of its voices in the order of its
    /// slots, a fading voice's output scaled by its fade, added in the order
    /// of the parts' indices, each product and sum no further than binary64's
    /// largest finite number.
    fn output(&mut self, at: Instant) -> f64 {
        let Self {
            configuration,
            tuning,
            parts,
            master_tune,
            ..
        } = self;
        let glide = finite(configuration.level_glide_s * configuration.sample_rate);
        let mut output = 0.0;
        for part in parts.iter_mut() {
            let PartState { controls, slots } = part;
            let context = controls.context(configuration, tuning, *master_tune);
            let mut sum = 0.0;
            for slot in slots.iter_mut() {
                let mut voice = slot.voice.output(at, &context);
                if let Some(start) = slot.fading {
                    voice = finite(voice * faded(start, at, glide));
                }
                sum = finite(sum + voice);
            }
            output = finite(output + finite(configuration.output_gain * sum));
        }
        output
    }

    /// The tests at the end of an engine sample (MATH §13.3, §13.4): each
    /// voice whose note has ended, or whose fade has reached 0, ends there;
    /// and at every 64th sample counted from the engine's first, the voices'
    /// feedback states below 2⁻⁵⁰⁰ are set to 0.
    fn close(&mut self, at: Instant) {
        let Self {
            configuration,
            tuning,
            parts,
            master_tune,
            pool,
            sample,
            ..
        } = self;
        let glide = finite(configuration.level_glide_s * configuration.sample_rate);
        let flush = *sample % 64 == 0;
        for part in parts.iter_mut() {
            let PartState { controls, slots } = part;
            let context = controls.context(configuration, tuning, *master_tune);
            for slot in slots.iter_mut() {
                if let Some(start) = slot.fading {
                    if faded(start, at, glide) <= 0.0 || slot.voice.ended(at, &context) {
                        slot.voice.end(at);
                        slot.fading = None;
                        slot.sustained = false;
                    }
                } else if slot.playing() && slot.voice.ended(at, &context) {
                    slot.voice.end(at);
                    slot.sustained = false;
                    *pool = pool.saturating_sub(1);
                }
                if flush {
                    slot.voice.flush();
                }
            }
        }
    }
}
