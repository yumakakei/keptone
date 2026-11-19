<!--
SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
SPDX-License-Identifier: GPL-3.0-or-later

This file is part of Keptone. The SPDX lines name its copyright holder and its
license, in the machine-readable form of the REUSE specification: the GNU
General Public License, version 3 or any later version
(LICENSES/GPL-3.0-or-later.txt).
-->

# Dependencies

Every dependency of Keptone, and every pinned tool its build and CI use: what it is, why it is used rather than written, its license, and its pinned version or commit. A dependency is added only with its entry here, in the same commit (`docs/DESIGN.md` §12.1). `docs/DESIGN.md` §12.2 lists the dependencies planned for later milestones; each moves here when it is added.

## Libraries

None in the product. The workspace takes no crate from crates.io: `Cargo.lock` names the workspace's own crates only, and cargo always runs with `--locked`.

The fuzz targets, a crate outside the workspace (`fuzz/libfuzzer`), take libfuzzer-sys and what it needs from crates.io; `fuzz/libfuzzer/Cargo.lock` pins each of them, and `ci/fuzz.sh` checks that it is up to date. None of them is built into Keptone.

| Crate | Version | License | Why |
| --- | --- | --- | --- |
| libfuzzer-sys | 0.4.13, the latest on 2026-11-17, pinned exactly in `fuzz/libfuzzer/Cargo.toml` | (MIT OR Apache-2.0) AND NCSA | LLVM's libFuzzer, which it compiles from the sources it carries, and the macro that makes a fuzz target of a function: the coverage-guided fuzzing of every parser (DESIGN §12.3). A fuzzer of the project's own would cost far more. |
| arbitrary | 1.5.0 | MIT OR Apache-2.0 | libfuzzer-sys's own, for targets that take structured inputs; Keptone's take bytes. |
| cc | 1.11.2 | MIT OR Apache-2.0 | libfuzzer-sys's build script compiles libFuzzer with it, by Clang (`ci/fuzz.sh`). |
| find-msvc-tools, jobserver, libc, shlex | 0.1.15, 0.1.35, 0.2.191, 2.0.1 | MIT OR Apache-2.0 | cc's own. |
| cfg-if, getrandom, r-efi | 1.0.5, 0.4.3, 6.0.0 | MIT OR Apache-2.0; r-efi MIT OR Apache-2.0 OR LGPL-2.1-or-later | In `Cargo.lock` for builds on other systems; none is built on Linux, where the fuzzing runs. |

## Submodules

| Dependency | Commit | Where | License | Why |
| --- | --- | --- | --- | --- |
| CORE-MATH | `35c1dd0e08b9ea28a55dcf9378869806e4efe37f`, the latest on 2026-11-16 | `thirdparty/core-math` (`.gitmodules`); the tests of `keptone-math` only | MIT | Its `cr_sinpi`, `cr_cospi`, `cr_exp2`, `cr_exp2m1`, `cr_log2` and `cr_log2p1` round correctly for every input, which makes them the reference that `sin2pi`, `cos2pi`, `exp2`, `exp2m1`, `log2` and `log2p1` are measured against (MATH §16). The tests compile its six files, with the header that `log2p1.c` includes, and a filter of the project's own, `crates/keptone-math/tests/oracle/core_math_oracle.c`, with Clang, and talk to the program through pipes, so that no Rust calls C. `ci/core-math.sh` fetches those files and the license alone, at the recorded commit, in a shallow, partial and sparse fetch; `git submodule update --init` fetches all of CORE-MATH. Nothing of it is built into Keptone. |

## Scripts

The tools that `scripts/` runs to compute what the code and the tests take, which is checked in with the command that wrote it; neither the build nor the tests run them.

| Tool | Version | License | Why |
| --- | --- | --- | --- |
| Python | 3.14, the target of `ruff.toml` | PSF-2.0 | The language of `scripts/`. |
| Sollya | 8.0, Ubuntu's package | CeCILL-C | `scripts/minimax.sollya` designs the polynomials of `keptone-math` with `fpminimax`, their coefficients in binary64 and the leading one a double-double, and certifies their errors with `supnorm` (MATH §12); `scripts/minimax.py` runs it and writes `crates/keptone-math/src/coefficients.rs`, which says which Sollya wrote it. |
| mpmath | 1.4.1, Ubuntu's package `python3-mpmath` | BSD-3-Clause | Arithmetic of any precision: `scripts/sinc_references.py`, `scripts/adaa_references.py` and `scripts/kapteyn_references.py` write the references of sinc and sinc′, of ADAA-1 and ADAA-2, of sinc″ and sinc‴ and of the Kapteyn coefficients in `crates/keptone-math/tests/data/`, and `scripts/rounding_bounds.py` computes the constants of the rounding analyses in the documentation of `keptone-math`. |
| SymPy | 1.14.0, Ubuntu's package `python3-sympy` | BSD-3-Clause | The computer algebra that MATH §10.3 asks for: `scripts/adaa_derivatives.py` derives with it the third derivative in the expansion of ADAA-2 and the Taylor coefficients of sinc″ and sinc‴, and checks them against `crates/keptone-math`. |

## Toolchain

| Tool | Version | Pinned in | License | Why |
| --- | --- | --- | --- | --- |
| Rust: rustc, cargo, clippy, rustfmt | the latest stable; `rust-version` 1.100, the stable release at the project's start | `rust-toolchain.toml`, `Cargo.toml` | MIT OR Apache-2.0 | The language of the workspace. |
| LLVM: Clang, LLD | 23.1.5, the latest release on 2026-11-16 | `ci/setup.sh`: the SHA-256 of the Windows and macOS release archives, and the fingerprint of apt.llvm.org's signing key on Ubuntu | Apache-2.0 WITH LLVM-exception | Clang drives the link and LLD links on every system (`.cargo/config.toml`); Clang compiles the CORE-MATH oracle of the tests of `keptone-math`, and is to compile the C23 layout tests and the C++26 wrappers. `ci/setup.sh` checks its floating-point contraction (DESIGN §13.1). |
| ruff | 0.17.5, the latest release on 2026-11-16 | `.github/workflows/ci.yml` | MIT | Checks the Python of `scripts/`. |
| REUSE tool | Ubuntu's package | — | GPL-3.0-or-later | Checks that every file names its copyright holder and license. |
| Rust nightly | `nightly-2026-11-16`, the latest on 2026-11-17 | `ci/fuzz.sh` | MIT OR Apache-2.0 | cargo-fuzz builds the fuzz targets with options that only a nightly rustc takes, for the address sanitizer and the coverage that guides libFuzzer. The workspace is built by the stable toolchain alone. |
| cargo-fuzz | 0.13.2, the latest release on 2026-11-17 | `ci/fuzz.sh` | MIT OR Apache-2.0 | Builds the fuzz targets for libFuzzer (`ci/fuzz.sh setup` installs it). |
| actions/upload-artifact | v7.0.2, commit `cf430e030ddbb5b0abf93d22962f4752f3646cd9`, the latest release on 2026-11-17 | `.github/workflows/ci.yml`, `.github/workflows/fuzz.yml` | MIT | GitHub's own action, and the only action the CI uses: it keeps a run's fuzzing corpora and the inputs that failed, for the next run to carry on from and to replay. |

Not dependencies, and used on a machine of one's own: Intel's Software Development Emulator, for the tests at levels of x86-64 no machine at hand has (`ci/emulated.sh`). The tools of later milestones are listed in `docs/DESIGN.md` §12.3.
