#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Stage the target `raven` admin binary for the modern-admin profile. The binary is the std CLI
# linked against the packed ravenlibc archive and the C64d crt objects; it is installed at
# `/bin/raven` by profiles/raven-admin-test.d/20-raven.toml. The K74 host/stub binary is never
# used here.
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${1:-x86_64}
if [ "$arch" != x86_64 ]; then
    printf 'modern-admin: only x86_64 is in K78b\n' >&2
    exit 2
fi

archive=$root/build/x86_64/libc.a
if [ ! -f "$archive" ]; then
    printf 'modern-admin: missing %s; run cargo xtask libc-pack --arch x86_64\n' "$archive" >&2
    exit 2
fi

out=$root/build/fixtures/modern-admin
mkdir -p "$out"
target_dir=$root/build/cargo/modern-admin

RUSTUP_TOOLCHAIN=ravenbsd cargo rustc -q -p raven --target x86_64-unknown-ravenbsd \
    --target-dir "$target_dir" -- \
    -C "link-arg=$root/build/x86_64/crt1.o" \
    -C "link-arg=$root/build/x86_64/crti.o" \
    -C "link-arg=-L$root/build/x86_64" \
    -C "link-arg=-lc" \
    -C "link-arg=$root/build/x86_64/crtn.o"

binary=$target_dir/x86_64-unknown-ravenbsd/debug/raven
if [ ! -f "$binary" ]; then
    printf 'modern-admin: cargo did not produce %s\n' "$binary" >&2
    exit 1
fi
cp "$binary" "$out/raven"
printf 'modern-admin: %s\n' "$out/raven"
