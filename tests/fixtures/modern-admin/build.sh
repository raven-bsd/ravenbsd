#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Stage the target `raven` admin binary for the modern-admin profile. The binary is the std CLI
# linked against the packed ravenlibc archive and the C64d crt objects; it is installed at
# `/usr/sbin/raven` by profiles/raven-admin-test.d/20-raven.toml. The K74 host/stub binary is never
# used here. The debug user programs nearly fill the 64 MiB ESP, so the admin binary is stripped.
#
# The target dir is persistent for incremental builds, but it must not inherit an artifact set
# built from different manifests/lock/toolchain: a stale `raven_theme` rmeta without the config
# feature's serde rlib produced E0463 in a release initfs prebuild (K548 rev 337). The input
# stamp discards such a dir, and a cache-signature failure rebuilds it clean once, keeping the
# first failure in the log.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
case "$arch" in
    x86_64)
        target=x86_64-unknown-ravenbsd
        linker=$root/tests/fixtures/modern-admin/raven.ld
        ;;
    aarch64)
        target=aarch64-unknown-ravenbsd
        # The std layout is arch-independent; it keeps .eh_frame/.debug_gdb_scripts that a
        # no_std userland script discards (the aarch64 std binary needs them).
        linker=$root/tests/fixtures/modern-admin/raven.ld
        ;;
    *)
        printf 'modern-admin: unsupported arch %s (x86_64 or aarch64)\n' "$arch" >&2
        exit 2
        ;;
esac

archive=$root/build/$arch/libc.a
if [ ! -f "$archive" ]; then
    printf 'modern-admin: missing %s; run cargo xtask libc-pack --arch %s\n' "$archive" "$arch" >&2
    exit 2
fi

out=$root/build/fixtures/modern-admin/$arch
mkdir -p "$out"
target_dir=$root/build/cargo/modern-admin
stamp=$target_dir/.inputs
log=$out/build.log

inputs=$({ cat "$root/Cargo.lock" "$root/rust-toolchain.toml" \
    "$root/crates/raven-theme/Cargo.toml" \
    "$root/crates/raven-cli/Cargo.toml" \
    "$root/userland/bin/raven/Cargo.toml"; \
    printf 'target=%s\n' "$target"; } | sha256sum | cut -d' ' -f1)

if [ -d "$target_dir" ] && { [ ! -f "$stamp" ] || [ "$(cat "$stamp")" != "$inputs" ]; }; then
    printf 'modern-admin: discarding the target dir built from different inputs (%s)\n' \
        "$target_dir" >&2
    rm -rf "$target_dir"
fi
mkdir -p "$target_dir"

build() {
    RUSTUP_TOOLCHAIN=ravenbsd cargo rustc -q -p raven --target "$target" \
        --target-dir "$target_dir" -- \
        -C relocation-model=static \
        -C link-arg=-no-pie \
        -C "link-arg=-T$linker" \
        -C "link-arg=$root/build/$arch/crt1.o" \
        -C "link-arg=$root/build/$arch/crti.o" \
        -C "link-arg=-L$root/build/$arch" \
        -C "link-arg=-lc" \
        -C "link-arg=$root/build/$arch/crtn.o" \
        -C strip=symbols \
        ;
}

if ! build >"$log" 2>&1; then
    if grep -qE "can.t find crate|E0463" "$log"; then
        printf 'modern-admin: stale target cache (%s); retrying with a clean target dir\n' \
            "$target_dir" >&2
        cat "$log" >&2
        rm -rf "$target_dir"
        mkdir -p "$target_dir"
        if ! build >"$log" 2>&1; then
            cat "$log" >&2
            exit 1
        fi
    else
        cat "$log" >&2
        exit 1
    fi
fi
printf '%s\n' "$inputs" >"$stamp"

binary=$target_dir/$target/debug/raven
if [ ! -f "$binary" ]; then
    printf 'modern-admin: cargo did not produce %s\n' "$binary" >&2
    exit 1
fi
cp "$binary" "$out/raven"
# The admin-test and qemu-ufs profiles still stage the arch-less path; keep x86_64 working there.
if [ "$arch" = x86_64 ]; then
    cp "$binary" "$root/build/fixtures/modern-admin/raven"
fi
printf 'modern-admin: %s\n' "$out/raven"
