#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S70 service-restart fixture and pack build/fixtures/service-restart/initfs.img.
# The image is self-contained: raven-init is PID 1, the fixture binary provides the restartable
# `rst` scheme worker and the reconnecting client service.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/service-restart
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
        echo "service-restart fixture: unsupported architecture $arch" >&2
        exit 1
        ;;
esac

RUSTFLAGS="$rustflags" \
    cargo build -p raven-init -p raven-service-restart --target "$target" --release

target_dir=${CARGO_TARGET_DIR:-$root/target}
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-init" \
    /sbin/srrestart="$target_dir/$target/release/raven-service-restart" \
    /etc/services.d/worker.toml="$root/tests/fixtures/service-restart/worker.toml" \
    /etc/services.d/client.toml="$root/tests/fixtures/service-restart/client.toml" \
    /etc/svcd.conf="$root/tests/fixtures/service-restart/svcd.conf"
