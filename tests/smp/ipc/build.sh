#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S85c SMP IPC stress fixture and pack build/smp/ipc/initfs.img.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    printf 'skipped: smp-ipc is x86_64 only in S85c\n'
    exit 77
fi

out=build/smp/ipc
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-$root/target}

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build --manifest-path "$root/tests/smp/ipc/Cargo.toml" \
    --target-dir "$target_dir" --target x86_64-unknown-none --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/x86_64-unknown-none/release/raven-smp-ipc-init:0755"

# The fixture is a standalone package; its generated lockfile is not part of the tree.
rm -f "$root/tests/smp/ipc/Cargo.lock"
