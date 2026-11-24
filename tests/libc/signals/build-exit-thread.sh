#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S194: build the two-thread exit/quiescence experiment into build/libc-test/libc-test.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${1:-x86_64}
[ "$arch" = x86_64 ] || { printf 's194-exit-quiesce: only x86_64\n' >&2; exit 2; }

out=$root/build/libc-test
mkdir -p "$out"
target_dir=$root/build/libc/signals/exit-thread-target
crt=$root/build/x86_64

RUSTUP_TOOLCHAIN=ravenbsd CARGO_TARGET_DIR="$target_dir" \
RUSTFLAGS="--cfg getrandom_backend=\"custom\" -C strip=symbols -C relocation-model=static -C code-model=small \
-C link-arg=-T$root/tests/libc/tls/x86_64.ld \
-C link-arg=$crt/crt1.o -C link-arg=$crt/crti.o -C link-arg=-L$crt -C link-arg=-lc -C link-arg=$crt/crtn.o" \
    cargo build --manifest-path "$root/tests/libc/signals/exit-thread-quiesce/Cargo.toml" \
        --target x86_64-unknown-ravenbsd --release

cp "$target_dir/x86_64-unknown-ravenbsd/release/s194-exit-quiesce" "$out/libc-test"
: > "$out/libc-child"
: > "$out/libplug.so"
: > "$out/libc.so.1"
: > "$out/ld-elf.so.1"
printf 'exit-thread-quiesce\n' > "$out/stamp"
sh "$root/tests/libc/target/shape-check.sh" "$out/libc-test"

# Overlay the probe and the one-shot svcd policy on the libc-test initfs. The image lifetime is
# the probe's: init exits with its status, which is the K445 init-exit contract at 4 vCPUs.
base=$root/build/x86_64/initfs.img
[ -f "$base" ] || cargo xtask initfs --arch x86_64 --profile libc-test
fixture=$root/build/fixtures/exit-thread-quiesce
mkdir -p "$fixture"
cargo build -q -p raven-b70-overlay
"$root/target/debug/raven-b70-overlay" "$base" "$fixture/initfs.img" \
    "/bin/libc-test=$out/libc-test:0755" \
    "/etc/svcd.conf=$root/tests/libc/signals/exit-thread-quiesce.svcd.conf:0644"
printf 's194-exit-quiesce: staged %s and %s\n' "$out/libc-test" "$fixture/initfs.img"
