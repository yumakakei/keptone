#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This file is part of Keptone. The SPDX lines name its copyright holder and its
# license, in the machine-readable form of the REUSE specification: the GNU
# General Public License, version 3 or any later version
# (LICENSES/GPL-3.0-or-later.txt).
#
#   ci/core-math.sh
#
# Fetches what the tests of keptone-math take of CORE-MATH (MIT): the files of
# the six correctly rounded functions that crates/keptone-math/tests/oracle.rs
# compiles and compares with, a header that one of them includes, and the
# license. They come from the repository that .gitmodules names, at the commit
# that the submodule thirdparty/core-math records (docs/DEPENDENCIES.md), in a
# fetch that is shallow, partial and sparse: that commit alone, and the contents
# of these eight files alone, some hundreds of kilobytes where the whole
# checkout takes more than 500 MB. Run it from the
# top of the repository. A checkout already there, such as
# `git submodule update --init` leaves, is kept if it is at the recorded commit.
set -euo pipefail

path=thirdparty/core-math
commit=$(git ls-files --stage -- "$path" | awk '$1 == "160000" { print $2 }')
if [[ -z $commit ]]; then
    echo "error: the index records no commit for $path" >&2
    exit 1
fi
url=$(git config --file .gitmodules "submodule.$path.url")
# Patterns of the sparse checkout, anchored at the top of CORE-MATH. They go to
# git on its standard input: on Windows, MSYS2's bash would rewrite an argument
# that begins with / into a path of its own.
files=(/LICENSE /src/binary64/sinpi/sinpi.c /src/binary64/cospi/cospi.c /src/binary64/exp2/exp2.c
    /src/binary64/exp2m1/exp2m1.c /src/binary64/log2/log2.c /src/binary64/log2p1/log2p1.c
    /src/binary64/log2p1/dint_log2p1.h)

if [[ -e $path/.git ]]; then
    have=$(git -C "$path" rev-parse HEAD)
    if [[ $have != "$commit" ]]; then
        echo "error: $path is at $have, not at the recorded $commit" >&2
        exit 1
    fi
    echo "$path is already at $commit"
    exit 0
fi

mkdir -p "$path"
git -C "$path" -c init.defaultBranch=main init --quiet
git -C "$path" remote add origin "$url"
printf '%s\n' "${files[@]}" | git -C "$path" sparse-checkout set --no-cone --stdin
git -C "$path" fetch --no-tags --quiet --depth 1 --filter=blob:none origin "$commit"
git -C "$path" checkout --quiet --detach "$commit"
git -C "$path" log -1 --format='%H %cs %s'
for file in "${files[@]}"; do
    [[ -f $path$file ]] || {
        echo "error: $path$file is missing" >&2
        exit 1
    }
done
