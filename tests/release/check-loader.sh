#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K131: a release initfs/image build must replace a stale dynamic loader.
#
# Plants a non-ELF `build/x86_64/ld-elf.so.1` with no manifest, runs the release initfs build
# (the same `ensure_prerequisites` path the image uses), then asserts the loader was rebuilt from
# the current sources and that the manifest records the supported base, interpreter and hash.
# The previous loader is restored only when the build fails.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
cd "$root"

arch=x86_64
loader="build/$arch/ld-elf.so.1"
manifest="build/$arch/loader-manifest.toml"
initfs="build/$arch/initfs.img"

backup=$(mktemp "${TMPDIR:-/tmp}/k131-loader.XXXXXX")
had_loader=0
if [ -f "$loader" ]; then
    cp "$loader" "$backup"
    had_loader=1
fi
cleanup() {
    status=$?
    if [ "$status" -ne 0 ] && [ "$had_loader" -eq 1 ]; then
        cp "$backup" "$loader"
    fi
    rm -f "$backup"
}
trap cleanup EXIT

fail() {
    echo "release-loader-test: $*" >&2
    exit 1
}

printf 'STALE-NOT-ELF\n' >"$loader"
rm -f "$manifest"

echo "release-loader-test: planted stale loader ($(wc -c <"$loader") bytes), building release initfs"
cargo xtask initfs --arch "$arch" --profile release >/dev/null

magic=$(od -An -tx1 -N4 "$loader" | tr -d ' \n')
[ "$magic" = "7f454c46" ] || fail "stale loader was not replaced (magic=$magic)"

base=$(readelf -lW "$loader" | awk '$1 == "LOAD" { print $3; exit }')
[ "$base" = "0x0000000000001000" ] || fail "loader first PT_LOAD is $base, expected 0x1000"

readelf -dW "$loader" | grep -q 'Library soname: \[ld-elf.so.1\]' ||
    fail "loader SONAME is not ld-elf.so.1"

grep -q '^image_base = "0x1000"$' "$manifest" ||
    fail "manifest does not record image_base 0x1000"
grep -q '^interpreter = "/libexec/ld-elf.so.1"$' "$manifest" ||
    fail "manifest does not record /libexec/ld-elf.so.1"

sha=$(sha256sum "$loader" | cut -d' ' -f1)
grep -q "^sha256 = \"$sha\"$" "$manifest" ||
    fail "manifest sha256 does not match the rebuilt loader"

size=$(wc -c <"$loader" | tr -d ' ')
packed=$(cargo run -q -p raven-initfs --features std --example lsinitfs -- "$initfs" |
    awk '$1 == "/libexec/ld-elf.so.1" { print $2 }')
[ "$packed" = "$size" ] ||
    fail "initfs loader size is $packed, fresh loader is $size"

echo "release-loader-test: PASS base=$base sha256=$sha"
