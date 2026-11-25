#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the signal acceptance fixtures and stage them as one-shot svcd services over the arch's
# release initfs. Test-only; no product source is changed.
#
# RAVEN_QT_ARCH selects x86_64 (default) or aarch64. Each architecture gets its own output
# directory. The boundary image runs the Rust public-libc sigaction/sigprocmask rows without the
# C libc raise path, which is owned separately. The x86_64 acceptance image is also kept at the
# historical build/signals/initfs.img path used by process-create.toml.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        c_target=x86_64-unknown-ravenbsd
        rust_target=x86_64-unknown-none
        rust_code_model="-C code-model=small"
        linker_script=$root/userland/linker/x86_64.ld
        fp_flags="-mno-sse -mno-mmx -msoft-float"
        linker_flavor=
        ;;
    aarch64)
        c_target=aarch64-unknown-ravenbsd
        rust_target=aarch64-unknown-none-softfloat
        rust_code_model=
        linker_script=$root/userland/linker/aarch64.ld
        fp_flags=-mgeneral-regs-only
        linker_flavor="-Wl,-m,aarch64linux"
        ;;
    *)
        echo "signals: unsupported architecture $arch" >&2
        exit 2
        ;;
esac

out=$root/build/signals/$arch
mkdir -p "$out"
sysroot=$root/build/$arch/sysroot
# Rebuild libc and the fixtures together: never trust a stale sysroot or a pre-existing initfs.
cargo xtask libc-pack --arch "$arch"
for file in crt1.o crti.o crtn.o libc.a; do
    [ -f "$sysroot/usr/lib/$file" ] || {
        echo "signals: missing $sysroot/usr/lib/$file; run cargo xtask libc-pack --arch $arch" >&2
        exit 1
    }
done

# Public-libc boundary rows: Rust extern "C" calls linked against the freshly packed archive,
# in the tests/libc/std-link pattern. Guard pages, canaries and the flags/mask readback stay in
# the Rust source, so the C acceptance fixture is untouched.
#
# S712 exec probe: the new-image program is built first and embedded into the libc fixture with
# `include_bytes!(env!("RAVEN_EXEC_PROBE"))`; exec therefore never needs the file scheme.
# shellcheck disable=SC2086
RUSTFLAGS="-C relocation-model=static -C lto=no $rust_code_model -C link-arg=-T$linker_script" \
    cargo build -q -p raven-s44-action --bin raven-s44-exec-probe \
    --target "$rust_target" --release
probe=${CARGO_TARGET_DIR:-$root/target}/$rust_target/release/raven-s44-exec-probe
[ -f "$probe" ] || {
    echo "signals: missing exec probe $probe" >&2
    exit 1
}
RAVEN_EXEC_PROBE=$probe
export RAVEN_EXEC_PROBE

# shellcheck disable=SC2086
rustc --target "$rust_target" --edition 2024 -O -C panic=abort \
    -C relocation-model=static $rust_code_model \
    -C link-arg=-static -C link-arg=-T"$linker_script" \
    -C link-arg="$sysroot/usr/lib/crt1.o" \
    -C link-arg="$sysroot/usr/lib/crti.o" \
    -C link-arg="$sysroot/usr/lib/libc.a" \
    -C link-arg="$sysroot/usr/lib/crtn.o" \
    "$root/tests/signals/signal_libc.rs" -o "$out/signal-libc"

if [ "$arch" = x86_64 ]; then
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
    if ! command -v ld.lld >/dev/null 2>&1; then
        echo "signals: ld.lld not found" >&2
        exit 77
    fi

    "$CC" --target="$c_target" --sysroot="$sysroot" \
        -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
        -D__ravenbsd__=1 -D__unix__=1 -D__BSD_VISIBLE=1 $fp_flags \
        -c "$root/tests/signals/signal_accept.c" -o "$out/signal-acceptance.o"

    builtins=
    if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
        builtins=1
    fi
    if [ -n "$builtins" ]; then
        extra_libs="-lclang_rt.builtins"
    else
        extra_libs=
    fi
    # shellcheck disable=SC2086
    "$CC" --target="$c_target" --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
        -nostdlib -nostartfiles -static -T"$linker_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/signal-acceptance.o" \
        -L"$sysroot/usr/lib" -lc $extra_libs "$sysroot/usr/lib/crtn.o" \
        -o "$out/signal-acceptance"
fi

base=$root/build/$arch/initfs.img
[ -f "$base" ] || cargo xtask initfs --arch "$arch" --profile release
cargo build -q -p raven-b70-overlay
overlay=${CARGO_TARGET_DIR:-$root/target}/debug/raven-b70-overlay

"$overlay" "$base" "$out/boundary-initfs.img" \
    "/bin/signal-libc=$out/signal-libc:0755" \
    "/etc/services.d/signal-boundary.toml=$root/tests/signals/signal-boundary.service.toml:0644" \
    "/etc/svcd.conf=$root/tests/signals/signal-boundary.svcd.conf:0644"

if [ "$arch" = x86_64 ]; then
    "$overlay" "$base" "$out/initfs.img" \
        "/bin/signal-acceptance=$out/signal-acceptance:0755" \
        "/etc/services.d/signal-acceptance.toml=$root/tests/signals/signal-acceptance.service.toml:0644" \
        "/etc/svcd.conf=$root/tests/signals/signal.svcd.conf:0644"
    # process-create.toml still names this historical path; keep it an acceptance image.
    cp "$out/initfs.img" "$root/build/signals/initfs.img"
fi
echo "signals: staged $arch at $out"
