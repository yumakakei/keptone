// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The settings of docs/MATH.md §17: every choice that changes behaviour, each
//! with its name, the values it takes and its default.
//!
//! [`SPECS`] lists them in the order of MATH's table, and a setting's place
//! there is its number, which its ID holds (docs/DESIGN.md §7.4, [`crate::id`]):
//! a setting to come takes the next place, and none is ever taken away.
//! [`Settings`] holds a value of each, each one its [`Spec`] takes, so that no
//! settings out of range can be made.

/// The bounds of a setting that is a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bounds {
    /// Above 0.
    Positive,
    /// 0 or above.
    NonNegative,
    /// Below 0.
    Negative,
    /// Above 0 and below 1.
    Open,
    /// 0 or above, and below 1.
    HalfOpen,
}

impl Bounds {
    /// Whether a number lies within them, finite as every setting is.
    #[must_use]
    pub fn contain(self, value: f64) -> bool {
        value.is_finite()
            && match self {
                Self::Positive => value > 0.0,
                Self::NonNegative => value >= 0.0,
                Self::Negative => value < 0.0,
                Self::Open => value > 0.0 && value < 1.0,
                Self::HalfOpen => (0.0..1.0).contains(&value),
            }
    }
}

/// What values a setting takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// On or off.
    Switch,
    /// One of these, by its index, each the name MATH gives it.
    Choice(&'static [&'static str]),
    /// A binary64 within these bounds.
    Number(Bounds),
    /// A binary64 within these bounds, or the default that MATH gives as a
    /// formula of other quantities, [`Value::Automatic`].
    NumberOrAutomatic(Bounds),
    /// An integer of at least this.
    Count(u64),
    /// A power of two: 1, 2, 4 and on.
    PowerOfTwo,
    /// Any integer of 64 bits.
    Seed,
}

/// A setting's value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// A switch's.
    Switch(bool),
    /// The index of a choice's name.
    Choice(u8),
    /// A number.
    Number(f64),
    /// The default that MATH gives as a formula.
    Automatic,
    /// A count, a power of two or a seed.
    Integer(u64),
}

/// A setting: its name, the values it takes and its default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spec {
    /// Its name, as MATH §17 gives it.
    pub name: &'static str,
    /// What values it takes.
    pub kind: Kind,
    /// Its default.
    pub default: Value,
}

impl Spec {
    /// Whether it takes a value.
    #[must_use]
    pub fn takes(&self, value: Value) -> bool {
        match (self.kind, value) {
            (Kind::Switch, Value::Switch(_))
            | (Kind::NumberOrAutomatic(_), Value::Automatic)
            | (Kind::Seed, Value::Integer(_)) => true,
            (Kind::Choice(names), Value::Choice(index)) => usize::from(index) < names.len(),
            (Kind::Number(bounds) | Kind::NumberOrAutomatic(bounds), Value::Number(number)) => bounds.contain(number),
            (Kind::Count(least), Value::Integer(count)) => count >= least,
            (Kind::PowerOfTwo, Value::Integer(integer)) => integer.is_power_of_two(),
            _ => false,
        }
    }
}

/// A switch, its default given.
const fn switch(name: &'static str, default: bool) -> Spec {
    Spec {
        name,
        kind: Kind::Switch,
        default: Value::Switch(default),
    }
}

/// A choice of names, its default the first.
const fn choice(name: &'static str, names: &'static [&'static str]) -> Spec {
    Spec {
        name,
        kind: Kind::Choice(names),
        default: Value::Choice(0),
    }
}

/// A number within bounds, its default given.
const fn number(name: &'static str, bounds: Bounds, default: f64) -> Spec {
    Spec {
        name,
        kind: Kind::Number(bounds),
        default: Value::Number(default),
    }
}

/// A count of at least `least`, its default given.
const fn count(name: &'static str, least: u64, default: u64) -> Spec {
    Spec {
        name,
        kind: Kind::Count(least),
        default: Value::Integer(default),
    }
}

/// How many settings there are.
pub const COUNT: usize = 57;

/// Every setting, in the order of MATH §17's table; a setting's place is its
/// number.
pub const SPECS: [Spec; COUNT] = [
    number("tuning.a4_hz", Bounds::Positive, 440.0),
    choice("tuning.source", &["12-tet", "scala", "mts-esp"]),
    choice("osc.fixed_detune", &["msfa", "symmetric", "none"]),
    choice("osc.phase_accumulator", &["plain", "compensated"]),
    choice("osc.free_phase_start", &["continue", "zero", "random"]),
    choice("level.curve_interpolation", &["linear", "step"]),
    switch("scaling.exact", false),
    switch("velocity.quantize_32", false),
    number("eg.clock_hz", Bounds::Positive, 49096.0),
    choice("eg.rate_law", &["dx7", "exponential"]),
    switch("eg.rate_quantize", true),
    switch("eg.attack_jump", true),
    choice("eg.attack_law", &["continuous", "stepped"]),
    choice("eg.static_segments", &["dx7", "none"]),
    choice("eg.zero_level", &["silence", "dx7"]),
    number("eg.end_db", Bounds::Negative, -200.0),
    number("param.level_glide_s", Bounds::NonNegative, 0.002),
    choice("lfo.scope", &["part", "voice"]),
    Spec {
        name: "lfo.seed",
        kind: Kind::Seed,
        default: Value::Integer(0),
    },
    choice("lfo.sh_source", &["splitmix", "dx7-lcg"]),
    number("am.max_steps", Bounds::NonNegative, 2048.0),
    choice("graph.algorithm_scope", &["patch", "voice"]),
    count("graph.operators", 1, 6),
    choice("feedback.model", &["relaxation", "dx7-discrete", "clamp"]),
    number("feedback.tau_s", Bounds::Positive, 1.5 / 49096.0),
    number("feedback.beta_clamp", Bounds::Open, 0.999),
    count("feedback.max_iterations", 1, 16),
    number("feedback.series_beta_max", Bounds::HalfOpen, 0.8),
    // 2⁻⁵³, half of binary64's epsilon of 2⁻⁵².
    number("feedback.series_epsilon", Bounds::Positive, f64::EPSILON / 2.0),
    number("alias.epsilon", Bounds::Negative, -180.0),
    Spec {
        name: "adaa.order",
        kind: Kind::Choice(&["0", "1", "2"]),
        default: Value::Choice(2),
    },
    // 2⁻¹⁰.
    number("adaa.tau2", Bounds::Positive, 1.0 / 1024.0),
    choice("adaa.order2_kernel", &["bilbao", "triangular"]),
    switch("adaa.modulators", true),
    choice("adaa.interp", &["linear", "cubic"]),
    choice("adaa.droop_eq", &["auto", "on", "off"]),
    Spec {
        name: "adaa.droop_eq_hz",
        kind: Kind::NumberOrAutomatic(Bounds::Positive),
        default: Value::Automatic,
    },
    Spec {
        name: "engine.oversampling",
        kind: Kind::PowerOfTwo,
        default: Value::Integer(2),
    },
    choice("engine.kernels", &["auto", "portable"]),
    choice("resample.filter", &["iir", "fir"]),
    number("resample.stopband_db", Bounds::Positive, 150.0),
    count("voice.count", 1, 16),
    choice("voice.steal", &["dx7", "oldest", "quietest", "released-first"]),
    choice("voice.retrigger", &["same-voice", "new-voice"]),
    number("engine.idle_s", Bounds::NonNegative, 0.5),
    count("part.count", 1, 2),
    choice("part.function_source", &["patch", "part"]),
    number("output.gain", Bounds::NonNegative, 0.5),
    choice("output.carrier_normalization", &["none", "count", "sqrt-count"]),
    choice("output.channels", &["stereo", "mono"]),
    switch("syx.accept_bad_checksum", true),
    choice("syx.out_of_range", &["clamp", "reject"]),
    choice("syx.export_rounding", &["nearest", "floor"]),
    choice("syx.performance_sensitivity", &["linear", "firmware"]),
    count("json.max_depth", 0, 64),
    choice("tuning.transpose_octave", &["keys", "period"]),
    choice("scaling.key", &["key", "onset-pitch", "pitch"]),
];

/// The number of a setting of a name, if one has it.
#[must_use]
pub fn number_of(name: &str) -> Option<u16> {
    SPECS
        .iter()
        .position(|spec| spec.name == name)
        .and_then(|place| u16::try_from(place).ok())
}

/// A value of each setting.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The values, in the order of [`SPECS`].
    values: [Value; COUNT],
}

impl Settings {
    /// Every setting at its default.
    #[must_use]
    pub fn new() -> Self {
        Self {
            values: SPECS.map(|spec| spec.default),
        }
    }

    /// The value of the setting of a number, if one has it.
    #[must_use]
    pub fn get(&self, number: u16) -> Option<Value> {
        self.values.get(usize::from(number)).copied()
    }

    /// Sets the setting of a number to a value, if one has the number and it
    /// takes the value, and says whether it did; a number setting takes −0
    /// as 0, so that equal values have equal bits.
    pub fn set(&mut self, number: u16, value: Value) -> bool {
        let place = usize::from(number);
        let takes = SPECS.get(place).is_some_and(|spec| spec.takes(value));
        if takes {
            self.values[place] = match value {
                Value::Number(number) => Value::Number(number + 0.0),
                other => other,
            };
        }
        takes
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::new()
    }
}
