#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build one C73 on-target libc case into build/libc-test/ (C73 4.4). The script is POSIX sh.
# It refreshes the default generated sysroot when it is missing or older than the libc sources,
# so a C case never links a stale TLS/TCB; a missing PG or tool still exits 77 and the qtest
# records the case as skipped. `RL_FORCE=1` builds a blocked case anyway (it never turns the
# case on; the skip stays in the case file).

set -eu

usage() {
    printf 'usage: %s <case> [<arch>]\n' "$0" >&2
    exit 2
}

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    usage
fi

case_name=${1#libc-}
arch=${RAVEN_QT_ARCH:-${2:-x86_64}}

case $arch in
    x86_64) ;;
    aarch64) ;;
    riscv64)
        printf 'skipped: arch not in this ticket\n'
        exit 77
        ;;
    *) usage ;;
esac

tsv=$root/tests/libc/target/cases.tsv
row=$(awk -F '	' -v want="$case_name" '
    $0 ~ /^#/ || $0 == "" || $1 == "case" { next }
    $1 == want { print; found = 1; exit }
    END { if (found == 0) exit 1 }
' "$tsv") || {
    printf 'error: unknown case %s\n' "$case_name" >&2
    exit 2
}

binaries=$(printf '%s\n' "$row" | cut -f4)
link_kind=$(printf '%s\n' "$row" | cut -f5)

probe_last=$("$root/tests/libc/target/pg-probe.sh" "$case_name" | tail -n 1)
case $probe_last in
    *': RUNNABLE') ;;
    *': BLOCKED needs '*)
        if [ "${RL_FORCE:-0}" != 1 ]; then
            printf 'skipped: libc-%s blocked: needs %s\n' \
                "$case_name" "${probe_last#*: BLOCKED needs }"
            exit 77
        fi
        ;;
    *)
        printf 'skipped: libc-%s probe failed\n' "$case_name" >&2
        exit 77
        ;;
esac

# A generated sysroot is not tracked. A stale build/<arch>/libc.a from an older libc makes a C
# case link an old TLS/TCB and fail in confusing ways (S115 saw aarch64 at 0xff0). Refresh the
# default generated sysroot when it is missing or older than the libc sources; an explicit
# RAVENLIBC_SYSROOT (the dry-run fixtures) is always trusted as-is.
if [ -z "${RAVENLIBC_SYSROOT:-}" ]; then
    sysroot=$root/build/$arch/sysroot
    libc_archive=$sysroot/usr/lib/libc.a
    stale=0
    if [ ! -f "$libc_archive" ]; then
        stale=1
    elif [ -n "$(find "$root/libc/ravenlibc/crates" "$root/libc/ravenlibc/abi" \
        "$root/libc/ravenlibc/crt" "$root/libc/ravenlibc/Cargo.toml" \
        -newer "$libc_archive" -print -quit 2>/dev/null)" ]; then
        stale=1
    fi
    if [ "$stale" -eq 1 ]; then
        printf 'refresh: generated %s sysroot is missing or older than libc sources\n' "$arch"
        if ! cargo xtask libc-pack --arch "$arch"; then
            printf 'error: cargo xtask libc-pack --arch %s failed\n' "$arch" >&2
            exit 2
        fi
    fi
    # The dynamic runtime is a separate `libc-link` artifact. `libc-pack` only re-stages the
    # sysroot copy, so a libc change newer than `libc.so.1` would otherwise run the case against
    # the previous shared library (K438: a mutated recvmsg still passed). Refresh it here.
    if [ "$link_kind" = dynamic ]; then
        # Compare the source-of-truth `libc-link` output, not the sysroot copy: `libc-pack`
        # re-stages the copy (with a fresh mtime) even when the shared library itself is stale.
        dynamic_libc=$root/build/$arch/libc.so.1
        dynamic_stale=0
        if [ ! -f "$dynamic_libc" ]; then
            dynamic_stale=1
        elif [ -n "$(find "$root/libc/ravenlibc/crates" "$root/libc/ravenlibc/abi" \
            "$root/libc/ravenlibc/crt" "$root/libc/ravenlibc/Cargo.toml" \
            -newer "$dynamic_libc" -print -quit 2>/dev/null)" ]; then
            dynamic_stale=1
        fi
        if [ "$dynamic_stale" -eq 1 ]; then
            printf 'refresh: generated %s libc.so.1 is missing or older than libc sources\n' "$arch"
            if ! cargo xtask libc-link --arch "$arch"; then
                printf 'error: cargo xtask libc-link --arch %s failed\n' "$arch" >&2
                exit 2
            fi
        fi
    fi
else
    sysroot=$RAVENLIBC_SYSROOT
fi
if [ ! -d "$sysroot" ]; then
    printf 'skipped: no sysroot (run cargo xtask libc-pack --arch %s)\n' "$arch"
    exit 77
fi

required="$sysroot/usr/lib/crt1.o $sysroot/usr/lib/crti.o $sysroot/usr/lib/crtn.o $sysroot/usr/lib/libc.a"
if [ "$link_kind" = dynamic ]; then
    required="$required $sysroot/usr/lib/Scrt1.o $sysroot/usr/lib/libc.so $sysroot/lib/libc.so.1 $sysroot/libexec/ld-elf.so.1"
fi
for file in $required; do
    if [ ! -e "$file" ]; then
        printf 'skipped: missing sysroot file %s\n' "$file"
        exit 77
    fi
done

CC=${CC:-clang}
if ! command -v "$CC" >/dev/null 2>&1; then
    printf 'skipped: %s not found\n' "$CC"
    exit 77
fi
# The pinned Rust sysroot ships ld.lld under lib/rustlib/<host>/bin/gcc-ld; use it when the
# host PATH has no standalone lld, so the exact clang -fuse-ld=lld vector still runs.
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
    printf 'skipped: ld.lld not found\n'
    exit 77
fi
if ! command -v llvm-ar >/dev/null 2>&1; then
    printf 'skipped: llvm-ar not found\n'
    exit 77
fi
if [ -z "${READELF:-}" ] && ! command -v llvm-readelf >/dev/null 2>&1 &&
    ! command -v readelf >/dev/null 2>&1; then
    printf 'skipped: llvm-readelf/readelf not found\n'
    exit 77
fi

# H-20 kept every case on the soft-float vector until PG-12 existed. S22 now saves the
# FP/SIMD state, so libc-time's double-returning difftime path builds with the normal SSE ABI;
# the other cases keep the conservative soft-float vector.
float=${RL_TEST_FLOAT:-}
if [ -z "$float" ]; then
    if [ "$arch" = aarch64 ]; then
        # The aarch64 libc is built for aarch64-unknown-none-softfloat. The C case must use the
        # same general-register ABI: with the hard-float ABI a `double` return (difftime) is
        # read from d0 while the libc writes it to x0.
        float=soft
    else
        case $case_name in
            time) float=hard ;;
            *) float=soft ;;
        esac
    fi
fi
case $float in
    soft) ;;
    hard)
        # PG-12 is the x86 FXSAVE/SSE context; aarch64 does not use that vector.
        if [ "$arch" = x86_64 ] &&
            ! "$root/tests/libc/target/pg-probe.sh" all | grep -qx 'PG-12: present'; then
            printf 'skipped: hard-float build needs PG-12\n'
            exit 77
        fi
        ;;
    *) usage ;;
esac

# The soft-float flag is target-specific: x86_64 has no SSE in the none target, aarch64 is
# built for aarch64-unknown-none-softfloat and must not allocate FP/SIMD registers.
fp_flags=
if [ "$float" = soft ]; then
    if [ "$arch" = aarch64 ]; then
        fp_flags=-mgeneral-regs-only
    else
        fp_flags="-mno-sse -mno-mmx -msoft-float"
    fi
fi

out=$root/build/libc-test
mkdir -p "$out/obj"
if [ "$arch" = aarch64 ]; then
    linker_script=$root/userland/linker/aarch64.ld
    linker_flavor="-Wl,-m,aarch64linux"
else
    linker_script=$root/tests/libc/target/x86_64.ld
    linker_flavor=
fi

builtins=
if [ -f "$sysroot/usr/lib/libclang_rt.builtins.a" ]; then
    builtins=1
fi

compile_obj() {
    if [ "$float" = soft ]; then
        "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" \
            -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
            -D__ravenbsd__=1 -D__unix__=1 $fp_flags \
            -I "$root/libc/ravenlibc/tests/c" -c "$1" -o "$2"
    else
        "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" \
            -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
            -D__ravenbsd__=1 -D__unix__=1 \
            -I "$root/libc/ravenlibc/tests/c" -c "$1" -o "$2"
    fi
}

link_static() {
    # A case may own `_start` (K548's exec-timing entry timestamp). Then crt1.o is not linked:
    # the case supplies the entry, and the start function it calls is the same one crt1 uses.
    if [ "${3:-0}" = 1 ]; then
        crt1=
    else
        crt1="$sysroot/usr/lib/crt1.o"
    fi
    if [ -n "$builtins" ]; then
        "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
            -nostdlib -nostartfiles -static -T"$linker_script" \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
            $crt1 "$sysroot/usr/lib/crti.o" \
            "$sysroot/usr/lib/crtbegin.o" "$1" \
            -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
            "$sysroot/usr/lib/crtend.o" "$sysroot/usr/lib/crtn.o" -o "$2"
    else
        "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
            -nostdlib -nostartfiles -static -T"$linker_script" \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
            $crt1 "$sysroot/usr/lib/crti.o" \
            "$sysroot/usr/lib/crtbegin.o" "$1" \
            -L"$sysroot/usr/lib" -lc \
            "$sysroot/usr/lib/crtend.o" "$sysroot/usr/lib/crtn.o" -o "$2"
    fi
}

link_dynamic() {
    if [ -n "$builtins" ]; then
        "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
            -nostdlib -nostartfiles -pie -Wl,-dynamic-linker,/libexec/ld-elf.so.1 \
            -Wl,--image-base=0x400000 \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
            "$sysroot/usr/lib/Scrt1.o" "$sysroot/usr/lib/crti.o" "$1" \
            "$out/obj/tls-stub.o" -L"$sysroot/lib" -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
            "$sysroot/usr/lib/crtn.o" -o "$2"
    else
        "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
            -nostdlib -nostartfiles -pie -Wl,-dynamic-linker,/libexec/ld-elf.so.1 \
            -Wl,--image-base=0x400000 \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
            "$sysroot/usr/lib/Scrt1.o" "$sysroot/usr/lib/crti.o" "$1" \
            "$out/obj/tls-stub.o" -L"$sysroot/lib" -L"$sysroot/usr/lib" -lc \
            "$sysroot/usr/lib/crtn.o" -o "$2"
    fi
}

link_shared() {
    "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
        -shared -nostdlib -nostartfiles -Wl,-soname,libplug.so \
        -Wl,--image-base=0x200000 \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 -Wl,--error-limit=0 \
        "$1" -o "$2"
}

compile_pie() {
    if [ "$float" = soft ]; then
        "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" \
            -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
            -D__ravenbsd__=1 -D__unix__=1 $fp_flags -fPIE \
            -I "$root/libc/ravenlibc/tests/c" -c "$1" -o "$2"
    else
        "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" \
            -std=gnu11 -O1 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
            -D__ravenbsd__=1 -D__unix__=1 -fPIE \
            -I "$root/libc/ravenlibc/tests/c" -c "$1" -o "$2"
    fi
}

compile_pic() {
    # The plugin's `__attribute__((constructor))` is emitted into `.init_array` only at -O0
    # with this target: clang 14 drops a static constructor at -O1 for the unknown OS, which
    # would silently remove the `init` check from the dynamic case. The plugin is tiny.
    if [ "$float" = soft ]; then
        "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" \
            -std=gnu11 -O0 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
            -D__ravenbsd__=1 -D__unix__=1 $fp_flags -fPIC \
            -I "$root/libc/ravenlibc/tests/c" -c "$1" -o "$2"
    else
        "$CC" --target="$arch-unknown-ravenbsd" --sysroot="$sysroot" \
            -std=gnu11 -O0 -g0 -Wall -Wextra -Werror=implicit-function-declaration \
            -D__ravenbsd__=1 -D__unix__=1 -fPIC \
            -I "$root/libc/ravenlibc/tests/c" -c "$1" -o "$2"
    fi
}

src=$root/tests/libc/c/libc-$case_name.c
if [ ! -f "$src" ]; then
    src=$root/libc/ravenlibc/tests/c/libc-$case_name.c
fi
if [ ! -f "$src" ]; then
    printf 'error: missing source %s\n' "$src" >&2
    exit 1
fi

obj=$out/obj/libc-$case_name.o
if [ "$link_kind" = dynamic ]; then
    compile_pie "$src" "$obj"
else
    compile_obj "$src" "$obj"
fi

# A static case that defines `_start` owns the entry; crt1.o must not be linked beside it.
own_start=0
if [ "$link_kind" = static ]; then
    symbol_tool=
    if command -v llvm-nm >/dev/null 2>&1; then
        symbol_tool=llvm-nm
    elif command -v nm >/dev/null 2>&1; then
        symbol_tool=nm
    fi
    if [ -n "$symbol_tool" ] && "$symbol_tool" "$obj" 2>/dev/null | grep -Eq ' T _start$'; then
        own_start=1
    fi
fi

case $binaries in
    *libc-child*)
        child_src=$root/libc/ravenlibc/tests/c/libc-child.c
        if [ "$case_name" = spawn-std ]; then
            child_src=$root/tests/libc/c/libc-spawn-std-child.c
        fi
        compile_obj "$child_src" "$out/obj/libc-child.o"
        link_static "$out/obj/libc-child.o" "$out/libc-child"
        ;;
    *)
        : > "$out/libc-child"
        ;;
esac

case $link_kind in
    static)
        link_static "$obj" "$out/libc-test" "$own_start"
        : > "$out/libplug.so"
        : > "$out/libc.so.1"
        : > "$out/ld-elf.so.1"
        ;;
    dynamic)
        compile_pic "$root/libc/ravenlibc/tests/c/libplug.c" "$out/obj/libplug.o"
        "$CC" --target="$arch-unknown-ravenbsd" -c \
            "$root/tests/libc/target/tls-stub.s" -o "$out/obj/tls-stub.o"
        link_shared "$out/obj/libplug.o" "$out/libplug.so"
        link_dynamic "$obj" "$out/libc-test"
        cp "$sysroot/lib/libc.so.1" "$out/libc.so.1"
        cp "$sysroot/libexec/ld-elf.so.1" "$out/ld-elf.so.1"
        ;;
    *)
        printf 'error: unknown link kind %s\n' "$link_kind" >&2
        exit 1
        ;;
esac

printf '%s\n' "$case_name" > "$out/stamp"
sh "$root/tests/libc/target/shape-check.sh" "$out/libc-test"
