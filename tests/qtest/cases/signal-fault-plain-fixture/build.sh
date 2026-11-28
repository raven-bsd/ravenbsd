#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S779 plain signal(2) fixtures as no_std Rust programs linked against the generated
# ravenlibc sysroot, and pack one initfs image per mapping variant.
#
# Two images share one source: `signal-fault-plain` uses the donor `mprotect_write` flags
# (`MAP_ANON` with no private/shared type) and `signal-fault-plain-private` adds `MAP_PRIVATE`
# so the plain-handler path is exercised while the donor mapping is unavailable. Test-only
# code; no C source and no product build dependency.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64) ;;
    *)
        echo "signal-fault-plain fixture: unsupported architecture $arch" >&2
        exit 1
        ;;
esac

sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
# K441: a present stamp must match the tree; a stale artifact fails closed here too.
if [ -f "$root/build/$arch/libc.stamp" ]; then
    cargo xtask libc-pack --arch "$arch" --verify || exit 1
fi
libc_archive=$sysroot/usr/lib/libc.a
if [ ! -f "$libc_archive" ] ||
    [ -n "$(find "$root/libc/ravenlibc/crates" "$root/libc/ravenlibc/abi" \
        "$root/libc/ravenlibc/crt" "$root/libc/ravenlibc/Cargo.toml" \
        -newer "$libc_archive" -print -quit 2>/dev/null)" ]; then
    cargo xtask libc-pack --arch "$arch"
fi

for file in crt1.o crti.o crtn.o libc.a; do
    if [ ! -e "$sysroot/usr/lib/$file" ]; then
        echo "signal-fault-plain fixture: missing sysroot file $sysroot/usr/lib/$file" >&2
        exit 77
    fi
done

manifest=tests/qtest/cases/signal-fault-plain-fixture/Cargo.toml
target_dir_base=build/cargo/signal-fault-plain

build() {
    name=$1
    features=$2
    target_dir=$target_dir_base-$name
    out=build/fixtures/$name
    mkdir -p "$out"
    rustflags="-C relocation-model=static -C code-model=small \
-C link-arg=$sysroot/usr/lib/crt1.o \
-C link-arg=$sysroot/usr/lib/crti.o \
-C link-arg=$sysroot/usr/lib/libc.a \
-C link-arg=$sysroot/usr/lib/crtn.o \
-C link-arg=-T$root/tests/libc/target/x86_64.ld"
    # shellcheck disable=SC2086
    RUSTUP_TOOLCHAIN=ravenbsd RUSTFLAGS="$rustflags" \
        cargo build --manifest-path "$manifest" --target x86_64-unknown-ravenbsd \
            --release ${features:+--features "$features"} --target-dir "$target_dir"
    cargo run -q -p raven-initfs --features std --example mkinitfs -- \
        "$out/initfs.img" \
        /init="$target_dir/x86_64-unknown-ravenbsd/release/raven-s779-plain-signal"
}

build signal-fault-plain ""
build signal-fault-plain-private "private-map"
