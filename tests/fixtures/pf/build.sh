#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K273 pf-core fixture initfs: /init = pf-core-fixture.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "pf-core fixture: x86_64 only" >&2
    exit 77
fi

out=build/fixtures/pf
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-$root/tests/fixtures/pf/target}

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    CARGO_TARGET_DIR="$target_dir" \
    cargo build --manifest-path "$root/tests/fixtures/pf/Cargo.toml" \
        --target x86_64-unknown-none --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/pf-core.img" \
    "/init=$target_dir/x86_64-unknown-none/release/pf-core-fixture:0755"

echo "pf-core fixture: $out/pf-core.img"
