#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S176 clock conformance probe and stage it over the release initfs. No product source
# is changed; the qtest row is self-contained under tests/conformance/clocks.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
out=build/conformance/clocks
mkdir -p "$out"
sysroot=$root/build/$arch/sysroot
for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "clocks: missing $sysroot/usr/lib/$file; run cargo xtask libc-pack --arch $arch" >&2
        exit 1
    }
done

mutation=0
if [ "${1:-}" = "--mutation" ]; then
    mutation=1
fi
name=clock-conf
extra_cflags=
if [ "$mutation" = 1 ]; then
    name=clock-conf-mutation
    extra_cflags=-DMUTATION_EARLY_SLEEP
fi

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "clocks: $CC not found" >&2
    exit 77
}
if ! command -v ld.lld >/dev/null 2>&1; then
    host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')
    if [ -n "$host" ]; then
        lld_dir=$(rustc --print sysroot 2>/dev/null)/lib/rustlib/$host/bin/gcc-ld
        if [ -x "$lld_dir/ld.lld" ]; then
            PATH="$lld_dir:$PATH"
            export PATH
        fi
    fi
fi

"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float $extra_cflags \
    -c "$root/tests/conformance/clocks/clock_conf.c" -o "$out/$name.o"
"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
    -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
    -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
    "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/$name.o" \
    -L"$sysroot/usr/lib" -lc "$sysroot/usr/lib/crtn.o" -o "$out/$name"

base=$root/build/$arch/initfs.img
[ -f "$base" ] || cargo xtask initfs --arch "$arch" --profile release
cargo build -q -p raven-b70-overlay
overlay=${CARGO_TARGET_DIR:-$root/target}/debug/raven-b70-overlay
image=$out/initfs.img
if [ "$mutation" = 1 ]; then
    image=$out/initfs-mutation.img
fi
"$overlay" "$base" "$image" \
    "/bin/clock-conf=$out/$name:0755" \
    "/etc/services.d/clock-conf.toml=$root/tests/conformance/clocks/clock-conf.service.toml:0644" \
    "/etc/svcd.conf=$root/tests/conformance/clocks/clock.svcd.conf:0644"
echo "clocks: staged $out/$name and $image"
