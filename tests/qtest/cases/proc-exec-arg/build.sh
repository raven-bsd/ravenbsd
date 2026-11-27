#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S168b C8 exec argument-register fixture: /init execs /arg-child, whose custom
# `_start` reports the argument register and stack pointer set by the kernel exec path.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

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
        echo "proc-exec-arg fixture: unsupported architecture $arch" >&2
        exit 2
        ;;
esac

out=build/fixtures/proc-exec-arg/$arch
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="$rustflags" \
    cargo build --manifest-path tests/qtest/cases/proc-exec-arg/Cargo.toml \
        --target "$target" --target-dir "$target_dir" --release \
        --bin arg_init --bin arg_child

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/arg.img" \
    "/init=$target_dir/$target/release/arg_init:0755" \
    "/arg-child=$target_dir/$target/release/arg_child:0755"
