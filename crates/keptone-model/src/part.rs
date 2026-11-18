// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! A part, what Keptone plays: one patch, on the notes it takes (docs/MATH.md
//! §13.5, docs/DESIGN.md §6.4).
//!
//! A [`Part`] holds what a part has of its own: whether it is on, the channels
//! and the keys it takes, its note shift, its note limit and whether it
//! alternates, its function data and its patch. Every value is a range type,
//! as a voice's are ([`crate::voice`]), and [`crate::id`] numbers and names
//! the parameters. What else a part is to have, its detune, volume and pan
//! among it, comes with the milestones that write its laws (docs/DESIGN.md
//! §16).

use std::num::NonZeroU32;

use crate::dx7ii::Index;
use crate::function::Function;
use crate::patch::Patch;

/// Which of the keys in a part's range it takes, by its step: every key, or
/// every other one (MATH §13.5) \[DX7II-MANUAL\].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum KeyFilter {
    /// 0: every key. The default.
    #[default]
    All,
    /// 1: the odd keys.
    Odd,
    /// 2: the even keys.
    Even,
}

impl KeyFilter {
    /// The filter of a step, if the step is 0 to 2.
    #[must_use]
    pub const fn from_step(step: u8) -> Option<Self> {
        match step {
            0 => Some(Self::All),
            1 => Some(Self::Odd),
            2 => Some(Self::Even),
            _ => None,
        }
    }

    /// Its step.
    #[must_use]
    pub const fn step(self) -> u8 {
        match self {
            Self::All => 0,
            Self::Odd => 1,
            Self::Even => 2,
        }
    }
}

/// A part's note shift, in semitones from −24 to +24, which adds to its
/// patch's transpose (MATH §13.5) \[TX802-MANUAL\].
///
/// A shift may lie between the semitones, as a host's automation leaves it. It
/// is never NaN, never beyond 24 either way, and never −0, which
/// [`NoteShift::new`] takes as 0, so that equal shifts have equal bits.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct NoteShift(f64);

impl NoteShift {
    /// No shift.
    pub const NONE: Self = Self(0.0);

    /// The shift, if it is a number from −24 to +24.
    #[must_use]
    pub fn new(semitones: f64) -> Option<Self> {
        // Adding +0 takes −0 to +0 and leaves every other number as it is.
        (-24.0..=24.0).contains(&semitones).then_some(Self(semitones + 0.0))
    }

    /// The shift, in semitones.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// A part.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// Whether it takes notes at all, as a tone generator of the TX802 that is
    /// switched on does \[TX802-MANUAL\].
    pub on: bool,
    /// Whether it receives every channel, whatever [`Part::group`] and
    /// [`Part::channel`] say.
    pub omni: bool,
    /// The group of MIDI 2.0 whose channel it receives, 0 for the first of 16;
    /// MIDI 1.0 has the first alone.
    pub group: Index<15>,
    /// The channel it receives, 0 for the first of 16.
    pub channel: Index<15>,
    /// The lowest key it takes, by MIDI's numbers of keys.
    pub lowest_key: Index<127>,
    /// The highest key it takes.
    pub highest_key: Index<127>,
    /// Which keys in its range it takes.
    pub key_filter: KeyFilter,
    /// How far it shifts the keys it takes.
    pub note_shift: NoteShift,
    /// At most how many voices it holds, if it has a limit.
    pub note_limit: Option<NonZeroU32>,
    /// Whether it takes its channel's notes in turn with the parts beside it
    /// that alternate as well (MATH §13.5) \[TX802-MANUAL\].
    pub alternate: bool,
    /// Its function data, whose place its patch's supplement takes where it
    /// has one and `part.function_source` says so (MATH §13.5).
    pub function: Function,
    /// Its patch.
    pub patch: Patch,
}

impl Part {
    /// A part as it starts (MATH §13.5), on or off as `on` says: it receives
    /// every channel, takes every key without a shift, a filter or a note
    /// limit, alternates with none, and plays the DX7's VOICE INIT
    /// ([`Patch::init`]) with the function data of [`Function::initial`].
    #[must_use]
    pub fn initial(on: bool) -> Self {
        Self {
            on,
            omni: true,
            group: Index::saturating(0),
            channel: Index::saturating(0),
            lowest_key: Index::saturating(0),
            highest_key: Index::saturating(127),
            key_filter: KeyFilter::All,
            note_shift: NoteShift::NONE,
            note_limit: None,
            alternate: false,
            function: Function::initial(),
            patch: Patch::init(),
        }
    }
}
