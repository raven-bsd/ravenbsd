#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S185 C19 signal acceptance fixture and stage it as a one-shot svcd service over the
# release initfs. Test-only; no product source is changed.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

arch=x86_64
out=build/signals
mkdir -p "$out"
sysroot=$root/build/$arch/sysroot
for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "signals: missing $sysroot/usr/lib/$file; run cargo xtask libc-pack --arch $arch" >&2
        exit 1
    }
done

CC=${CC:-clang}
command -v "$CC" >/dev/null 2>&1 || {
    echo "signals: $CC not found" >&2
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
    -D__ravenbsd__=1 -D__unix__=1 -D__BSD_VISIBLE=1 -mno-sse -mno-mmx -msoft-float \
    -c "$root/tests/signals/signal_accept.c" -o "$out/signal-acceptance.o"
"$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
    -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
    -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
    "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/signal-acceptance.o" \
    -L"$sysroot/usr/lib" -lc "$sysroot/usr/lib/crtn.o" -o "$out/signal-acceptance"

base=$root/build/$arch/initfs.img
[ -f "$base" ] || cargo xtask initfs --arch "$arch" --profile release
cargo build -q -p raven-b70-overlay
overlay=${CARGO_TARGET_DIR:-$root/target}/debug/raven-b70-overlay
"$overlay" "$base" "$out/initfs.img" \
    "/bin/signal-acceptance=$out/signal-acceptance:0755" \
    "/etc/services.d/signal-acceptance.toml=$root/tests/signals/signal-acceptance.service.toml:0644" \
    "/etc/svcd.conf=$root/tests/signals/signal.svcd.conf:0644"
echo "signals: staged $out/signal-acceptance and $out/initfs.img"
