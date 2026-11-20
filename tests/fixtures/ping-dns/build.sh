#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K203: build the loopback DNS fixture, the release initfs, and the patched initfs used by the
# ping-dns qtest. The test-only resolv.conf nameserver 127.0.0.1 is staged here; production
# resolver policy (K198) is untouched.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/ping-dns
mkdir -p "$out"

if_missing=0
patch_only=0
for argument in "$@"; do
    case "$argument" in
        --if-missing) if_missing=1 ;;
        --patch-only) patch_only=1 ;;
        *)
            echo "usage: sh tests/fixtures/ping-dns/build.sh [--if-missing] [--patch-only]" >&2
            exit 2
            ;;
    esac
done
if [ "$patch_only" -eq 0 ] && [ "$if_missing" -eq 1 ] && [ -s "$out/ping-dns-dnsd" ] && [ -s "$out/initfs.img" ]; then
    echo "ping-dns: --if-missing: fixture and initfs already exist; skipping build"
    exit 0
fi

if [ "$patch_only" -eq 0 ]; then
    echo "ping-dns: building the loopback DNS fixture"
    RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
        CARGO_TARGET_DIR="$out/target" \
        cargo build --manifest-path "$root/tests/fixtures/ping-dns/dnsd/Cargo.toml" \
            --target x86_64-unknown-none --release
    cp "$out/target/x86_64-unknown-none/release/raven-ping-dnsd" "$out/ping-dns-dnsd"

    echo "ping-dns: building the release initfs"
    # The image builder can reuse a cached build/x86_64/initfs.img; force the initfs so the current
    # raven-ping binary is packed when this script rebuilds.
    rm -f build/x86_64/initfs.img
    cargo xtask image --arch x86_64 --profile release --output "$out/base.img"
else
    test -s build/x86_64/initfs.img || {
        echo "ping-dns: --patch-only needs build/x86_64/initfs.img" >&2
        exit 2
    }
    test -s "$out/ping-dns-dnsd" || {
        echo "ping-dns: --patch-only needs $out/ping-dns-dnsd" >&2
        exit 2
    }
fi

echo "ping-dns: patching the initfs (fixture + test resolv.conf + getty order)"
CARGO_TARGET_DIR="$out/host-target" \
    cargo run --quiet --manifest-path "$root/tests/fixtures/ping-dns/patch-initfs/Cargo.toml" -- \
        build/x86_64/initfs.img "$out/ping-dns-dnsd" "$out/initfs.img"

test -s "$out/initfs.img"
echo "ping-dns: initfs=$out/initfs.img"
