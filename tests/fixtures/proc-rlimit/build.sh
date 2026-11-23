#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K491 rlimit fixture: a raw-syscall init and a libc-linked NOFILE child.
#   initfs.img -> /init = proc-rlimit-init, /nofile = proc-rlimit-nofile

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
        ;;
    *)
        printf 'proc-rlimit fixture: unsupported architecture %s\n' "$arch" >&2
        exit 77
        ;;
esac

out=build/fixtures/proc-rlimit
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-target}

RUSTFLAGS="$rustflags" \
    cargo build -p raven-proc-rlimit --target "$target" --release

sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
. "$root/tests/fixtures/libc-sysroot.sh"
ensure_libc_sysroot "$root" "$arch" "$sysroot" proc-rlimit

CC=${CC:-clang}
if ! command -v "$CC" >/dev/null 2>&1; then
    printf 'proc-rlimit fixture: %s not found\n' "$CC" >&2
    exit 77
fi
if ! command -v ld.lld >/dev/null 2>&1; then
    host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p' || true)
    if [ -n "$host" ]; then
        lld_dir=$(rustc --print sysroot 2>/dev/null)/lib/rustlib/$host/bin/gcc-ld
        if [ -x "$lld_dir/ld.lld" ]; then
            PATH="$lld_dir:$PATH"
            export PATH
        fi
    fi
fi
if ! command -v ld.lld >/dev/null 2>&1; then
    printf 'proc-rlimit fixture: ld.lld not found\n' >&2
    exit 77
fi

builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi

"$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" \
    -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
    -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
    -c "$root/tests/fixtures/proc-rlimit/src/nofile.c" \
    -o "$out/nofile.o"

if [ -n "$builtins" ]; then
    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/nofile.o" \
        -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/proc-rlimit-nofile"
else
    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld \
        -nostdlib -nostartfiles -static -T"$root/tests/libc/target/x86_64.ld" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/nofile.o" \
        -L"$sysroot/usr/lib" -lc \
        "$sysroot/usr/lib/crtn.o" -o "$out/proc-rlimit-nofile"
fi

cargo xtask initfs --arch "$arch" --profile core-servers-files > /dev/null
cp "build/$arch/initfs.img" "$out/base.img"

cargo build -q -p raven-b70-overlay
overlay=$root/target/debug/raven-b70-overlay
"$overlay" "$out/base.img" "$out/initfs.img" \
    "/bin/proc-rlimit=$target_dir/$target/release/proc-rlimit-init:0755" \
    "/bin/proc-rlimit-nofile=$out/proc-rlimit-nofile:0755" \
    "/etc/services.d/proc-rlimit.toml=$root/tests/fixtures/proc-rlimit/service.toml:0644" \
    "/etc/svcd.conf=$root/tests/fixtures/proc-rlimit/svcd.conf:0644"

printf 'proc-rlimit\n' > "$out/stamp"
printf 'proc-rlimit fixture: staged %s\n' "$out/initfs.img"
