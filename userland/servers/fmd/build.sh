#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the FMA fixture initfs: /init = fmd-fixture.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "fma fixture: x86_64 only" >&2
    exit 77
fi

out=build/fixtures/fmd
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-$root/userland/servers/fmd/fixture/target}

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    CARGO_TARGET_DIR="$target_dir" \
    cargo build --manifest-path "$root/userland/servers/fmd/fixture/Cargo.toml" \
        --target x86_64-unknown-none --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/ereport.img" \
    "/init=$target_dir/x86_64-unknown-none/release/fmd-fixture:0755"

echo "fma fixture: $out/ereport.img"
