#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S197 aarch64 exec-default acceptance pair and overlay it as a one-shot service:
# /bin/signal-init installs a caught SIGUSR1 handler and execs /bin/signal-exec-child; the child
# reads the disposition and raises the signal. aarch64 only.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../../" && pwd)
cd "$root"

out=build/conformance/aarch64/signal-exec
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-aarch64}
if [ "$arch" != aarch64 ]; then
    echo "signal-exec: only aarch64 is the S197 acceptance target (got $arch)" >&2
    exit 1
fi
target=aarch64-unknown-none-softfloat
rustflags="-C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/aarch64.ld"
target_dir=${CARGO_TARGET_DIR:-target}
manifest=tests/conformance/aarch64/signal-exec/Cargo.toml

RUSTFLAGS="$rustflags" \
    cargo build --manifest-path "$manifest" --target-dir "$target_dir" \
    --target "$target" --release --bin signal-exec-child
cp "$target_dir/$target/release/signal-exec-child" "$out/exec-child.elf"
RUSTFLAGS="$rustflags" \
    cargo build --manifest-path "$manifest" --target-dir "$target_dir" \
    --target "$target" --release --bin signal-init

base=$root/build/aarch64/initfs.img
[ -f "$base" ] || cargo xtask initfs --arch aarch64 --profile release
cargo build -q -p raven-b70-overlay
overlay=${CARGO_TARGET_DIR:-$root/target}/debug/raven-b70-overlay
"$overlay" "$base" "$out/initfs.img" \
    "/bin/signal-init=$target_dir/$target/release/signal-init:0755" \
    "/etc/services.d/signal-exec-aarch64.toml=$root/tests/conformance/aarch64/signal-exec/signal-exec.service.toml:0644" \
    "/etc/svcd.conf=$root/tests/conformance/aarch64/signal-exec/signal-exec.svcd.conf:0644"
echo "signal-exec: staged $out/initfs.img"
