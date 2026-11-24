#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# OpenSSL 3.5 C add-on build. The upstream source is vendored unmodified under
# addons/tls/openssl/ and is built out of tree against the RavenBSD libc clang sysroot.
# TLS is never implemented in-house (DR-45); this script only builds and packages upstream.
#
# Usage:
#   sh addons/tls/build.sh [--arch x86_64|aarch64] --check   verify the pinned source, provenance
#   sh addons/tls/build.sh [--arch x86_64|aarch64]           build libcrypto/libssl and openssl
#   sh addons/tls/build.sh [--arch x86_64|aarch64] --qtest   build and pack the openssl-version qtest
#
# The aarch64 build uses the aarch64 libc sysroot, userland/linker/aarch64.ld and the cross
# aarch64 libgcc for the compiler builtins (the host clang has no aarch64 compiler-rt). No
# upstream OpenSSL source is modified.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

arch=x86_64
mode=--build
while [ $# -gt 0 ]; do
    case "$1" in
        --check | --build | --qtest) mode=$1; shift ;;
        --arch) arch=$2; shift 2 ;;
        *) echo "tls: unknown argument $1" >&2; exit 2 ;;
    esac
done

source=$root/addons/tls/openssl
compat=$root/addons/tls/compat/tls_compat.c
wrapper=$root/addons/tls/qtest/openssl_version_main.c
sysroot=$root/build/tls/$arch/sysroot
build=$root/build/tls/openssl-build-$arch
install=$root/build/tls/$arch
version_file=$source/VERSION.dat
jobs=${TLS_JOBS:-4}

log() {
    echo "tls: $*"
}

die() {
    echo "tls: $*" >&2
    exit 2
}

case "$arch" in
    x86_64)
        target=x86_64-unknown-ravenbsd
        ld_script=$root/tests/libc/target/x86_64.ld
        openssl_target=linux-x86_64
        fp_flags=
        linker_flavor=
        ;;
    aarch64)
        target=aarch64-unknown-ravenbsd
        ld_script=$root/userland/linker/aarch64.ld
        openssl_target=linux-aarch64
        fp_flags=-mgeneral-regs-only
        linker_flavor=-Wl,-m,aarch64linux
        ;;
    *)
        die "unsupported arch $arch (x86_64 or aarch64)"
        ;;
esac

version_value() {
    sed -n "s/^$1=//p" "$version_file" | tr -d '"'
}

find_lld() {
    if command -v ld.lld >/dev/null 2>&1; then
        return 0
    fi
    host=$(rustc -vV 2>/dev/null | sed -n 's/^host: //p' || true)
    [ -n "$host" ] || return 1
    directory=$(rustc --print sysroot 2>/dev/null)/lib/rustlib/$host/bin/gcc-ld
    [ -x "$directory/ld.lld" ] || return 1
    PATH="$directory:$PATH"
    export PATH
}

check_source() {
    [ -f "$version_file" ] || die "missing vendored OpenSSL VERSION.dat"
    major=$(version_value MAJOR)
    minor=$(version_value MINOR)
    patch=$(version_value PATCH)
    [ "$major.$minor.$patch" = "3.5.0" ] ||
        die "vendored OpenSSL is $major.$minor.$patch, expected 3.5.0"
    [ -f "$source/LICENSE.txt" ] || die "missing OpenSSL LICENSE.txt"
    grep -q 'path = "addons/tls/openssl"' "$root/PROVENANCE.toml" ||
        die "PROVENANCE.toml has no addons/tls/openssl import"
    grep -q 'upstream_rev = "636dfadc70ce26f2473870570bfd9ec352806b1d"' "$root/PROVENANCE.toml" ||
        die "PROVENANCE.toml does not pin the OpenSSL 3.5.0 revision"
    grep -q '^addons/tls/openssl/' "$root/.provenance-ignore" ||
        die ".provenance-ignore does not exempt the vendored OpenSSL tree"
    [ -f "$compat" ] || die "missing compat shim $compat"
    [ -f "$wrapper" ] || die "missing qtest wrapper $wrapper"
    [ -f "$root/docs/tls.md" ] || die "missing docs/tls.md"
    if grep -rqE "BEGIN (RSA |EC |DSA |OPENSSH |ENCRYPTED )?PRIVATE KEY" "$source"; then
        die "vendored OpenSSL contains a private-key fixture rejected by push protection"
    fi
}

check_tools() {
    for tool in clang perl make llvm-ar; do
        command -v "$tool" >/dev/null 2>&1 || die "$tool is required"
    done
    find_lld || die "ld.lld is required (host PATH or the Rust toolchain)"
}

check() {
    check_source
    check_tools
    log "check ok version=3.5.0 source=addons/tls/openssl"
}

stage_sysroot() {
    log "libc: refreshing build/$arch artifacts"
    cargo xtask libc-pack --arch "$arch" --out "build/$arch" >/dev/null

    rm -rf "$sysroot"
    mkdir -p "$sysroot/usr/include" "$sysroot/usr/lib" "$sysroot/lib" "$sysroot/libexec"
    cp -a libc/ravenlibc/include/. "$sysroot/usr/include/"
    cp -a libc/ravenlibc/arch/$arch/include/. "$sysroot/usr/include/"
    cp -a libc/ravenlibc/abi/generated/include/. "$sysroot/usr/include/"
    cp build/$arch/libc.a build/$arch/crt1.o build/$arch/crti.o build/$arch/crtn.o \
        build/$arch/Scrt1.o build/$arch/rcrt1.o "$sysroot/usr/lib/"
    for name in libdl libpthread librt; do
        if [ -f "build/$arch/stubs/$name.a" ]; then
            cp "build/$arch/stubs/$name.a" "$sysroot/usr/lib/"
        else
            (cd "$sysroot/usr/lib" && ar crs "$name.a")
        fi
    done
    resource=$(clang --print-resource-dir)
    if [ "$arch" = aarch64 ]; then
        builtins=/usr/lib/gcc-cross/aarch64-linux-gnu/11/libgcc.a
    else
        builtins=$resource/lib/linux/libclang_rt.builtins-x86_64.a
    fi
    [ -f "$builtins" ] || die "missing builtins: $builtins"
    cp "$builtins" "$sysroot/usr/lib/libclang_rt.builtins.a"
    # The RavenBSD forwarding stdatomic.h shadows clang's header and its include_next finds
    # nothing. Remove it from the staged sysroot so clang's own C11 atomics are used. This is a
    # build-sysroot workaround for the libc forwarding header; no OpenSSL source is modified.
    rm -f "$sysroot/usr/include/stdatomic.h"
}

configure() {
    find_lld
    rm -rf "$build"
    mkdir -p "$build"
    (
        cd "$build"
        CC=clang \
        CFLAGS="--target=$target --sysroot=$sysroot -D__ravenbsd__=1 -D__unix__=1 -fno-stack-protector $fp_flags" \
        LDFLAGS="--target=$target --sysroot=$sysroot -fuse-ld=lld $linker_flavor -nostdlib -nostartfiles -static -T$ld_script -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 $sysroot/usr/lib/crt1.o $sysroot/usr/lib/crti.o" \
        "$source/Configure" "$openssl_target" no-shared no-tests no-asm no-dso no-engine no-async \
            no-threads no-legacy no-ssl3 no-comp \
            --prefix="$install" --openssldir="$install/ssl"
    )
}

build() {
    stage_sysroot
    find_lld
    log "build: configuring OpenSSL 3.5.0 for $target"
    configure
    clang --target="$target" --sysroot="$sysroot" $fp_flags \
        -std=gnu11 -O1 -D__ravenbsd__=1 -D__unix__=1 \
        -c "$compat" -o "$root/build/tls/tls_compat.o"
    log "build: compiling (jobs=$jobs)"
    (
        cd "$build"
        make -j"$jobs" build_sw \
            EX_LIBS="-L$sysroot/usr/lib -lc -lclang_rt.builtins $root/build/tls/tls_compat.o $sysroot/usr/lib/crtn.o -Wl,--error-limit=0"
    )
    mkdir -p "$install/bin" "$install/lib" "$install/include"
    cp "$build/apps/openssl" "$install/bin/openssl"
    cp "$build/libcrypto.a" "$build/libssl.a" "$install/lib/"
    cp -a "$build/include/openssl/." "$install/include/openssl/"
    # The generated include tree does not contain every public source header (for example
    # openssl/macros.h). Add the upstream public headers without clobbering generated files.
    cp -an "$source/include/openssl/." "$install/include/openssl/"
    if [ "$arch" = x86_64 ]; then
        clang --target="$target" --sysroot="$sysroot" $fp_flags \
            -I"$build/include" -I"$source/include" -I"$source/apps" \
            -std=gnu11 -O1 -D__ravenbsd__=1 -D__unix__=1 \
            -c "$wrapper" -o "$root/build/tls/openssl_version_main.o"
        clang --target="$target" --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
            -nostdlib -nostartfiles -static -T"$ld_script" \
            -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
            -Wl,--allow-multiple-definition -Wl,--error-limit=0 \
            "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" \
            "$root/build/tls/openssl_version_main.o" \
            "$build/apps/openssl-bin-version.o" "$build/apps/libapps.a" \
            -L"$build" -L"$sysroot/usr/lib" \
            -lssl -lcrypto -lc -lclang_rt.builtins \
            "$root/build/tls/tls_compat.o" "$sysroot/usr/lib/crtn.o" \
            -o "$install/bin/openssl-version"
        log "built: $install/bin/openssl $install/bin/openssl-version libcrypto.a libssl.a"
    else
        # The qtest wrapper uses x86 syscall registers; the aarch64 lane only needs the libraries.
        log "built: $install/bin/openssl libcrypto.a libssl.a (aarch64; x86-only wrapper skipped)"
    fi
}

qtest_initfs() {
    [ -x "$install/bin/openssl-version" ] || build
    init=$root/build/$arch/user/raven-init
    if [ ! -x "$init" ]; then
        cargo xtask build user --arch "$arch" -p raven-init
    fi
    output=$install/openssl-version-initfs.img
    cargo run -q -p raven-initfs --features std --example mkinitfs -- \
        "$output" \
        "/init=$init" \
        "/bin/openssl-version=$install/bin/openssl-version" \
        "/etc/services.d/openssl.toml=$root/tests/tls/openssl-version.d/openssl.toml" \
        "/etc/svcd.conf=$root/tests/tls/openssl-version.d/svcd.conf"
    log "qtest initfs: $output"
}

case "$mode" in
    --check)
        check
        ;;
    --qtest)
        check_source
        check_tools
        qtest_initfs
        ;;
    --build)
        check_source
        check_tools
        build
        ;;
    *)
        die "usage: sh addons/tls/build.sh [--check|--build|--qtest]"
        ;;
esac
