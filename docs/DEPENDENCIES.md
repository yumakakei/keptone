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

None. The workspace takes no crate from crates.io: `Cargo.lock` names the workspace's own crates only, and cargo always runs with `--locked`. There is no git submodule yet.

## Toolchain

| Tool | Version | Pinned in | License | Why |
| --- | --- | --- | --- | --- |
| Rust: rustc, cargo, clippy, rustfmt | the latest stable; `rust-version` 1.100, the stable release at the project's start | `rust-toolchain.toml`, `Cargo.toml` | MIT OR Apache-2.0 | The language of the workspace. |
| LLVM: Clang, LLD | 23.1.5, the latest release on 2026-11-16 | `ci/setup.sh`: the SHA-256 of the Windows and macOS release archives, and the fingerprint of apt.llvm.org's signing key on Ubuntu | Apache-2.0 WITH LLVM-exception | Clang drives the link and LLD links on every system (`.cargo/config.toml`); Clang is to compile the C23 layout tests and the C++26 wrappers. `ci/setup.sh` checks its floating-point contraction (DESIGN §13.1). |
| ruff | 0.17.5, the latest release on 2026-11-16 | `.github/workflows/ci.yml` | MIT | Checks the Python of `scripts/`. |
| REUSE tool | Ubuntu's package | — | GPL-3.0-or-later | Checks that every file names its copyright holder and license. |

Not dependencies, and used on a machine of one's own: Intel's Software Development Emulator, for the tests at levels of x86-64 no machine at hand has (`ci/emulated.sh`). The tools of later milestones are listed in `docs/DESIGN.md` §12.3.
