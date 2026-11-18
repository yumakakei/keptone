// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! A patch: what a part plays (docs/DESIGN.md §6.4), a DX7 voice with the
//! DX7II's supplement and fractional scaling where it has them.
//!
//! A patch made of a DX7's voice has neither, and plays as a DX7 voice; one
//! made of a DX7II's has either or both. What they do to the sound is written
//! in docs/MATH.md with the milestones that give it (docs/DESIGN.md §16);
//! until then a patch holds them and they are not heard.

use crate::dx7ii::FractionalScaling;
use crate::supplement::Supplement;
use crate::voice::Voice;

/// A patch: a voice, and the supplement and the fractional scaling it has.
#[derive(Debug, Clone, PartialEq)]
pub struct Patch {
    /// The voice.
    pub voice: Voice,
    /// The voice's supplement, if it has one.
    pub supplement: Option<Supplement>,
    /// The voice's fractional scaling, if it has one.
    pub fractional_scaling: Option<FractionalScaling>,
}

impl Patch {
    /// The patch a part starts with (MATH §13.5): the DX7's VOICE INIT
    /// ([`Voice::init`]), with no supplement and no fractional scaling.
    #[must_use]
    pub fn init() -> Self {
        Self {
            voice: Voice::init(),
            supplement: None,
            fractional_scaling: None,
        }
    }
}
