#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S44 C19 sigaction/sigframe fixture for the RAVEN_QT_ARCH architecture.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/signal
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
        ;;
    aarch64)
        target=aarch64-unknown-none-softfloat
        rustflags="-C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/aarch64.ld"
        ;;
    *)
        echo "signal fixture: unsupported architecture $arch" >&2
        exit 1
        ;;
esac
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="$rustflags" \
    cargo build -p raven-s44-action --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-s44-action"

# S44 poll-interrupt row: a distinct first process blocks in EVQ_WAIT and is interrupted by a
# caught kernel SIGUSR1 sent from a helper thread.
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/poll-initfs.img" \
    /init="$target_dir/$target/release/raven-s44-poll"
