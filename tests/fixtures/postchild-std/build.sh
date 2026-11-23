#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K495 rev167 test-only static std fixture. Build the program that
# profiles/release.d/89-postchild-std.toml stages at /bin/postchild-std. It runs one external child,
# then a HashSet/HashMap insert and an environ read, to separate brush-internal corruption from the
# Raven std/exec/environ contract. No product source is changed.
#
# Usage: sh tests/fixtures/postchild-std/build.sh
# Output: build/fixtures/postchild-std/postchild-std

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
out=build/fixtures/postchild-std
mkdir -p "$out"

for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$root/build/$arch/$file" ] || {
        echo "postchild-std: missing build/$arch/$file; run cargo xtask libc-pack --arch $arch" >&2
        exit 1
    }
done
[ -f "$root/xtask/brush-x86_64.ld" ] || {
    echo "postchild-std: missing xtask/brush-x86_64.ld" >&2
    exit 1
}

flags="--cfg getrandom_backend=\"custom\" -C strip=none -C relocation-model=static -C code-model=small \
 -C link-arg=-T$root/xtask/brush-x86_64.ld \
 -C link-arg=$root/build/$arch/crt1.o -C link-arg=$root/build/$arch/crti.o \
 -C link-arg=-L$root/build/$arch -C link-arg=-lc \
 -C link-arg=$root/build/$arch/crtn.o"

RUSTFLAGS="$flags" cargo +ravenbsd build \
    --manifest-path "$root/tests/fixtures/postchild-std/Cargo.toml" \
    --target x86_64-unknown-ravenbsd --release \
    --target-dir "$root/build/cargo/postchild-std"
cp "$root/build/cargo/postchild-std/x86_64-unknown-ravenbsd/release/postchild-std" \
    "$out/postchild-std"
echo "postchild-std fixture: staged $out/postchild-std"
