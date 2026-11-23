#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K548 std spawn probe and pack build/fixtures/std-spawn/initfs.img.
#
# The probe is built with the same static libc + std link as brush (ravenbsd target, userland
# linker, build/x86_64/libc.a) so it can separate a std/libc initialization defect from a
# brush-specific one. It runs as a one-shot service, not through the shell.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd -P)
cd "$root"

out=build/fixtures/std-spawn
mkdir -p "$out"

target=x86_64-unknown-ravenbsd
target_dir=build/cargo/std-spawn
RUSTFLAGS="--cfg getrandom_backend=\"custom\" -C strip=symbols -C relocation-model=static -C code-model=small -C link-arg=-T$root/xtask/brush-x86_64.ld -C link-arg=$root/build/x86_64/crt1.o -C link-arg=$root/build/x86_64/crti.o -C link-arg=-L$root/build/x86_64 -C link-arg=-lc -C link-arg=$root/build/x86_64/crtn.o -C link-arg=-Map=$root/build/fixtures/std-spawn/probe.map" \
    cargo +ravenbsd build --manifest-path tests/fixtures/std-spawn/Cargo.toml \
    --bin std-spawn --target "$target" --target-dir "$target_dir" --release

probe=$root/$target_dir/$target/release/std-spawn
[ -f "$probe" ] || {
    echo "std-spawn: missing probe $probe" >&2
    exit 1
}

# A base release initfs is needed for vfsd's file scheme (the probe is exec'd from it). The
# probe service does not use the shell, so a base whose staged brush predates the spawn fix is
# fine; this experiment tests std, not brush.
base=build/x86_64/initfs.img
if ! strings "$base" 2>/dev/null | grep -q '/sbin/vfsd'; then
    cargo xtask initfs --arch x86_64 --profile release
    base=build/x86_64/initfs.img
fi
cargo build -q -p raven-b70-overlay
overlay=${CARGO_TARGET_DIR:-target}/debug/raven-b70-overlay
"$overlay" "$base" "$out/initfs.img" \
    "/bin/std-spawn=$probe:0755" \
    "/etc/services.d/std-spawn.toml=$root/tests/fixtures/std-spawn/std-spawn.service.toml:0644" \
    "/etc/svcd.conf=$root/tests/fixtures/std-spawn/std-spawn.svcd.conf:0644"
