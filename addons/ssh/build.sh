#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# OpenSSH portable C add-on build. The upstream source is vendored unmodified under
# addons/ssh/openssh/ and is built out of tree against the RavenBSD libc clang sysroot and the T1
# OpenSSL add-on. SSH is never implemented in-house (DR-65); this script only builds upstream.
#
# Usage:
#   sh addons/ssh/build.sh --check   verify the pinned source, provenance and host tools
#   sh addons/ssh/build.sh           build sshd, ssh and ssh-keygen
#   sh addons/ssh/build.sh --qtest   stage a minimal initfs (requires a completed build)
#
# Current status: BLOCKED on T1. The T1 OpenSSL build fails before this script can configure
# OpenSSH: (1) the libc stdatomic.h forwarding header breaks clang's <stdatomic.h>, and
# (2) with a scratch header workaround, the T1 compat shim duplicates libc symbols at link.
# The check mode below still passes; build/qtest report the exact T1 failure.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

arch=x86_64
mode=--build
for argument in "$@"; do
    case "$argument" in
        --check | --build | --qtest) mode=$argument ;;
        *) echo "ssh: unknown argument $argument" >&2; exit 2 ;;
    esac
done

source=$root/addons/ssh/openssh
compat=$root/addons/ssh/compat
sysroot=$root/build/ssh/$arch/sysroot
t1=$root/build/tls/$arch
build=$root/build/ssh/openssh-build-$arch
install=$root/build/ssh/$arch
version_file=$source/version.h
jobs=${SSH_JOBS:-4}

log() {
    echo "ssh: $*"
}

die() {
    echo "ssh: $*" >&2
    exit 2
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
    [ -f "$version_file" ] || die "missing vendored OpenSSH version.h"
    grep -q 'SSH_VERSION[[:space:]]*"OpenSSH_10.6"' "$version_file" ||
        die "vendored OpenSSH is not V_10_6"
    grep -q 'SSH_PORTABLE[[:space:]]*"p2"' "$version_file" ||
        die "vendored OpenSSH is not p2"
    [ -f "$source/LICENCE" ] || die "missing OpenSSH LICENCE"
    grep -q 'path = "addons/ssh/openssh"' "$root/PROVENANCE.toml" ||
        die "PROVENANCE.toml has no addons/ssh/openssh import"
    grep -q 'upstream_rev = "d449e662b6b8b8cb20e852d8084c269af1c5e8f1"' "$root/PROVENANCE.toml" ||
        die "PROVENANCE.toml does not pin OpenSSH V_10_6_P2"
    grep -q '^addons/ssh/openssh/$' "$root/.provenance-ignore" ||
        die ".provenance-ignore does not exempt the vendored OpenSSH tree"
    [ -f "$root/docs/ssh-addon.md" ] || die "missing docs/ssh-addon.md"
    if grep -rqE "BEGIN (RSA |EC |DSA |OPENSSH |ENCRYPTED )?PRIVATE KEY" "$source"; then
        die "vendored OpenSSH contains a private-key fixture rejected by push protection"
    fi
}

check_tools() {
    for tool in clang perl make llvm-ar; do
        command -v "$tool" >/dev/null 2>&1 || die "$tool is required"
    done
    find_lld || die "ld.lld is required (host PATH or the Rust toolchain)"
}

check_t1() {
    [ -f "$t1/lib/libcrypto.a" ] && [ -f "$t1/lib/libssl.a" ] ||
        die "T1 libcrypto/libssl missing; run addons/tls/build.sh (currently blocked by the libc stdatomic.h and duplicate-symbol defects)"
}

check() {
    check_source
    check_tools
    if [ -f "$t1/lib/libcrypto.a" ] && [ -f "$t1/lib/libssl.a" ]; then
        log "check ok version=OpenSSH_10.6p2 source=addons/ssh/openssh t1=present"
    else
        log "check ok version=OpenSSH_10.6p2 source=addons/ssh/openssh t1=missing"
    fi
}

stage_sysroot() {
    log "libc: refreshing build/$arch artifacts"
    cargo xtask libc-pack --arch "$arch" --out "build/$arch" >/dev/null

    rm -rf "$sysroot"
    mkdir -p "$sysroot/usr/include/openssl" "$sysroot/usr/lib" "$sysroot/lib" "$sysroot/libexec"
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
    builtins=$resource/lib/linux/libclang_rt.builtins-x86_64.a
    [ -f "$builtins" ] || die "missing compiler-rt builtins: $builtins"
    cp "$builtins" "$sysroot/usr/lib/libclang_rt.builtins.a"

    # K200 portability workaround: the RavenBSD forwarding stdatomic.h shadows clang's header
    # and its include_next finds nothing. Remove it from the build sysroot so clang's own C11
    # atomics are used. The libc owner should fix the forwarding header; no upstream source is
    # modified.
    rm -f "$sysroot/usr/include/stdatomic.h"

    cp -a "$t1/include/openssl/." "$sysroot/usr/include/openssl/"
    cp "$t1/lib/libcrypto.a" "$t1/lib/libssl.a" "$sysroot/usr/lib/"
}

configure() {
    find_lld
    rm -rf "$build"
    mkdir -p "$build"
    (
        cd "$build"
        CC=clang \
        CFLAGS="--target=x86_64-unknown-ravenbsd --sysroot=$sysroot -D__ravenbsd__=1 -D__unix__=1 -fno-stack-protector -I$t1/include" \
        LDFLAGS="--target=x86_64-unknown-ravenbsd --sysroot=$sysroot -fuse-ld=lld -nostdlib -nostartfiles -static -T$root/tests/libc/target/x86_64.ld -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 $sysroot/usr/lib/crt1.o $sysroot/usr/lib/crti.o -L$t1/lib" \
        "$source/configure" --host=x86_64-unknown-ravenbsd --build=x86_64-pc-linux-gnu \
            --prefix="$install" --sysconfdir="$install/etc/ssh" \
            --with-ssl-dir="$t1" --without-openssl-header-check \
            --without-pam --without-selinux --without-audit --without-kerberos5 --without-libedit
    )
}

build() {
    if [ ! -f "$t1/lib/libcrypto.a" ] || [ ! -f "$t1/lib/libssl.a" ]; then
        log "T1 libcrypto/libssl missing; building T1 OpenSSL first"
        sh "$root/addons/tls/build.sh"
    fi
    check_t1
    stage_sysroot
    find_lld
    log "build: configuring OpenSSH 10.6p2 for x86_64-unknown-ravenbsd"
    configure
    log "build: compiling (jobs=$jobs)"
    (
        cd "$build"
        make -j"$jobs" ssh sshd ssh-keygen \
            EX_LIBS="-L$sysroot/usr/lib -L$t1/lib -lssl -lcrypto -lc -lclang_rt.builtins $sysroot/usr/lib/crtn.o -Wl,--error-limit=0"
    )
    mkdir -p "$install/bin"
    cp "$build/ssh" "$build/sshd" "$build/ssh-keygen" "$install/bin/"
    log "built: $install/bin/ssh $install/bin/sshd $install/bin/ssh-keygen"
}

qtest_initfs() {
    for binary in ssh sshd ssh-keygen; do
        [ -x "$install/bin/$binary" ] ||
            die "missing $install/bin/$binary; --qtest requires a completed build"
    done
    die "ssh-version qtest staging is BLOCKED on the T1 OpenSSL build; no initfs was produced"
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
        die "usage: sh addons/ssh/build.sh [--check|--build|--qtest]"
        ;;
esac
