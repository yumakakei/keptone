#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
#
#   ci/emulated.sh <sde64> [model...]
#
# Runs the workspace's tests under Intel's Software Development Emulator, as
# processors the machine running it may not be: SDE reports through CPUID what
# the processor it emulates has, and stops at any instruction that processor
# would not have run. Cargo runs each test program under it (the target's
# runner). On Linux, for x86-64. Only the test programs are checked, which hold
# all of Keptone and Rust's standard library: the system's own libraries are the
# system's matter, and a model the system's own programs cannot run as is left
# out, and said to be.
#
# Each model runs the default build, for x86-64-v3 (.cargo/config.toml); a run
# that SDE stops shows that the build took an instruction above its baseline.
# The models that have AVX-512 run the build for x86-64-v4 as well
# (ci/targets.sh), with vectors of 512 bits (.cargo/config.toml). The flags
# given here for a build are added to those of .cargo/config.toml.
#
# The models, by default: Haswell (the first with all of x86-64-v3), Alder Lake
# and Arrow Lake (x86-64-v3 and no AVX-512), Skylake-SP (the first with all of
# x86-64-v4), Ice Lake-SP and Sapphire Rapids.
#
# The tests of keptone-math against CORE-MATH take 10⁸ random inputs for each
# function in the release profile, and 10⁶ here, unless KEPTONE_ORACLE_INPUTS
# says otherwise; they need CORE-MATH's files (ci/core-math.sh).
set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "usage: ci/emulated.sh <sde64> [model...]" >&2
    exit 2
fi
sde=$(realpath "$1")
shift
models=("$@")
if [[ ${#models[@]} -eq 0 ]]; then
    models=(hsw adl arl skx icx spr)
fi
triple=x86_64-unknown-linux-gnu
export KEPTONE_ORACLE_INPUTS=${KEPTONE_ORACLE_INPUTS:-1000000}

# The builds each model runs besides the default one.
builds_of() {
    case "$1" in
    hsw | adl | arl) echo "" ;;
    skx | icx | spr) echo "v4" ;;
    *) return 1 ;;
    esac
}

status=0
for model in "${models[@]}"; do
    extra=$(builds_of "$model") || {
        echo "error: the builds of the model $model are not known" >&2
        exit 2
    }
    if ! refusal=$("$sde" "-$model" -chip_check_exe_only 1 -- /bin/true 2>&1); then
        echo "== $model: left out, the system's own programs do not run as it: ${refusal%%$'\n'*}"
        continue
    fi
    for build in v3 $extra; do
        arguments=(--release --locked --workspace --lib --bins --tests)
        if [[ $build == v3 ]]; then
            target_dir=target
            flags='[]'
        else
            arguments+=(--target "$triple")
            target_dir=target/targets
            flags='["-C", "target-cpu=x86-64-v4"]'
        fi
        echo "== $model, the build for x86-64-$build"
        if ! cargo test "${arguments[@]}" --target-dir "$target_dir" \
            --config "target.$triple.rustflags=$flags" \
            --config "target.$triple.runner=[\"$sde\", \"-$model\", \"-chip_check_exe_only\", \"1\", \"--\"]" \
            -- --test-threads 4; then
            echo "error: the tests failed on $model in the build for x86-64-$build" >&2
            status=1
        fi
    done
done
exit "$status"
