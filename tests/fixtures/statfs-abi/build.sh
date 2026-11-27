#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S775 statfs target probe against the packed ravenlibc archive and pack it as /init.
# The probe calls libc statfs(2) with the uutils record, so a 120-byte record overruns the frame.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    printf 'statfs-abi fixture: x86_64 only\n' >&2
    exit 77
fi

archive=$root/build/x86_64/libc.a
if [ ! -f "$archive" ]; then
    printf 'statfs-abi fixture: missing %s; run cargo xtask libc-pack --arch x86_64\n' "$archive" >&2
    exit 2
fi

out=$root/build/fixtures/statfs-abi
mkdir -p "$out"

rustc +ravenbsd --target x86_64-unknown-ravenbsd --edition 2024 -O \
    -C panic=abort -C relocation-model=static -C link-arg=-no-pie \
    -C link-arg=-T"$root/tests/libc/std-link/std-link.ld" \
    "$root/tests/fixtures/statfs-abi/probe.rs" -o "$out/probe" \
    -C link-arg="$root/build/x86_64/crt1.o" \
    -C link-arg="$root/build/x86_64/crti.o" \
    -C link-arg="-L$root/build/x86_64" \
    -C link-arg=-lc \
    -C link-arg="$root/build/x86_64/crtn.o"

# The probe runs from the shell image: statfs(2) needs vfsd and the filesystem servers, which a
# bare /init cannot provide. Rebuild the shell-test initfs with /bin/statfs-abi added.
cargo xtask initfs --arch x86_64 --profile shell-test >/dev/null
cargo run -q --manifest-path "$root/tests/fixtures/statfs-abi/Cargo.toml" \
    --bin statfs-initfs-add -- \
    "$root/build/x86_64/initfs.img" \
    "$out/initfs.img" \
    /bin/statfs-abi "$out/probe" 0755
