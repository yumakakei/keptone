#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
#
#   ci/rust.sh
#
# Checks and tests the workspace with the latest stable toolchain, which
# rust-toolchain.toml names: the layout (rustfmt), clippy with every warning an
# error (Cargo.toml names the lints), the documentation with every warning of
# rustdoc an error (a broken link among them), and the tests in the dev and the
# release profile. Nothing is fetched from crates.io (--locked keeps it that
# way).
set -euo pipefail

rustc --version
cargo --version
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo test --workspace --locked
cargo test --workspace --locked --release
