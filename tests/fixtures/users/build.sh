#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K229: build the stub TUI shell and the writable UFS disk for the raven-users qtest.
# The one-disk demo qcow2 is built by scripts/qemu-ufs.sh, converted to raw, injected with the
# stub through scripts/raven-ufs, and converted back so the test starts from a private copy.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
cd "$root"

arch=x86_64
out=build/fixtures/users
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-$root/target}

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    CARGO_TARGET_DIR="$target_dir" \
    cargo build --manifest-path tests/fixtures/users/tui-demo/Cargo.toml \
    --target x86_64-unknown-none --release

stub=$target_dir/x86_64-unknown-none/release/tui-demo
[ -f "$stub" ] || {
    printf 'raven-users: missing stub %s\n' "$stub" >&2
    exit 1
}

sh tests/fixtures/modern-admin/build.sh "$arch" >/dev/null
demo=$(sh scripts/qemu-ufs.sh --build-only | tail -n 1)
raw=$out/raven-users.raw
qcow2=$out/raven-users.qcow2
qemu-img convert -O raw "$demo" "$raw"
sh scripts/raven-ufs mkdir /home/raven "$raw"
sh scripts/raven-ufs put "$stub" /bin/tui-demo "$raw"
qemu-img convert -O qcow2 "$raw" "$qcow2"
rm -f "$raw"
printf '%s\n' "$qcow2"
