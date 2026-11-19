#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host check for the K26d release staging: every `build/aarch64/...` source in
# the Pi 4 release fragments must exist and be an AArch64 ELF, and the fragments
# must not reference `build/brush` or `build/x86_64/uutils`.
#
# This is a host artifact check; it makes no board claim.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"

fragments='profiles/pi4-release.d/30-brush.toml profiles/pi4-release.d/40-uutils.toml'

staged=$(sed -n 's/^src = "\(build\/aarch64\/[^"]*\)"$/\1/p' $fragments)
[ -n "$staged" ] || {
    echo "pi4-brush-stage: no build/aarch64 sources found" >&2
    exit 1
}

count=0
for path in $staged; do
    [ -f "$path" ] || {
        echo "pi4-brush-stage: missing $path" >&2
        exit 1
    }
    description=$(file -b "$path")
    case "$description" in
        *ELF*ARM\ aarch64*) ;;
        *)
            echo "pi4-brush-stage: $path is not aarch64: $description" >&2
            exit 1
            ;;
    esac
    machine=$(od -An -tx1 -N20 "$path" | tr -d ' \n')
    case "$machine" in
        *b700*) ;;
        *)
            echo "pi4-brush-stage: $path ELF e_machine is not 0xb7: $machine" >&2
            exit 1
            ;;
    esac
    echo "pi4-brush-stage: $path aarch64"
    count=$((count + 1))
done

if grep -Eq '^src = "build/(brush|x86_64/uutils)' $fragments; then
    echo "pi4-brush-stage: forbidden x86_64 staging source in the release fragments" >&2
    exit 1
fi

# The K169 acceptance is the actual raven-elf Strict planner, not readelf arithmetic.
echo "pi4-brush-stage: Strict planner aarch64"
timeout 30 cargo run -q -p xtask -- build brush --arch aarch64 --check || {
    echo "pi4-brush-stage: aarch64 brush fails the Strict load planner" >&2
    exit 1
}

# The x86_64 control is checked when it is staged (the K169 acceptance builds it).
if [ -f build/x86_64/brush ]; then
    echo "pi4-brush-stage: Strict planner x86_64 control"
    timeout 30 cargo run -q -p xtask -- build brush --arch x86_64 --check || {
        echo "pi4-brush-stage: x86_64 control fails the Strict load planner" >&2
        exit 1
    }
fi

echo "pi4-brush-stage: PASS ($count aarch64 files)"
