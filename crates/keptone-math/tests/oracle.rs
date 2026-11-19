// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! sin2pi, cos2pi, exp2, exp2m1, log2 and log2p1 against CORE-MATH's correctly
//! rounded `cr_sinpi`, `cr_cospi`, `cr_exp2`, `cr_exp2m1`, `cr_log2` and
//! `cr_log2p1` (docs/MATH.md §16: within 1 ulp, on 10⁸ random inputs and the
//! special points).
//!
//! The oracle is a C program, `tests/oracle/core_math_oracle.c`, that the test
//! compiles with Clang (`KEPTONE_CLANG`, else `clang` on PATH) and the six
//! files of CORE-MATH it calls, from `thirdparty/core-math` (`ci/core-math.sh`
//! fetches them), and talks to through pipes: inputs on its standard input, the
//! correctly rounded results on its standard output.
//!
//! The release profile takes 10⁸ random inputs for each function, the dev
//! profile 10⁶; `KEPTONE_ORACLE_INPUTS` sets the number (ci/emulated.sh takes
//! fewer under the emulator). A zero must have the sign the C23 functions give
//! it, and ±∞ and NaN must be the same.

mod common;

use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;

use common::{SplitMix64, ulp_error};
use core::f64::consts::SQRT_2;

use keptone_math::{cos2pi, exp2, exp2m1, log2, log2p1, sin2pi};

/// What one comparison takes: the oracle's function, ours, the argument the
/// oracle is given for an input, the random inputs and the special points.
struct Case {
    oracle: &'static str,
    ours: fn(f64) -> f64,
    argument: fn(f64) -> f64,
    random: fn(&mut SplitMix64) -> f64,
    specials: fn() -> Vec<f64>,
    seed: u64,
}

/// The largest error found, in ulp, the input it was found at, and how many
/// results were not the correctly rounded one.
#[derive(Debug, Default)]
struct Worst {
    ulp: f64,
    input: f64,
    not_correctly_rounded: u64,
}

#[test]
fn sin2pi_against_core_math() {
    let worst = compare(&Case {
        oracle: "sinpi",
        ours: sin2pi,
        argument: |x| 2.0 * x,
        random: turns,
        specials: turn_specials,
        seed: 0x5149_6e32_7069,
    });
    assert!(worst.ulp <= 1.0, "sin2pi: {worst:?}");
}

#[test]
fn cos2pi_against_core_math() {
    let worst = compare(&Case {
        oracle: "cospi",
        ours: cos2pi,
        argument: |x| 2.0 * x,
        random: turns,
        specials: turn_specials,
        seed: 0x636f_7332_7069,
    });
    assert!(worst.ulp <= 1.0, "cos2pi: {worst:?}");
}

#[test]
fn exp2_against_core_math() {
    let worst = compare(&Case {
        oracle: "exp2",
        ours: exp2,
        argument: |z| z,
        random: exponents,
        specials: exponent_specials,
        seed: 0x6578_7032,
    });
    assert!(worst.ulp <= 1.0, "exp2: {worst:?}");
}

#[test]
fn exp2m1_against_core_math() {
    let worst = compare(&Case {
        oracle: "exp2m1",
        ours: exp2m1,
        argument: |z| z,
        random: exponents_minus_one,
        specials: exponent_minus_one_specials,
        seed: 0x6578_7032_6d31,
    });
    assert!(worst.ulp <= 1.0, "exp2m1: {worst:?}");
}

#[test]
fn log2_against_core_math() {
    let worst = compare(&Case {
        oracle: "log2",
        ours: log2,
        argument: |x| x,
        random: positives,
        specials: logarithm_specials,
        seed: 0x6c6f_6732,
    });
    assert!(worst.ulp <= 1.0, "log2: {worst:?}");
}

#[test]
fn log2p1_against_core_math() {
    let worst = compare(&Case {
        oracle: "log2p1",
        ours: log2p1,
        argument: |x| x,
        random: above_minus_one,
        specials: log2p1_specials,
        seed: 0x6c6f_6732_7031,
    });
    assert!(worst.ulp <= 1.0, "log2p1: {worst:?}");
}

/// Random phases in turns: a half of them uniform on (−1, 1); a quarter
/// spread over every binade from the subnormals to 2⁶¹; a quarter next to a
/// multiple of 1/8 within ±16, where the octants of the reduction meet.
fn turns(rng: &mut SplitMix64) -> f64 {
    match rng.below(4) {
        0 | 1 => 2.0 * rng.unit() - 1.0,
        2 => rng.binades(1023 + 60),
        _ => eighth(rng) + rng.binades(1023 - 4),
    }
}

/// A multiple of 1/8 within ±16.
fn eighth(rng: &mut SplitMix64) -> f64 {
    f64::from(i32::try_from(rng.below(257)).expect("below 257") - 128) / 8.0
}

fn turn_specials() -> Vec<f64> {
    let mut specials = vec![
        0.0,
        f64::from_bits(1),
        f64::MIN_POSITIVE,
        1e-300,
        2e-9,
        0.1,
        1e9 + 0.375,
    ];
    specials.extend((-32..=32).map(|k| f64::from(k) / 8.0));
    for k in [1.0, 2.0, 3.0, 4.0, 5.0] {
        let x: f64 = k / 8.0;
        specials.extend([x.next_up(), x.next_down()]);
    }
    specials.extend([
        2.0_f64.powi(51) + 0.5,
        2.0_f64.powi(52) - 0.5,
        2.0_f64.powi(52),
        2.0_f64.powi(53),
        1e300,
    ]);
    specials.extend([f64::INFINITY, f64::NAN]);
    let negatives: Vec<f64> = specials.iter().map(|x| -x).collect();
    specials.extend(negatives);
    specials
}

/// Random exponents: a half of them uniform over [−1080, 1030), overflow and
/// underflow included; a quarter uniform on (−1, 1); a quarter next to an
/// integer or a half-integer of the range.
fn exponents(rng: &mut SplitMix64) -> f64 {
    match rng.below(4) {
        0 | 1 => rng.unit().mul_add(2110.0, -1080.0),
        2 => 2.0 * rng.unit() - 1.0,
        _ => f64::from(i32::try_from(rng.below(4241)).expect("below 4241") - 2160) / 2.0 + rng.binades(1023 - 2),
    }
}

fn exponent_specials() -> Vec<f64> {
    let mut specials = vec![0.0, -0.0, 1e-300, -1e-300, 0.5, -0.5, 1.0, -1.0, 2.0_f64.powi(-60)];
    for z in [
        1023.0, 1023.5, 1024.0, -1022.0, -1022.5, -1023.0, -1074.0, -1074.5, -1075.0, -1076.0,
    ] {
        let z: f64 = z;
        specials.extend([z, z.next_up(), z.next_down()]);
    }
    specials.extend([f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 1e300, -1e300]);
    specials
}

/// Random exponents for exp2m1: a quarter uniform over [−1100, 1030),
/// overflow included and −1 reached; a quarter small, spread over the binades
/// below 1/2, the subnormals included, where 2^z − 1 is near 0; a quarter
/// uniform on (−60, 60), where 2^z and 1 cancel or round together; a quarter
/// next to a multiple of 1/2 within ±64, where the reduction's k changes.
fn exponents_minus_one(rng: &mut SplitMix64) -> f64 {
    match rng.below(4) {
        0 => rng.unit().mul_add(2130.0, -1100.0),
        1 => rng.binades(1023 - 2),
        2 => rng.unit().mul_add(120.0, -60.0),
        _ => f64::from(i32::try_from(rng.below(257)).expect("below 257") - 128) / 2.0 + rng.binades(1023 - 4),
    }
}

fn exponent_minus_one_specials() -> Vec<f64> {
    let mut specials = vec![
        0.0,
        -0.0,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::MIN_POSITIVE,
        1e-300,
        -1e-300,
    ];
    for z in [
        0.5,
        -0.5,
        1.0,
        -1.0,
        1.5,
        -1.5,
        2.0_f64.powi(-60),
        53.0,
        54.0,
        -53.0,
        -53.5,
        -54.0,
        -54.5,
        1023.0,
        1023.5,
        1024.0,
        -1022.0,
        -1074.0,
        -1076.0,
        -1100.0,
    ] {
        let z: f64 = z;
        specials.extend([z, z.next_up(), z.next_down()]);
    }
    specials.extend([f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 1e300, -1e300]);
    specials
}

/// 2^k for a random k of [−1022, 1023].
fn power(rng: &mut SplitMix64) -> f64 {
    f64::from_bits((1 + rng.below(2046)) << 52)
}

/// A positive x moved by a random number of ulps, from −8 to 8.
fn nudge(x: f64, rng: &mut SplitMix64) -> f64 {
    f64::from_bits((x.to_bits() + rng.below(17)) - 8)
}

/// Random arguments of log2, every one positive: a quarter spread over every
/// binade, from the subnormals to the largest; a quarter uniform on [1/2, 2),
/// where log2 x is small or the reduction's split moves; a quarter within
/// 8 ulp of √2 times a power of two, where the split chooses m; a quarter
/// within 8 ulp of a power of two.
fn positives(rng: &mut SplitMix64) -> f64 {
    match rng.below(4) {
        0 => rng.binades(2046).abs(),
        1 => 1.5_f64.mul_add(rng.unit(), 0.5),
        2 => {
            let x = SQRT_2 * power(rng);
            nudge(x, rng)
        }
        _ => {
            let x = power(rng);
            nudge(x, rng)
        }
    }
}

fn logarithm_specials() -> Vec<f64> {
    let mut specials = vec![0.0, -0.0, f64::from_bits(1), f64::MIN_POSITIVE, f64::MAX, 1e-300, 1e300];
    for x in [1.0, 2.0, 0.5, 3.0, 10.0, SQRT_2, SQRT_2 / 2.0, 2.0 * SQRT_2] {
        let x: f64 = x;
        specials.extend([x, x.next_up(), x.next_down()]);
    }
    specials.extend([-1.0, -f64::MIN_POSITIVE, f64::INFINITY, f64::NEG_INFINITY, f64::NAN]);
    specials
}

/// Random arguments of log2p1, every one above −1: a quarter uniform on
/// (−1, 1); a quarter small, spread over the binades below 1/2, the
/// subnormals included, of either sign; a quarter above −1 by an amount spread
/// over the binades below 1/2; a quarter from 1/2 to the largest binary64,
/// spread over its binades.
fn above_minus_one(rng: &mut SplitMix64) -> f64 {
    match rng.below(4) {
        0 => 2.0_f64.mul_add(rng.unit(), -1.0),
        1 => rng.binades(1023 - 2),
        2 => -1.0 + rng.binades(1023 - 2).abs(),
        _ => f64::from_bits(((1022 + rng.below(1024)) << 52) | (rng.next_u64() >> 12)),
    }
}

fn log2p1_specials() -> Vec<f64> {
    let mut specials = vec![
        0.0,
        -0.0,
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        1e-300,
        -1e-300,
        f64::MAX,
        1e300,
    ];
    // The ends of the range in which log2p1 takes x itself, and points beyond.
    for x in [
        SQRT_2 - 1.0,
        SQRT_2 / 2.0 - 1.0,
        0.5,
        -0.5,
        1.0,
        -0.75,
        2.0_f64.powi(53),
        2.0_f64.powi(-53),
    ] {
        let x: f64 = x;
        specials.extend([x, x.next_up(), x.next_down()]);
    }
    specials.extend([
        -1.0,
        (-1.0_f64).next_up(),
        -2.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ]);
    specials
}

/// The number of random inputs.
fn count() -> u64 {
    match std::env::var("KEPTONE_ORACLE_INPUTS") {
        Ok(n) => n.parse().expect("KEPTONE_ORACLE_INPUTS is a count"),
        Err(_) if cfg!(debug_assertions) => 1_000_000,
        Err(_) => 100_000_000,
    }
}

/// The inputs of a case, the special points first, the same every time.
fn inputs(case: &Case) -> impl Iterator<Item = f64> + use<> {
    let random = case.random;
    let mut rng = SplitMix64(case.seed);
    (case.specials)()
        .into_iter()
        .chain((0..count()).map(move |_| random(&mut rng)))
}

/// Every input through ours and the oracle, the worst error kept; a
/// difference in a zero's sign, an infinity or a NaN fails at once.
fn compare(case: &Case) -> Worst {
    let mut child = Command::new(oracle())
        .arg(case.oracle)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the oracle starts");
    let stdin = child.stdin.take().expect("its input");
    let argument = case.argument;
    let sent = inputs(case);
    let writer = thread::spawn(move || {
        let mut out = BufWriter::with_capacity(1 << 20, stdin);
        for x in sent {
            out.write_all(&argument(x).to_ne_bytes()).expect("the oracle reads");
        }
    });
    let mut results = BufReader::with_capacity(1 << 20, child.stdout.take().expect("its output"));
    let mut worst = Worst::default();
    let mut buffer = [0_u8; 8];
    for x in inputs(case) {
        results.read_exact(&mut buffer).expect("the oracle answers");
        let reference = f64::from_ne_bytes(buffer);
        let ours = (case.ours)(x);
        if reference.is_nan() || ours.is_nan() || reference.is_infinite() || reference == 0.0 {
            assert!(
                ours.to_bits() == reference.to_bits() || (ours.is_nan() && reference.is_nan()),
                "{}({x:e}) = {ours:e}, correctly rounded {reference:e}",
                case.oracle
            );
            continue;
        }
        let error = ulp_error(ours, reference);
        if error > 0.0 {
            worst.not_correctly_rounded += 1;
        }
        if error > worst.ulp {
            worst.ulp = error;
            worst.input = x;
        }
    }
    writer.join().expect("the writer finishes");
    assert!(child.wait().expect("the oracle ends").success(), "the oracle failed");
    println!(
        "{}: worst {:.4} ulp at {:e}; {} of {} random inputs and the special points not correctly rounded",
        case.oracle,
        worst.ulp,
        worst.input,
        worst.not_correctly_rounded,
        count()
    );
    worst
}

/// The oracle, compiled once for the test process: the filter as the project's
/// own C is compiled, in C23 without extensions and with every warning an error
/// (docs/DESIGN.md §13.1), and CORE-MATH's files with floating-point
/// contraction off, one of the two settings its README allows (contraction
/// everywhere or nowhere), so that they compute as they are written.
fn oracle() -> &'static Path {
    static ORACLE: OnceLock<PathBuf> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let core_math = manifest.join("../../thirdparty/core-math/src/binary64");
        let functions = [
            core_math.join("sinpi/sinpi.c"),
            core_math.join("cospi/cospi.c"),
            core_math.join("exp2/exp2.c"),
            core_math.join("exp2m1/exp2m1.c"),
            core_math.join("log2/log2.c"),
            core_math.join("log2p1/log2p1.c"),
        ];
        for source in &functions {
            assert!(
                source.exists(),
                "{} is missing: fetch CORE-MATH with ci/core-math.sh",
                source.display()
            );
        }
        let directory = Path::new(env!("CARGO_TARGET_TMPDIR"));
        let filter = directory.join("core_math_oracle.o");
        let output = directory.join(if cfg!(windows) {
            "core_math_oracle.exe"
        } else {
            "core_math_oracle"
        });
        let clang = std::env::var_os("KEPTONE_CLANG").unwrap_or_else(|| "clang".into());
        compile(
            Command::new(&clang)
                .args(["-std=c23", "-Wall", "-Wextra", "-Wpedantic", "-Wconversion", "-Werror"])
                .args(["-O2", "-c", "-o"])
                .arg(&filter)
                .arg(manifest.join("tests/oracle/core_math_oracle.c")),
        );
        let mut command = Command::new(&clang);
        command
            .args(["-O2", "-ffp-contract=off", "-fuse-ld=lld", "-o"])
            .arg(&output)
            .arg(&filter)
            .args(&functions);
        // CORE-MATH's paths with FMA, at the project's baseline (DESIGN §10.1).
        if cfg!(target_arch = "x86_64") {
            command.arg("-march=x86-64-v3");
        }
        if !cfg!(windows) {
            command.arg("-lm");
        }
        compile(&mut command);
        output
    })
}

/// Runs Clang, and fails with what it said if it fails.
fn compile(command: &mut Command) {
    let done = command.output().expect("clang runs");
    assert!(
        done.status.success(),
        "{command:?} failed:\n{}",
        String::from_utf8_lossy(&done.stderr)
    );
}
