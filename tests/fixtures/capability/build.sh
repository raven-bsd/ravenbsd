#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K529 global-authority fixture.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/capability
mkdir -p "$out"
target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"

RUSTFLAGS="$rustflags" \
    cargo build -p raven-capability-authority --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-capability-authority"

printf 'capability\n' > "$out/stamp"
printf 'capability fixture: staged %s\n' "$out/initfs.img"
