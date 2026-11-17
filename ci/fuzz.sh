#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt). It follows ADLplug-Next's ci/fuzz.sh, by the
# same author, under the same license.
#
#   ci/fuzz.sh setup
#   ci/fuzz.sh <seconds> <corpora> <crashes> [<earlier crashes>]
#
# Fuzzes every target of fuzz/libfuzzer with libFuzzer, each for <seconds>, on
# Linux for x86-64. cargo-fuzz builds the targets with the nightly toolchain
# pinned below, as it builds them by default: optimized, with debug assertions,
# overflow checks and the address sanitizer. libFuzzer itself, which
# libfuzzer-sys compiles from the sources it carries, is compiled by Clang, as
# the project's own C and C++ are.
#
# `setup` installs that toolchain and the cargo-fuzz pinned below, as a CI
# runner needs; a machine of one's own needs it once. The pins are the latest
# releases at the time they are set (docs/DEPENDENCIES.md).
#
# Before anything is built for fuzzing, the fuzz crate's layout and lints are
# checked as the workspace's are (ci/rust.sh), by the stable toolchain, and so
# is its Cargo.lock, which must not need to change.
#
# A target runs on a corpus of its own in <corpora>/<target>/, given first,
# which it grows and which may carry over from an earlier run; on its seed
# inputs (fuzz/seeds/<target>/) and the inputs that once made it fail
# (fuzz/regressions/<target>/); and with its dictionary
# (fuzz/dict/<target>.dict). An input that fails is written to
# <crashes>/<target>/. Once a target has run, its corpus is merged down to the
# fewest inputs that reach what all of it reached, if the run changed it.
#
# Given <earlier crashes>, each target first runs once on the inputs under
# <earlier crashes>/<target>/ that failed in an earlier run. One that still
# fails is kept in <crashes>/<target>/ again and the target is not fuzzed:
# fuzzing looks for inputs at random, and a later run need not come upon the
# same one, so it is replayed until it is fixed. The script fails when any
# target fails.
#
# No run of a target lasts longer than the time it was given and ten minutes
# more: a target that does not end by then, wedged, or held up for ever by one
# input, is stopped and counted as a failure, so that the targets after it
# still run, and the run ends by itself with what it found kept.
set -euo pipefail

toolchain=nightly-2026-11-16
cargo_fuzz=0.13.2

if [ "${1:-}" = setup ]; then
  rustup toolchain install "$toolchain" --profile minimal --no-self-update
  cargo install cargo-fuzz --version "$cargo_fuzz" --locked
  cargo fuzz --version
  exit 0
fi

if [ $# -lt 3 ] || [ $# -gt 4 ] || ! [[ $1 =~ ^[0-9]+$ ]]; then
  echo "usage: ci/fuzz.sh setup" >&2
  echo "       ci/fuzz.sh <seconds> <corpora> <crashes> [<earlier crashes>]" >&2
  exit 2
fi
seconds=$1
corpora=$2
crashes=$3
earlier=${4:-}

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) ;;
  *)
    echo "error: the fuzzing is for x86-64 Linux" >&2
    exit 1
    ;;
esac
version=$(cargo fuzz --version)
if [ "$version" != "cargo-fuzz $cargo_fuzz" ]; then
  echo "error: $version, where ci/fuzz.sh pins $cargo_fuzz: bash ci/fuzz.sh setup installs it" >&2
  exit 1
fi
rustc "+$toolchain" --version

# The fuzz crate, checked as the workspace is, and built with every target in
# it. Clang compiles libFuzzer, for libfuzzer-sys's build script.
export CC=clang CXX=clang++
manifest=fuzz/libfuzzer/Cargo.toml
cargo fmt --check --manifest-path "$manifest"
cargo clippy --manifest-path "$manifest" --all-targets --locked -- -D warnings
# cargo-fuzz sets RUSTFLAGS for its instrumentation, taking any RUSTFLAGS given
# to it after its own, and cargo then reads none of the rustflags of
# .cargo/config.toml: the ones of the project's builds for x86-64 Linux are
# given again here, so that the targets are linked by LLD and built for
# x86-64-v3, as the rest of the project is.
RUSTFLAGS="-C link-arg=-fuse-ld=lld -C target-cpu=x86-64-v3 -C target-feature=-prefer-256-bit" \
  cargo "+$toolchain" fuzz build --fuzz-dir fuzz/libfuzzer
build=fuzz/libfuzzer/target/x86_64-unknown-linux-gnu/release
mapfile -t targets < <(cargo "+$toolchain" fuzz list --fuzz-dir fuzz/libfuzzer)
if [ ${#targets[@]} -eq 0 ]; then
  echo "error: fuzz/libfuzzer has no fuzz targets" >&2
  exit 1
fi

# libFuzzer's limits for every run below: inputs of up to 16 KiB, which holds a
# file of three 32-voice dumps, and ten seconds an input, far more than any
# input of that size takes when nothing has gone wrong.
limits=(-max_len=16384 -timeout=10 -rss_limit_mb=2048)

# How a stopped target shows: the status timeout keeps for the time running
# out, and the ones that a program ended by a signal leaves behind.
was_stopped() {  # status
  [ "$1" -eq 124 ] || [ "$1" -eq 137 ] || [ "$1" -eq 143 ]
}

shopt -s nullglob
guard=$((seconds + 600))
status=0
for target in "${targets[@]}"; do
  fuzzer=$build/$target
  mkdir -p "$corpora/$target" "$crashes/$target"

  if [ -n "$earlier" ]; then
    # What libFuzzer names an input it writes out: crash-, leak-, oom- and
    # timeout- are those that failed. A slow-unit- input failed at nothing.
    failed=("$earlier/$target/"crash-* "$earlier/$target/"leak-* "$earlier/$target/"oom-* \
      "$earlier/$target/"timeout-*)
    if [ ${#failed[@]} -gt 0 ]; then
      echo "== $target: ${#failed[@]} inputs that failed in an earlier run"
      code=0
      timeout --kill-after=60s "$guard" "$fuzzer" "${limits[@]}" "${failed[@]}" || code=$?
      if [ "$code" -ne 0 ]; then
        if was_stopped "$code"; then
          echo "error: $target was still replaying the inputs that failed in an earlier run after $guard seconds, and was stopped" >&2
        else
          echo "error: $target still fails on an input that failed in an earlier run" >&2
        fi
        cp "${failed[@]}" "$crashes/$target/"
        status=1
        continue
      fi
    fi
  fi

  # The seed inputs, the regressions and the dictionary that the target has.
  inputs=()
  for directory in "fuzz/seeds/$target" "fuzz/regressions/$target"; do
    if [ -d "$directory" ]; then
      inputs+=("$directory")
    fi
  done
  dictionary=()
  if [ -f "fuzz/dict/$target.dict" ]; then
    dictionary=("-dict=fuzz/dict/$target.dict")
  fi

  # The inputs the corpus holds before the run, by name, which libFuzzer makes
  # of what an input holds: whether the run added any is what decides whether
  # there is a merge to make afterwards.
  held=$(find "$corpora/$target" -maxdepth 1 -type f -printf '%f\n' | sort | sha256sum)

  echo "== $target: fuzzing for $seconds seconds"
  code=0
  timeout --kill-after=60s "$guard" "$fuzzer" "${limits[@]}" ${dictionary[@]+"${dictionary[@]}"} \
    -max_total_time="$seconds" -print_final_stats=1 -artifact_prefix="$crashes/$target/" \
    "$corpora/$target" ${inputs[@]+"${inputs[@]}"} || code=$?
  if [ "$code" -ne 0 ]; then
    if was_stopped "$code"; then
      echo "error: $target was still running $guard seconds after it began, and was stopped" >&2
    else
      echo "error: $target failed; what made it fail is above, and any input it found is in $crashes/$target/" >&2
    fi
    status=1
    continue
  fi

  # libFuzzer replays the whole corpus it is given before it fuzzes, and each
  # run adds to it what it found, so a corpus that only grew would make each
  # run start later than the last. The merge, beside the corpus and not in it,
  # leaves the next run the inputs its coverage needs; one stopped half-way
  # loses nothing. A run that left the corpus as it was has nothing to merge.
  if [ "$(find "$corpora/$target" -maxdepth 1 -type f -printf '%f\n' | sort | sha256sum)" = "$held" ]; then
    echo "== $target: the run left the corpus as it found it, and there is nothing to merge down"
    continue
  fi
  echo "== $target: merging the corpus down"
  merged=$(mktemp -d)
  code=0
  timeout --kill-after=60s "$guard" "$fuzzer" "${limits[@]}" -merge=1 "$merged" "$corpora/$target" || code=$?
  kept=("$corpora/$target"/*)
  if [ "$code" -eq 0 ]; then
    rm -rf "${corpora:?}/$target"
    mv "$merged" "$corpora/$target"
    now=("$corpora/$target"/*)
    echo "== $target: ${#kept[@]} inputs merged down to ${#now[@]}"
  else
    rm -rf "$merged"
    echo "== $target: the merge did not finish (status $code), and the corpus of ${#kept[@]} inputs carries over as it is"
  fi
done
exit $status
