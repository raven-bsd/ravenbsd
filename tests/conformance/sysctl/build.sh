#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S744: build the sysctl conformance probe against the current ravenlibc and stage a one-shot
# service over the release initfs. `RAVEN_QT_ARCH` selects x86_64 (default) or aarch64.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

case_name=sysctl
out=$root/build/conformance/$case_name
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        rustflags="-C relocation-model=static -C code-model=small -C force-frame-pointers=yes"
        linker=$root/userland/linker/x86_64.ld
        ;;
    aarch64)
        target=aarch64-unknown-none-softfloat
        rustflags="-C relocation-model=static -C force-frame-pointers=yes"
        linker=$root/userland/linker/aarch64.ld
        ;;
    *)
        echo "sysctl-conformance: unsupported arch $arch" >&2
        exit 2
        ;;
esac
sysroot=$root/build/$arch/sysroot

libc_archive=$sysroot/usr/lib/libc.a
if [ ! -f "$libc_archive" ] || [ -n "$(find "$root/libc/ravenlibc/crates" \
    "$root/libc/ravenlibc/abi" "$root/libc/ravenlibc/crt" \
    "$root/libc/ravenlibc/Cargo.toml" -newer "$libc_archive" -print -quit 2>/dev/null)" ]; then
    cargo xtask libc-pack --arch "$arch"
fi
for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "sysctl-conformance: missing $sysroot/usr/lib/$file" >&2
        exit 1
    }
done
[ -f "$linker" ] || {
    echo "sysctl-conformance: missing linker script $linker" >&2
    exit 1
}

probe=$out/conformance-probe
# shellcheck disable=SC2086
rustc --target "$target" --edition 2024 -O -C panic=abort $rustflags \
    -C link-arg=-static -C link-arg=-T"$linker" \
    -C link-arg="$sysroot/usr/lib/crt1.o" \
    -C link-arg="$sysroot/usr/lib/crti.o" \
    -C link-arg="$sysroot/usr/lib/libc.a" \
    -C link-arg="$sysroot/usr/lib/crtn.o" \
    "$root/tests/conformance/$case_name/sysctl-conformance.rs" -o "$probe"
# The interim 4 KiB ELF shape gate is the x86_64 PG-53 rule; the aarch64 linker script uses the
# 64 KiB segment alignment the aarch64 loader expects, so the check does not apply there.
if [ "$arch" = x86_64 ]; then
    sh "$root/tests/libc/target/shape-check.sh" "$probe"
fi

base=$root/build/$arch/initfs.img
cargo xtask initfs --arch "$arch" --profile base
cargo build -q -p raven-b70-overlay
overlay=${CARGO_TARGET_DIR:-$root/target}/debug/raven-b70-overlay
set -- \
    "/bin/conformance-probe=$probe:0755" \
    "/etc/services.d/sysctl-conformance.toml=$root/tests/conformance/$case_name/sysctl-conformance.service.toml:0644" \
    "/etc/svcd.conf=$root/tests/conformance/$case_name/sysctl-conformance.svcd.conf:0644" \
    "/sbin/logd=$root/build/$arch/user/raven-logd:0755" \
    "/etc/services.d/logd.toml=$root/etc/services.d/logd.toml:0644" \
    "/sbin/sysd=$root/build/$arch/user/raven-sysd:0755" \
    "/etc/services.d/sysd.toml=$root/etc/services.d/sysd.toml:0644" \
    "/sbin/randd=$root/build/$arch/user/raven-randd:0755" \
    "/etc/services.d/randd.toml=$root/etc/services.d/randd.toml:0644" \
    "/sbin/zerod=$root/build/$arch/user/raven-zerod:0755" \
    "/etc/services.d/zerod.toml=$root/etc/services.d/zerod.toml:0644" \
    "/sbin/nulld=$root/build/$arch/user/raven-nulld:0755" \
    "/etc/services.d/nulld.toml=$root/etc/services.d/nulld.toml:0644" \
    "/sbin/initfsd=$root/build/$arch/user/raven-initfsd:0755" \
    "/etc/services.d/initfsd.toml=$root/etc/services.d/initfsd.toml:0644" \
    "/sbin/ramfsd=$root/build/$arch/user/raven-ramfsd:0755" \
    "/etc/services.d/ramfsd.toml=$root/etc/services.d/ramfsd.toml:0644" \
    "/sbin/vfsd=$root/build/$arch/user/raven-vfsd:0755" \
    "/etc/services.d/vfsd.toml=$root/etc/services.d/vfsd.toml:0644" \
    "/sbin/consd=$root/build/$arch/user/raven-consd:0755" \
    "/etc/services.d/consd.toml=$root/etc/services.d/consd.toml:0644" \
    "/etc/mounts=$root/etc/mounts:0644"
"$overlay" "$base" "$out/initfs.img" "$@"
echo "sysctl-conformance: staged $out/initfs.img ($arch)"
