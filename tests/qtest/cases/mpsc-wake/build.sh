#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K155 std mpsc/condvar fixture, build the shell-test initfs, and add the fixture at
# /bin/mpsc-wake so it runs as a normal shell child with valid std fds.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

out=build/fixtures/mpsc-wake
target=build/cargo/mpsc-wake
manifest=tests/qtest/cases/mpsc-wake/Cargo.toml
mkdir -p "$out"
RUSTUP_TOOLCHAIN=ravenbsd RUSTFLAGS="--cfg getrandom_backend=\"custom\" -C strip=symbols -C relocation-model=static -C code-model=small \
-C link-arg=$root/build/x86_64/crt1.o -C link-arg=$root/build/x86_64/crti.o \
-C link-arg=$root/build/x86_64/libc.a -C link-arg=$root/build/x86_64/crtn.o \
-C link-arg=-T$root/userland/uutils/x86_64.ld" \
    cargo build --manifest-path "$manifest" --target x86_64-unknown-ravenbsd \
        --release --target-dir "$target"
cargo xtask initfs --arch x86_64 --profile shell-test >/dev/null
cargo run --quiet --manifest-path tests/qtest/cases/mpsc-wake/image/Cargo.toml \
    --target-dir build/cargo/mpsc-wake-image --bin mpsc-wake-image -- \
    build/x86_64/initfs.img "$out/shell-test-initfs.img" \
    "/bin/mpsc-wake=$target/x86_64-unknown-ravenbsd/release/mpsc-wake"
