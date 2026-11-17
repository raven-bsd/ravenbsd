#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S71 fault fixture and pack the initfs images:
#
#   build/fixtures/fault/initfs.img          client mode `client`  (in-flight/reconnect)
#   build/fixtures/fault/initfs-restart.img  client mode `restart` (idle/during-restart matrix)
#
# Both are self-contained: raven-init is PID 1 and the fixture binary provides the restartable
# `frt` scheme worker and the driver. They work on x86_64 and aarch64, unlike the x86_64-only
# release profile.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/fault
mkdir -p "$out"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
        ;;
    aarch64)
        target=aarch64-unknown-none-softfloat
        rustflags="-C relocation-model=static -C force-frame-pointers=yes -C link-arg=-T$root/userland/linker/aarch64.ld"
        ;;
    *)
        echo "fault fixture: unsupported architecture $arch" >&2
        exit 1
        ;;
esac

RUSTFLAGS="$rustflags" \
    cargo build -p raven-init -p raven-fault-reconnect --target "$target" --release

target_dir=${CARGO_TARGET_DIR:-$root/target}
initfs() {
    cargo run -q -p raven-initfs --features std --example mkinitfs -- \
        "$1" \
        /init="$target_dir/$target/release/raven-init" \
        /sbin/frtsvc="$target_dir/$target/release/raven-fault" \
        /etc/services.d/worker.toml="$root/tests/fixtures/fault/worker.toml" \
        /etc/services.d/client.toml="$2" \
        /etc/svcd.conf="$root/tests/fixtures/fault/svcd.conf"
}

initfs "$out/initfs.img" "$root/tests/fixtures/fault/client.toml"
initfs "$out/initfs-restart.img" "$root/tests/fixtures/fault/restart-client.toml"
