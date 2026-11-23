#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the P2f process/signal fixtures. Each qtest gets its own initfs:
#   signal.img   -> /init = proc-signal-init
#   session.img  -> /init = proc-session-init
#   exec.img     -> /init = proc-exec-init, /exec-child = proc-exec-child
#   priority.img -> /init = proc-priority-init

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
        ;;
    aarch64)
        target=aarch64-unknown-none-softfloat
        rustflags="-C relocation-model=static -C force-frame-pointers=yes -C link-arg=-T$root/userland/linker/aarch64.ld"
        ;;
    *)
        echo "proc primitives fixture: unsupported architecture $arch" >&2
        exit 2
        ;;
esac

out=build/fixtures/proc
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-target}

# K220: build the libc exec-large fixture into the libc-test profile's staging paths.
build_exec_large() {
    if [ "$arch" != x86_64 ]; then
        echo "exec-large fixture: x86_64 only" >&2
        exit 77
    fi
    sysroot=${RAVENLIBC_SYSROOT:-$root/build/$arch/sysroot}
    . "$root/tests/fixtures/libc-sysroot.sh"
    ensure_libc_sysroot "$root" "$arch" "$sysroot" exec-large
    CC=${CC:-clang}
    if ! command -v "$CC" >/dev/null 2>&1; then
        echo "exec-large fixture: $CC not found" >&2
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
        echo "exec-large fixture: ld.lld not found" >&2
        exit 77
    fi
    stage=$root/build/libc-test
    mkdir -p "$stage/obj"
    link_script=$root/tests/libc/target/x86_64.ld
    builtins=
    if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
        builtins=1
    fi
    compile() {
        "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
            -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
            -D__ravenbsd__=1 -D__unix__=1 -mno-sse -mno-mmx -msoft-float \
            -c "$1" -o "$2"
    }
    link() {
        if [ -n "$builtins" ]; then
            "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
                -nostdlib -nostartfiles -static -T"$link_script" \
                -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
                "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$1" \
                -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
                "$sysroot/usr/lib/crtn.o" -o "$2"
        else
            "$CC" --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
                -nostdlib -nostartfiles -static -T"$link_script" \
                -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
                "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$1" \
                -L"$sysroot/usr/lib" -lc \
                "$sysroot/usr/lib/crtn.o" -o "$2"
        fi
    }
    compile "$root/tests/fixtures/proc/exec-large-init.c" "$stage/obj/exec-large-init.o"
    link "$stage/obj/exec-large-init.o" "$stage/libc-test"
    compile "$root/tests/fixtures/proc/exec-large-child.c" "$stage/obj/exec-large-child.o"
    link "$stage/obj/exec-large-child.o" "$stage/libc-child"
    : > "$stage/libplug.so"
    : > "$stage/libc.so.1"
    : > "$stage/ld-elf.so.1"
    printf 'exec-large fixture: %s %s\n' "$stage/libc-test" "$stage/libc-child"
    ls -l "$stage/libc-test" "$stage/libc-child"
}

if [ "${1:-}" = exec-large ]; then
    build_exec_large
    exit 0
fi

RUSTFLAGS="$rustflags" \
    cargo build -p raven-proc-primitives --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/signal.img" \
    "/init=$target_dir/$target/release/proc-signal-init:0755"

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/session.img" \
    "/init=$target_dir/$target/release/proc-session-init:0755"

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/exec.img" \
    "/init=$target_dir/$target/release/proc-exec-init:0755" \
    "/exec-child=$target_dir/$target/release/proc-exec-child:0755"

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/priority.img" \
    "/init=$target_dir/$target/release/proc-priority-init:0755"

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/wait.img" \
    "/init=$target_dir/$target/release/proc-wait-init:0755"
