#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build a release initfs whose dynamic interpreter has the release-image shape: the first
# PT_LOAD starts at vaddr=0. The interpreter is relinked from the same archive as the normal
# `ld-elf.so.1` with `--image-base=0`, so its symbols and relative relocations stay consistent.
# The pre-S129 raven-spawn parser rejects that shape with SegmentOutsideUserRange; S129 accepts
# it and applies the non-zero interpreter bias. The staged loader is restored before exit.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=${1:-x86_64}
case "$arch" in
    x86_64) target=x86_64-unknown-none ;;
    *)
        printf 'login fixture: unsupported arch %s\n' "$arch" >&2
        exit 2
        ;;
esac

loader="build/$arch/ld-elf.so.1"
archive="build/$arch/cargo/libc-rtld/$target/libc-so/libraven_libc_rtld.a"
out="build/fixtures/login/release-dynamic-vaddr0.img"
vaddr0="build/fixtures/login/ld-elf-vaddr0.so.1"

cargo xtask libc-link --arch "$arch"
if [ ! -f "$archive" ]; then
    printf 'login fixture: missing %s\n' "$archive" >&2
    exit 2
fi

lld=${LD_LLD:-}
if [ -z "$lld" ]; then
    lld="$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld"
fi
if [ ! -x "$lld" ]; then
    printf 'login fixture: linker not found: %s\n' "$lld" >&2
    exit 2
fi

mkdir -p "$(dirname -- "$out")"
"$lld" -flavor gnu -shared --image-base=0 -e _rtld_start -Bsymbolic --no-undefined \
    -z now -z relro -z noexecstack -z pack-relative-relocs \
    -z max-page-size=4096 -z common-page-size=4096 \
    --hash-style=gnu --gc-sections --soname=ld-elf.so.1 \
    --version-script=libc/ravenlibc/ld/Symbols.map \
    -u _rtld_start -u _rtld_error_str -u dlclose -u dlerror -u dlopen -u dlsym \
    -u dlvsym -u dladdr -u dlinfo -u dl_iterate_phdr -u __tls_get_addr \
    -u _rtld_allocate_tls -u _rtld_free_tls -u _rtld_get_stack_prot \
    -u _rtld_set_dlerror_slot -u r_debug_state -u _r_debug_postinit -u _rtld_version \
    "$archive" -o "$vaddr0"

first_load=$(readelf -lW "$vaddr0" | awk '/^  LOAD / {print; exit}')
case "$first_load" in
    *0x0000000000000000*) ;;
    *)
        printf 'login fixture: vaddr-0 link failed: %s\n' "$first_load" >&2
        exit 3
        ;;
esac

backup=$(mktemp "${TMPDIR:-/tmp}/ld-elf.XXXXXX")
cp "$loader" "$backup"
restore() {
    cp "$backup" "$loader"
    rm -f "$backup"
}
trap restore EXIT INT TERM HUP

cp "$vaddr0" "$loader"
cargo xtask initfs --arch "$arch" --profile release
cp "build/$arch/initfs.img" "$out"
