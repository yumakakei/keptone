// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! A part as it starts (docs/MATH.md §13.5): the DX7's VOICE INIT, stated
//! again here as the steps of a single-voice dump from what firmware 1.8 holds
//! \[AJXS-ROM\], Keptone's own function data, and what a part takes.

use keptone_model::function::{Assign, Controller, Function, Polyphony, PortamentoMode};
use keptone_model::part::{KeyFilter, NoteShift, Part};
use keptone_model::patch::Patch;
use keptone_model::voice::{Parameter, Value, Voice};

/// The steps of VOICE INIT in the single-voice dump's order, OP6's first.
fn init_steps() -> Vec<u8> {
    let mut steps = Vec::new();
    for operator in (1..=6).rev() {
        // R1 to R4, L1 to L4.
        steps.extend([99, 99, 99, 99, 99, 99, 99, 0]);
        // The break point, the depths, the curves, the rate scaling and the
        // two sensitivities.
        steps.extend([0; 8]);
        // The output level: OP1's alone is heard.
        steps.push(if operator == 1 { 99 } else { 0 });
        // The ratio mode, coarse 1, fine 0, and no detune.
        steps.extend([0, 1, 0, 7]);
    }
    // The pitch envelope's rates and its levels at the middle.
    steps.extend([99, 99, 99, 99, 50, 50, 50, 50]);
    // Algorithm 1 (its step 0), no feedback, the oscillators' key sync on.
    steps.extend([0, 0, 1]);
    // The LFO: speed 35, no delay or depths, key sync on, the triangle.
    steps.extend([35, 0, 0, 0, 1, 0]);
    // The pitch sensitivity, and no transposition.
    steps.extend([3, 24]);
    steps.extend(*b"INIT VOICE");
    steps
}

#[test]
fn the_voice_init() {
    let voice = Voice::init();
    let steps = init_steps();
    assert_eq!(steps.len(), 155);
    for parameter in Parameter::all().take(155) {
        let step = steps[usize::from(parameter.number())];
        assert_eq!(voice.value(parameter), f64::from(step), "{parameter}");
    }
    assert!(voice.operators.iter().all(|operator| operator.on));
}

#[test]
fn the_function_data_a_part_starts_with() {
    let range = |step| Value::from_step(step).expect("a range");
    let none = Controller {
        range: range(0),
        assign: Assign::default(),
    };
    let wheel = Controller {
        range: range(99),
        assign: Assign {
            pitch: true,
            amplitude: false,
            eg_bias: false,
        },
    };
    let function = Function::initial();
    assert_eq!(
        (function.polyphony, function.portamento_mode, function.glissando),
        (Polyphony::Poly, PortamentoMode::Retain, false)
    );
    assert_eq!(
        (
            function.pitch_bend_range.get(),
            function.pitch_bend_step.get(),
            function.portamento_time.get()
        ),
        (2.0, 0.0, 0.0)
    );
    assert_eq!(
        [
            function.modulation_wheel,
            function.foot_controller,
            function.breath_controller,
            function.aftertouch
        ],
        [wheel, none, none, none]
    );
}

#[test]
fn a_part_as_it_starts() {
    for on in [true, false] {
        let part = Part::initial(on);
        assert_eq!(part.on, on);
        assert!(part.omni && !part.alternate);
        assert_eq!(
            (
                part.group.get(),
                part.channel.get(),
                part.lowest_key.get(),
                part.highest_key.get()
            ),
            (0, 0, 0, 127)
        );
        assert_eq!(
            (part.key_filter, part.note_shift, part.note_limit),
            (KeyFilter::All, NoteShift::NONE, None)
        );
        assert_eq!(part.function, Function::initial());
        assert_eq!(part.patch, Patch::init());
        assert_eq!(
            (part.patch.supplement.as_ref(), part.patch.fractional_scaling.as_ref()),
            (None, None)
        );
    }
}

#[test]
fn note_shifts() {
    // On the TX802's scale of −24 to +24, between its steps and beyond its
    // ends (docs/MATH.md §0.5).
    for semitones in [-24.0, -0.5, 0.0, 12.25, 24.0, -24.5, 24.5, 1e300, -f64::MAX] {
        assert_eq!(NoteShift::new(semitones).map(NoteShift::get), Some(semitones));
    }
    for semitones in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(NoteShift::new(semitones), None);
    }
    let zero = NoteShift::new(-0.0).expect("a shift");
    assert!(zero.get().is_sign_positive());
}
