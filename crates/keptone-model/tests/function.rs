// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The DX7's function data against \[DX7-MANUAL\] and \[DEXED-SYSEX\], stated
//! again here: parameters 64 to 77, mono/poly, the pitch bend's range and step,
//! the portamento's mode, glissando and time, and the range and the assignment
//! of the modulation wheel, the foot controller, the breath controller and the
//! aftertouch, an assignment's bits 0, 1 and 2 the pitch, the amplitude and
//! the EG bias. Each step lands in its field and is read back from it alone; a
//! step beyond its parameter's range is taken as the largest and reported; and
//! a value between steps is rounded as `syx.export_rounding` says.

mod common;

use common::FUNCTION_LARGEST;
use keptone_model::function::{Assign, Controller, Function, FunctionParameter, Polyphony, PortamentoMode};
use keptone_model::syx::FunctionStep;
use keptone_model::voice::{Rounding, Value};

/// The value of a step within its range.
fn value<const MAX: u8>(step: u8) -> Value<MAX> {
    Value::from_step(step).expect("a step within its range")
}

/// The function parameter of a number.
fn parameter(number: u8) -> FunctionParameter {
    FunctionParameter::new(number).expect("a function parameter")
}

/// Function data of every step 0.
fn zero() -> Function {
    let controller = Controller {
        range: value(0),
        assign: Assign::default(),
    };
    Function {
        polyphony: Polyphony::Poly,
        pitch_bend_range: value(0),
        pitch_bend_step: value(0),
        portamento_mode: PortamentoMode::Retain,
        glissando: false,
        portamento_time: value(0),
        modulation_wheel: controller,
        foot_controller: controller,
        breath_controller: controller,
        aftertouch: controller,
    }
}

/// Function data of a step for each parameter, 64 to 77 in order, each within
/// its range.
fn function_of(steps: [u8; 14]) -> Function {
    let mut function = zero();
    for (parameter, step) in FunctionParameter::all().zip(steps) {
        assert!(function.set_step(parameter, step), "{parameter} at {step}");
    }
    function
}

#[test]
fn parameters_and_their_names() {
    let numbers: Vec<u8> = FunctionParameter::all().map(FunctionParameter::number).collect();
    assert_eq!(numbers, (64..=77).collect::<Vec<u8>>());
    assert_eq!((FunctionParameter::FIRST, FunctionParameter::LAST), (64, 77));
    assert!(FunctionParameter::new(63).is_none() && FunctionParameter::new(78).is_none());
    let largest: Vec<u8> = FunctionParameter::all().map(FunctionParameter::largest).collect();
    assert_eq!(largest, FUNCTION_LARGEST);
    let names: Vec<String> = FunctionParameter::all()
        .map(|parameter| parameter.to_string())
        .collect();
    assert_eq!(
        names,
        [
            "mono/poly",
            "pitch bend range",
            "pitch bend step",
            "portamento mode",
            "glissando",
            "portamento time",
            "modulation wheel range",
            "modulation wheel assign",
            "foot controller range",
            "foot controller assign",
            "breath controller range",
            "breath controller assign",
            "aftertouch range",
            "aftertouch assign"
        ]
    );
}

#[test]
fn each_step_lands_in_its_field() {
    // Steps unlike each other where the ranges allow, so that a parameter that
    // set another's field would show.
    let function = function_of([1, 12, 3, 1, 1, 50, 99, 7, 10, 1, 20, 2, 30, 4]);
    let controller = |range: u8, pitch: bool, amplitude: bool, eg_bias: bool| Controller {
        range: value(range),
        assign: Assign {
            pitch,
            amplitude,
            eg_bias,
        },
    };
    let expected = Function {
        polyphony: Polyphony::Mono,
        pitch_bend_range: value(12),
        pitch_bend_step: value(3),
        portamento_mode: PortamentoMode::Follow,
        glissando: true,
        portamento_time: value(50),
        modulation_wheel: controller(99, true, true, true),
        foot_controller: controller(10, true, false, false),
        breath_controller: controller(20, false, true, false),
        aftertouch: controller(30, false, false, true),
    };
    assert_eq!(function, expected);
}

#[test]
fn every_step_and_beyond() {
    for parameter in FunctionParameter::all() {
        let largest = parameter.largest();
        for step in 0..=largest {
            let mut function = zero();
            assert!(function.set_step(parameter, step));
            assert_eq!(function.value(parameter), f64::from(step));
            assert!(function.is_step(parameter));
            assert_eq!(function.step(parameter, Rounding::Nearest), step);
            assert_eq!(function.step(parameter, Rounding::Floor), step);
            for other in FunctionParameter::all().filter(|&other| other != parameter) {
                assert_eq!(function.value(other), 0.0, "{parameter} at {step} moved {other}");
            }
        }
        for step in largest + 1..=u8::MAX {
            let mut function = zero();
            assert!(!function.set_step(parameter, step), "{parameter} at {step}");
            assert_eq!(function.step(parameter, Rounding::Nearest), largest);
        }
    }
}

#[test]
fn values_between_steps() {
    let mut function = zero();
    function.pitch_bend_range = Value::new(2.5).expect("a range");
    function.portamento_time = Value::new(3.5).expect("a time");
    function.foot_controller.range = Value::new(98.75).expect("a range");
    let (range, time, foot) = (parameter(65), parameter(69), parameter(72));
    assert_eq!(function.value(range), 2.5);
    assert!(!function.is_step(range) && !function.is_step(time) && !function.is_step(foot));
    assert!(function.is_step(parameter(66)));
    // Halfway to the even step, and down.
    let steps = |parameter| {
        (
            function.step(parameter, Rounding::Nearest),
            function.step(parameter, Rounding::Floor),
        )
    };
    assert_eq!(steps(range), (2, 2));
    assert_eq!(steps(time), (4, 3));
    assert_eq!(steps(foot), (99, 98));
    assert_eq!(FunctionStep::of(&function, time, Rounding::Nearest).step(), 4);
    assert_eq!(FunctionStep::of(&function, time, Rounding::Floor).step(), 3);
}

#[test]
fn choices_and_assignments() {
    assert_eq!(
        [0, 1, 2].map(Polyphony::from_step),
        [Some(Polyphony::Poly), Some(Polyphony::Mono), None]
    );
    assert_eq!([Polyphony::Poly.step(), Polyphony::Mono.step()], [0, 1]);
    assert_eq!(
        [0, 1, 2].map(PortamentoMode::from_step),
        [Some(PortamentoMode::Retain), Some(PortamentoMode::Follow), None]
    );
    assert_eq!([PortamentoMode::Retain.step(), PortamentoMode::Follow.step()], [0, 1]);
    for step in 0..8 {
        let assign = Assign::from_step(step).expect("an assignment");
        assert_eq!(
            (assign.pitch, assign.amplitude, assign.eg_bias),
            (step & 1 != 0, step & 2 != 0, step & 4 != 0)
        );
        assert_eq!(assign.step(), step);
    }
    assert!(Assign::from_step(8).is_none());
    assert_eq!(Assign::default().step(), 0);
}

#[test]
fn the_steps_a_parameter_change_carries() {
    let steps = [1, 12, 0, 1, 0, 50, 99, 7, 0, 1, 20, 2, 99, 4];
    let function = function_of(steps);
    for (parameter, step) in FunctionParameter::all().zip(steps) {
        let of = FunctionStep::of(&function, parameter, Rounding::Nearest);
        assert_eq!((of.parameter(), of.step()), (parameter, step));
        assert_eq!(FunctionStep::new(parameter, step), Some(of));
    }
    for parameter in FunctionParameter::all() {
        assert!(FunctionStep::new(parameter, parameter.largest() + 1).is_none());
    }
}
