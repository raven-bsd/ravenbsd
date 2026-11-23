#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S36 lifecycle fixture and pack build/fixtures/s36-lifecycle/initfs.img.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/s36-lifecycle
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/s36-lifecycle/linker.ld" \
    cargo build -p raven-s36-lifecycle --target x86_64-unknown-none --release

# The vfsd map row needs the release services (vfsd), not the minimal base image. Prefer the
# release initfs cache from the K547 build when the shared build/<arch> image is the base profile;
# otherwise build the release profile (the clean-tree fallback).
base=build/x86_64/initfs.img
if ! strings "$base" 2>/dev/null | grep -q '/sbin/vfsd'; then
    cached=build/fixtures/sshd-trace/initfs.img
    if strings "$cached" 2>/dev/null | grep -q '/sbin/vfsd'; then
        echo "s36-lifecycle: using cached release initfs $cached" >&2
        base=$cached
    else
        cargo xtask initfs --arch x86_64 --profile release
        base=build/x86_64/initfs.img
    fi
fi
cargo build -q -p raven-b70-overlay
overlay=${CARGO_TARGET_DIR:-target}/debug/raven-b70-overlay
"$overlay" "$base" "$out/initfs.img" \
    "/bin/s36-lifecycle=$root/target/x86_64-unknown-none/release/raven-s36-lifecycle:0755" \
    "/etc/services.d/s36-lifecycle.toml=$root/tests/fixtures/s36-lifecycle/s36.service.toml:0644" \
    "/etc/svcd.conf=$root/tests/fixtures/s36-lifecycle/s36.svcd.conf:0644"
