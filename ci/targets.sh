#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
#
#   ci/targets.sh [test]
#
# Builds the workspace for the levels of x86-64 offered beside the default one.
# The default build is for x86-64-v3 (.cargo/config.toml), which ci/rust.sh
# builds and tests; beside it there is a build for x86-64-v4 (docs/DESIGN.md
# §10.1), with vectors of 512 bits. LLVM keeps to 256 bits for x86-64-v4 unless
# told otherwise, which -prefer-256-bit turned off does: .cargo/config.toml turns
# it off for every build for x86-64, and this build adds only the target CPU to
# the flags there. rustc passes the feature to LLVM, and warns that it does not
# know it: such features are to become an error once their replacement,
# -Zllvm-target-feature, is stable with -Zbuild-std (rust-lang/rust#162235), and
# the builds will then take the replacement. The target is named, so that build
# scripts, which run here, are built for this processor; the builds go to
# target/targets/.
#
# With `test`, each build's tests run too, where the processor has what the
# build needs: AVX-512 F, BW, CD, DQ and VL besides x86-64-v3's AVX2, FMA, BMI1,
# BMI2, F16C, LZCNT and MOVBE. Where /proc/cpuinfo does not list one of those
# flags, the build is not tested, and the flags it lacks and the line of the
# processor's flags are printed, so that a runner whose processor has the level,
# though /proc/cpuinfo does not say so, can be told from one whose processor
# lacks it.
set -euo pipefail

case "$(uname -s)" in
MINGW* | MSYS* | CYGWIN*) triple=x86_64-pc-windows-msvc ;;
Linux) triple=x86_64-unknown-linux-gnu ;;
*)
    echo "error: the builds for the levels of x86-64 are for x86-64 Windows and Linux" >&2
    exit 1
    ;;
esac

# The flags of a list that /proc/cpuinfo, which MSYS2 has on Windows too, does
# not list, on one line: nothing where it lists them all.
missing() {
    local flag
    local absent=()
    for flag in "$@"; do
        grep -qw -- "$flag" /proc/cpuinfo 2>/dev/null || absent+=("$flag")
    done
    echo "${absent[*]}"
}

builds=(
    'v4 ["-C", "target-cpu=x86-64-v4"] avx2 fma bmi1 bmi2 f16c abm movbe avx512f avx512bw avx512cd avx512dq avx512vl'
)
for build in "${builds[@]}"; do
    name=${build%% *}
    rest=${build#* }
    flags=${rest%]*}]
    read -r -a needs <<<"${rest##*] }"
    arguments=(--release --locked --workspace --target "$triple" --target-dir target/targets
        --config "target.$triple.rustflags=$flags")
    echo "== x86-64-$name: $flags"
    cargo build "${arguments[@]}" --all-targets
    if [[ "${1:-}" == test ]]; then
        lacking=$(missing "${needs[@]}")
        if [[ -z $lacking ]]; then
            cargo test "${arguments[@]}"
        else
            echo "note: /proc/cpuinfo does not list $lacking, of ${needs[*]}; the build is not tested"
            grep -m 1 '^flags' /proc/cpuinfo || echo "note: /proc/cpuinfo has no line of flags"
        fi
    fi
done
