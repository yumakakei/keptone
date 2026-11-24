// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The LFO (docs/MATH.md §6): its frequency from its speed, its phase in closed
//! form from its last start, its waves, the sample-and-hold values that it
//! draws from the random source at each wrap of its phase and each reset, and
//! the fade of its delay, each a function of continuous time.
//!
//! Each function documents its error against the law's exact value at its
//! binary64 inputs; tests/lfo.rs measures each against exact values
//! (`scripts/lfo_references.py`). A part's LFO and, under `lfo.scope = voice`,
//! each voice's are values of [`Lfo`], which the part or the voice drives at
//! its key-ons and changes (MATH §6.5).

use keptone_math::eft::two_product;
use keptone_math::{exp2, log2, sin2pi};
use keptone_model::voice::Wave;

use crate::binary64::{finite, scaled};
use crate::random;
use crate::steps::{lerp, scale_value};
use crate::time::Instant;

/// The rate F of the firmware's timer, 9.4265 MHz/(8·3165), correctly rounded
/// (MATH §5, §6.1).
const TICK: f64 = f64::from_bits(0x4077_44b6_cca3_6315);

/// 2⁵³ turns a sample, from which the phase stands still (MATH §6.1).
const STILL: f64 = 9_007_199_254_740_992.0;

/// The advance of the phase in turns a tick at a whole speed S of 0 or more
/// (MATH §6.1) \[AJXS-ROM `PATCH_ACTIVATE_SCALE_LFO_SPEED`\]: the firmware's
/// increment of its 16-bit phase over 2¹⁶, the increment 11·max(1, a) for
/// a = ⌊165S/64⌋ below 160 and a·(11 + ⌊(a − 160)/4⌋) from 160 on; +∞ where it
/// lies beyond binary64's largest finite number, the frequency then lying
/// beyond it too.
///
/// a takes its floor of an exact value ([`scale_value`]): it is exact below
/// 2⁵³ and within 2u of itself beyond, where ⌊(a − 160)/4⌋ is within 7u of
/// itself, its argument within 3u of the exact one and the floor of a value
/// from 2⁵¹ on at most 2⁻⁵¹ of that value from the exact one's floor, and 11
/// added within u more. So the advance is exact where the increment is below
/// 2⁵³, S below 7.36·10⁷; within u of itself, the product's rounding, where a
/// is below 2⁵³, S below 3.49·10¹⁵; and within 11u beyond.
fn advance(speed: f64) -> f64 {
    let a = scale_value(speed);
    if a < 160.0 {
        11.0 * a.max(1.0) / 65536.0
    } else {
        a / 65536.0 * (11.0 + ((a - 160.0) / 4.0).floor())
    }
}

/// The frequency f of MATH §6.1 at a speed S, in hertz: F times the advance of
/// the phase a tick at whole speeds, a period of 16.0 s at 0 and 49.25 Hz at
/// 99, and between them in log f (MATH §0.6); below 0 halving at each step,
/// f(0)·2^S, and above 99 the law as written. A frequency beyond binary64's
/// largest finite number, from S = 1.38·10¹⁵⁵ on, is taken at it.
///
/// Let e be the advance's error: 0 where the increments are below 2⁵³, S below
/// 7.36·10⁷; u where a is below 2⁵³, S below 3.49·10¹⁵; and 11u beyond. At a
/// whole speed F, within u of the timer's rate, and the advance multiply with
/// one rounding: within 2u + e relatively. Between whole speeds n and n + 1,
/// f = f(n)·2^(tL), t the speed's part and L the log₂ of the advances'
/// quotient, at most log₂ 2.5, from S = 1 to 2: the quotient is within 2e + u
/// of itself; its log₂ within 0.81 ulp, 1.62u|L|, of the log₂ of that, which
/// lies within (2e + u)/ln 2 of L; t times it within u·t|L|; 2 to the exponent
/// so found within ln 2 times the exponent's error of 2^(tL), and exp2 within
/// 0.93 ulp, 1.86u, of that; and the advance below within e, two products
/// within u each and F within u: within u(4.86 + t + 1.82t|L|) + e(1 + 2t)
/// relatively. Below 0, F·11/2¹⁶ within 2u, 2^S within 1.86u and their
/// product's rounding: within 4.86u relatively, and 2⁻¹⁰⁷⁴ absolutely where f
/// is subnormal, S below −1018.
#[must_use]
pub fn frequency(speed: f64) -> f64 {
    if speed < 0.0 {
        return TICK * advance(0.0) * exp2(speed);
    }
    let whole = speed.floor();
    let below = advance(whole);
    let part = speed - whole;
    let per_tick = if part > 0.0 {
        below * exp2(part * log2(advance(whole + 1.0) / below))
    } else {
        below
    };
    finite(TICK * per_tick)
}

/// ℓ of MATH §6.2 of a wave at a phase φ in [0, 1), the sample-and-hold value
/// held being `held`: triangle |4φ − 2| − 1, +1 at 0 and −1 at ½; saw down
/// 1 − 2φ; saw up 2φ − 1; square −1 below ½ and +1 from ½ on; sine −sin 2πφ;
/// sample and hold the value held. The phase counts from where the firmware's
/// key sync puts it, half a turn into the firmware's own (VERIFICATION V11).
///
/// The triangle within u absolutely: 4φ − 2 rounds only below φ = 1/4, within
/// u, Sterbenz's lemma making it exact from there on, and the subtraction of 1
/// only where it leaves a result below −1/2, within u/2. The saws within u/2,
/// rounding only below φ = 1/4; the square and the held value exact; the sine
/// within sin2pi's error, a relative 1.46u, or 0.96 ulp where it is subnormal.
#[must_use]
pub fn shape(wave: Wave, turns: f64, held: f64) -> f64 {
    match wave {
        Wave::Triangle => (4.0 * turns - 2.0).abs() - 1.0,
        Wave::SawDown => 1.0 - 2.0 * turns,
        Wave::SawUp => 2.0 * turns - 1.0,
        Wave::Square => {
            if turns < 0.5 {
                -1.0
            } else {
                1.0
            }
        }
        Wave::Sine => -sin2pi(turns),
        Wave::SampleAndHold => held,
    }
}

/// t₁ and t₂ of MATH §6.4 in seconds at a whole DELAY of 1 or more:
/// a = 99 − DELAY, Euclid's a mod 16 and ⌊a/16⌋, the increment
/// i = (16 + a mod 16)·2^(2 + ⌊a/16⌋) of the counter at every tick,
/// t₁ = 2¹⁶/(i·F) and t₂ = 256/(max(1, ⌊i/256⌋)·F) \[AJXS-ROM
/// `PATCH_ACTIVATE_SCALE_LFO_DELAY`\], t₁ taken at binary64's largest finite
/// number where it lies beyond it (MATH §0.5), from DELAY 16 461 on.
///
/// a and the increment are exact wherever t₁ is not so taken, and each time
/// within 3u of itself, the constant's, the product's and the quotient's
/// roundings. Where t₁ is so taken, the increment is below 256, and t₂ is
/// 256/F, whatever a's rounding.
fn stages_at(delay: f64) -> (f64, f64) {
    let a = 99.0 - delay;
    let group = (a / 16.0).floor();
    let rest = a - 16.0 * group;
    let increment = scaled(16.0 + rest, 2.0 + group);
    let wait = finite(65536.0 / (increment * TICK));
    let rise = 256.0 / ((increment / 256.0).floor().max(1.0) * TICK);
    (wait, rise)
}

/// The two stages of MATH §6.4's delay at a DELAY, t₁ with no depth and t₂ in
/// which the depth rises to 1, in seconds: none at or below 0, where there is
/// no delay, as the firmware restarts none of a patch whose DELAY is 0; between
/// whole DELAYs linear, DELAY 0 taken as no delay (MATH §0.6); beyond 99 the
/// law as written, t₁ doubling every 16 steps and t₂ staying at 256/F. DELAY 99
/// gives 2.75 s and 0.69 s, and DELAY 1 38 ms each.
///
/// At a whole DELAY within 3u of each. Between them, of the steps' values,
/// which rise with DELAY, the difference and the fused step round within u of
/// the result each: within 5u relatively, and 2⁻¹⁰⁷⁵ absolutely where a
/// result is subnormal, DELAY below 5.8·10⁻³⁰⁷.
#[must_use]
pub fn stages(delay: f64) -> Option<(f64, f64)> {
    if delay <= 0.0 {
        return None;
    }
    let whole = delay.floor();
    let part = delay - whole;
    let below = if whole <= 0.0 { (0.0, 0.0) } else { stages_at(whole) };
    if part > 0.0 {
        let above = stages_at(whole + 1.0);
        Some((lerp(part, below.0, above.0), lerp(part, below.1, above.1)))
    } else {
        Some(below)
    }
}

/// Where the fade of an LFO's delay is (MATH §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Stage 1: no depth yet.
    Waiting,
    /// Stage 2: the depth rising linearly to 1.
    Rising,
    /// The delay over, or none: the full depth.
    Full,
}

/// The fade w of an LFO's delay (MATH §6.4): its stage at an instant, the
/// share of the stage that had passed there, and the stages' lengths in
/// samples.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Fade {
    /// The stage at `start`.
    stage: Stage,
    /// The instant from which it counts.
    start: Instant,
    /// The share of its stage that had passed at `start`, from 0 up to 1.
    passed: f64,
    /// t₁ in samples.
    wait: f64,
    /// t₂ in samples.
    rise: f64,
}

impl Fade {
    /// The full depth from an instant on: no delay.
    const fn full(at: Instant) -> Self {
        Self {
            stage: Stage::Full,
            start: at,
            passed: 0.0,
            wait: 0.0,
            rise: 0.0,
        }
    }

    /// A delay of a DELAY from an instant on, its stage and the share of it that
    /// has passed given, at the engine's rate.
    fn of(at: Instant, delay: f64, sample_rate: f64, stage: Stage, passed: f64) -> Self {
        match stages(delay) {
            Some((wait, rise)) if stage != Stage::Full => Self {
                stage,
                start: at,
                passed,
                wait: finite(wait * sample_rate),
                rise: finite(rise * sample_rate),
            },
            _ => Self::full(at),
        }
    }

    /// The stage at an instant and the share of it that has passed: the stage
    /// at the start, or the rise after it once the wait's share reaches 1, or
    /// the full depth once the rise's does. A stage that rounds to no length
    /// at all is over at once.
    fn at(&self, at: Instant) -> (Stage, f64) {
        let samples = at.samples_since(self.start).max(0.0);
        match self.stage {
            Stage::Full => (Stage::Full, 1.0),
            Stage::Waiting => {
                let left = (1.0 - self.passed) * self.wait;
                if samples < left {
                    (Stage::Waiting, self.passed + samples / self.wait)
                } else {
                    Self::rising((samples - left) / self.rise)
                }
            }
            Stage::Rising => Self::rising(self.passed + samples / self.rise),
        }
    }

    /// The rise with a share passed, or the full depth from 1 on, and where a
    /// rise of no length leaves no share.
    fn rising(passed: f64) -> (Stage, f64) {
        if passed < 1.0 {
            (Stage::Rising, passed)
        } else {
            (Stage::Full, 1.0)
        }
    }

    /// w at an instant: 0 while it waits, the share of the rise while it rises,
    /// and 1 after.
    fn depth(&self, at: Instant) -> f64 {
        match self.at(at) {
            (Stage::Waiting, _) => 0.0,
            (Stage::Rising, passed) => passed,
            (Stage::Full, _) => 1.0,
        }
    }

    /// Its form from an instant on: as [`Fade::at`] takes its stages, with the
    /// samples from the instant to their ends.
    fn fading(&self, at: Instant) -> Fading {
        const FULL: Fading = Fading {
            depth: 1.0,
            rise: None,
            full: None,
            length: 0.0,
        };
        let samples = at.samples_since(self.start).max(0.0);
        match self.stage {
            Stage::Full => FULL,
            Stage::Waiting => {
                let left = (1.0 - self.passed) * self.wait;
                if samples < left {
                    Fading {
                        depth: 0.0,
                        rise: Some(left - samples),
                        full: Some(finite(left + self.rise) - samples),
                        length: self.rise,
                    }
                } else {
                    let passed = (samples - left) / self.rise;
                    if passed < 1.0 {
                        Fading {
                            depth: passed,
                            rise: None,
                            full: Some(finite(left + self.rise) - samples),
                            length: self.rise,
                        }
                    } else {
                        FULL
                    }
                }
            }
            Stage::Rising => {
                let passed = self.passed + samples / self.rise;
                if passed < 1.0 {
                    Fading {
                        depth: passed,
                        rise: None,
                        full: Some((1.0 - self.passed) * self.rise - samples),
                        length: self.rise,
                    }
                } else {
                    FULL
                }
            }
        }
    }
}

/// The fade w of an LFO's delay from an instant on (MATH §6.4), as a voice's
/// pitch takes it within a sample (§2.5): w there, and the samples from there
/// to where its rise starts and to where it is full, over which it rises
/// linearly from 0 to 1. The instants are those at which the LFO's own fade
/// changes its stage, within the rounding of their differences from the
/// instant, u of them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fading {
    /// w at the instant.
    pub depth: f64,
    /// The samples to the rise's start, none where it rises or is full.
    pub rise: Option<f64>,
    /// The samples to where it is full, none where it is.
    pub full: Option<f64>,
    /// The rise's length in samples, t₂·fₑ.
    pub length: f64,
}

/// The LFO's phase from its last start (MATH §6.1): the turns there, from 0 up
/// to 1, its advance a sample, and the sample-and-hold values that it had
/// drawn there.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Phase {
    /// The instant of its last start.
    start: Instant,
    /// Its turns there.
    turns: f64,
    /// Its advance a sample, f/fₑ.
    step: f64,
    /// The values drawn there, the first, at the engine's first sample, counted
    /// as 0, modulo 2⁶⁴.
    draws: u64,
}

/// The wraps ⌊high⌋ + carry of a phase modulo 2⁶⁴, as the random source takes
/// its indices (MATH §6.3), exactly: both are whole binary64 below 2¹¹⁷ in
/// magnitude, the step lying below 2⁵³ and the samples below 2⁶⁴, which i128
/// holds.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "whole binary64 below 2¹¹⁷ convert exactly, and the sum is taken modulo 2⁶⁴"
)]
fn wraps(whole: f64, carry: f64) -> u64 {
    ((whole as i128) + (carry as i128)) as u64
}

impl Phase {
    /// The turns at an instant at or after its start, from 0 up to 1, and the
    /// wraps since its start modulo 2⁶⁴: φ₀ + step·Δ, Δ the samples since the
    /// start, the product formed exactly as the sum of two binary64 by a fused
    /// multiply–add and reduced to its fraction. Where the step reaches 2⁵³
    /// turns, the phase stands still, binary64 holding no fraction of a turn
    /// there, and so do its wraps.
    fn at(&self, at: Instant) -> (f64, u64) {
        if self.step >= STILL {
            return (self.turns, 0);
        }
        let samples = at.samples_since(self.start).max(0.0);
        let (high, low) = two_product(self.step, samples);
        let whole = high.floor();
        let part = (high - whole) + (low + self.turns);
        let carry = part.floor();
        let turns = part - carry;
        if turns < 1.0 {
            (turns, wraps(whole, carry))
        } else {
            // A part just below 0 leaves a fraction that rounds up to 1.
            (0.0, wraps(whole, carry + 1.0))
        }
    }
}

/// An LFO (MATH §6): its phase, the values that it draws, and its delay's fade.
///
/// Its phase at an instant is within 3u + 2u²·step·Δ turns of φ₀ + step·Δ at
/// the binary64 step and Δ: the product is exact, and its low part, at most
/// 1/2 where step·Δ is below 2⁵³ and at most u·step·Δ beyond, sums with φ₀
/// within u(1 + |low|) and that with the product's fraction within
/// u(2 + |low|) more. The step is f/fₑ within u of itself beside the
/// frequency's error ([`frequency`]), and Δ within u(|Δ| + 1/2) of the exact
/// count: the phase lies within that relative error of step·Δ more of its
/// exact value, beside its start's error, which a change of speed carries
/// over. The count of its draws is exact modulo 2⁶⁴, as the random source takes
/// its indices, wherever its phase lies farther from a wrap than its error.
///
/// Its fade's share of a stage, p₀ + Δ/L from the share p₀ at the stage's
/// start, 0 at a key-on and carried over by a change of DELAY, L the stage's
/// length in samples within ε of itself ([`stages`]'s error and u for the
/// product by the engine's rate), is within e₀ + (Δ/L)(ε + u) + u of its exact
/// value, e₀ being p₀'s error; and the share of the rise after the wait,
/// p = (Δ − (1 − p₀)L₁)/L₂, within (e₀ + (1 − p₀)(ε₁ + 2u))L₁/L₂ + p(ε₂ + 2u).
/// Its depth w, 0, p or 1, is so within the rise's error of its exact value,
/// but where its time lies within these errors of a boundary of its stages,
/// where the stage may be the neighbouring one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lfo {
    /// Its phase.
    phase: Phase,
    /// Its delay's fade.
    fade: Fade,
    /// The seed of its stream of sample-and-hold values (MATH §6.3).
    seed: u64,
    /// The index before its first value: 0 for a part's LFO, 2³²m for the
    /// LFO of the part's m-th note.
    base: u64,
}

impl Lfo {
    /// An LFO from an instant on, the engine's first sample for a part's or a
    /// voice's key-on for a voice's: at phase 0, its first value drawn, at a
    /// speed, the full depth until a key-on restarts its delay; its values
    /// from a stream's seed, after an index (MATH §6.3).
    #[must_use]
    pub fn new(at: Instant, speed: f64, sample_rate: f64, stream: u64, base: u64) -> Self {
        Self {
            phase: Phase {
                start: at,
                turns: 0.0,
                step: frequency(speed) / sample_rate,
                draws: 0,
            },
            fade: Fade::full(at),
            seed: stream,
            base,
        }
    }

    /// A key-on that restarts it (MATH §6.5): its delay restarts at a DELAY
    /// above 0, its fade from 0; and with LFO SYNC its phase restarts at 0,
    /// where it draws a new value.
    pub fn key_on(&mut self, at: Instant, sync: bool, delay: f64, sample_rate: f64) {
        self.fade = Fade::of(at, delay, sample_rate, Stage::Waiting, 0.0);
        if sync {
            let (_, wraps) = self.phase.at(at);
            self.phase = Phase {
                start: at,
                turns: 0.0,
                step: self.phase.step,
                draws: self.phase.draws.wrapping_add(wraps).wrapping_add(1),
            };
        }
    }

    /// The LFO of a voice under `lfo.scope = voice` from the key-on of the
    /// part's m-th note (MATH §6.5): its delay starting there; with LFO SYNC
    /// its phase starting at 0 there, and without it the part's phase, which no
    /// key-on resets, the same closed form from the same start, at the part's
    /// speed either way; and its sample-and-hold values from the stream of the
    /// part's voices, its c-th value since the key-on, from 0, of index
    /// 2³²m + c + 1 (MATH §6.3).
    #[must_use]
    pub fn of_voice(
        part: &Self,
        at: Instant,
        sync: bool,
        delay: f64,
        sample_rate: f64,
        stream: u64,
        note: u64,
    ) -> Self {
        let base = note << 32;
        let (phase, base) = if sync {
            let phase = Phase {
                start: at,
                turns: 0.0,
                step: part.phase.step,
                draws: 0,
            };
            (phase, base)
        } else {
            (part.phase, base.wrapping_sub(part.draws(at)))
        };
        Self {
            phase,
            fade: Fade::of(at, delay, sample_rate, Stage::Waiting, 0.0),
            seed: stream,
            base,
        }
    }

    /// A change of speed at an instant, which carries the phase over without a
    /// jump (MATH §6.1).
    pub fn set_speed(&mut self, at: Instant, speed: f64, sample_rate: f64) {
        let (turns, wraps) = self.phase.at(at);
        self.phase = Phase {
            start: at,
            turns,
            step: frequency(speed) / sample_rate,
            draws: self.phase.draws.wrapping_add(wraps),
        };
    }

    /// A change of DELAY at an instant (MATH §6.5): the delay's progress carried
    /// over, the share of its stage that has passed going on at the new length;
    /// a change to DELAY 0 or below ends the delay at once.
    pub fn set_delay(&mut self, at: Instant, delay: f64, sample_rate: f64) {
        let (stage, passed) = self.fade.at(at);
        self.fade = Fade::of(at, delay, sample_rate, stage, passed);
    }

    /// Its phase in turns at an instant at or after its last start, from 0 up to
    /// 1.
    #[must_use]
    pub fn turns(&self, at: Instant) -> f64 {
        self.phase.at(at).0
    }

    /// The values that it has drawn by an instant, its first counted as 0,
    /// modulo 2⁶⁴: c of MATH §6.3, whose value its sample and hold holds.
    #[must_use]
    pub fn draws(&self, at: Instant) -> u64 {
        self.phase.draws.wrapping_add(self.phase.at(at).1)
    }

    /// ℓ of a wave at an instant (MATH §6.2), the sample and hold holding the
    /// value of index c + 1 after its base in its stream (MATH §6.3).
    #[must_use]
    pub fn wave(&self, at: Instant, wave: Wave) -> f64 {
        let (turns, wraps) = self.phase.at(at);
        let held = if wave == Wave::SampleAndHold {
            self.held(self.phase.draws.wrapping_add(wraps))
        } else {
            0.0
        };
        shape(wave, turns, held)
    }

    /// The fade w of its delay at an instant (MATH §6.4), from 0 to 1.
    #[must_use]
    pub fn fade(&self, at: Instant) -> f64 {
        self.fade.depth(at)
    }

    /// The fade of its delay from an instant on: w there and where its stages
    /// end.
    #[must_use]
    pub fn fading(&self, at: Instant) -> Fading {
        self.fade.fading(at)
    }

    /// Its phase's advance a sample, f/fₑ in turns: from 2⁵³ on the phase stands
    /// still (MATH §6.1).
    #[must_use]
    pub const fn step(&self) -> f64 {
        self.phase.step
    }

    /// The value that its sample and hold holds after a count of draws, its
    /// first counted as 0 (MATH §6.3), as [`Lfo::wave`] takes it.
    #[must_use]
    pub fn held(&self, draws: u64) -> f64 {
        random::sample_hold(random::value(self.seed, self.base.wrapping_add(draws).wrapping_add(1)))
    }
}
