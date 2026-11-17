#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the guest service set and the argv/env wrapper used by
# `raven-atf-run ... --run-backend qemu`. The output is a stable directory so the host runner
# does not need to know the cargo target hashes.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/g1/guest
mkdir -p "$out/bin"

target_dir=${CARGO_TARGET_DIR:-$root/target}
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build \
        -p raven-init \
        -p raven-logd \
        -p raven-consd \
        -p raven-initfsd \
        -p raven-ramfsd \
        -p raven-nulld \
        -p raven-zerod \
        -p raven-randd \
        -p raven-vfsd \
        --target x86_64-unknown-none

for pair in \
    init:raven-init \
    logd:raven-logd \
    consd:raven-consd \
    initfsd:raven-initfsd \
    ramfsd:raven-ramfsd \
    nulld:raven-nulld \
    zerod:raven-zerod \
    randd:raven-randd \
    vfsd:raven-vfsd
do
    name=${pair%%:*}
    package=${pair#*:}
    cp "$target_dir/x86_64-unknown-none/debug/$package" "$out/bin/$name"
done

wrapper_target=build/fixtures/g1/wrapper-target
CARGO_TARGET_DIR=$wrapper_target \
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build \
        --manifest-path tests/fixtures/g1/wrapper/Cargo.toml \
        --target x86_64-unknown-none

cp "$wrapper_target/x86_64-unknown-none/debug/raven-g1-wrapper" "$out/bin/g1wrap"
