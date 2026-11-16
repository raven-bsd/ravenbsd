#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the P2f process/signal fixtures. Each qtest gets its own initfs:
#   signal.img   -> /init = proc-signal-init
#   session.img  -> /init = proc-session-init

set -eu

arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "proc primitives fixture is x86_64-only (requested $arch)" >&2
    exit 2
fi

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/proc
mkdir -p "$out"
target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"

RUSTFLAGS="$rustflags" \
    cargo build -p raven-proc-primitives --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/signal.img" \
    "/init=$target_dir/$target/release/proc-signal-init:0755"

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/session.img" \
    "/init=$target_dir/$target/release/proc-session-init:0755"
