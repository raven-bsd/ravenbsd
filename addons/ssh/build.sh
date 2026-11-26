#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# OpenSSH portable C add-on build. The upstream source is vendored unmodified under
# addons/ssh/openssh/ and is built out of tree against the RavenBSD libc clang sysroot and the T1
# OpenSSL add-on. SSH is never implemented in-house (DR-65); this script only builds upstream.
#
# Usage:
#   sh addons/ssh/build.sh [--arch x86_64|aarch64] --check   verify the pinned source, provenance
#   sh addons/ssh/build.sh [--arch x86_64|aarch64]           build ssh, sshd, sshd-session,
#                                                            sshd-auth and ssh-keygen
#   sh addons/ssh/build.sh [--arch x86_64|aarch64] --qtest   stage a minimal initfs
#
# The aarch64 build uses the aarch64 libc sysroot, userland/linker/aarch64.ld, the cross aarch64
# libgcc builtins and the aarch64 T1 OpenSSL (build/tls/aarch64). `--arch` selects the target and
# defaults to RAVEN_QT_ARCH so a qtest `pre` step builds the case's architecture, x86_64 otherwise.
# The ssh-version qtest wrapper is built for both arches. No upstream OpenSSH source is modified.
#
# OpenSSH 10.6 splits the server into `sshd` (the listener), `sshd-session` and `sshd-auth`.
# The target runtime paths are `/usr/libexec/sshd-session` and `/usr/libexec/sshd-auth`; the
# build outputs are staged at `build/ssh/<arch>/libexec/` so K195 can install them there.
# No SSH protocol, crypto or privilege-separation source is changed.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
mode=--build
while [ $# -gt 0 ]; do
    case "$1" in
        --check | --build | --qtest) mode=$1; shift ;;
        --arch) arch=$2; shift 2 ;;
        *) echo "ssh: unknown argument $1" >&2; exit 2 ;;
    esac
done

source=$root/addons/ssh/openssh
compat=$root/addons/ssh/compat
sysroot=$root/build/ssh/$arch/sysroot
t1=$root/build/tls/$arch
t1_stage=$root/build/ssh/$arch/t1
build=$root/build/ssh/openssh-build-$arch
install=$root/build/ssh/$arch
version_file=$source/version.h
version_src=$root/addons/ssh/qtest/ssh_version_main.c
jobs=${SSH_JOBS:-4}

log() {
    echo "ssh: $*"
}

die() {
    echo "ssh: $*" >&2
    exit 2
}

case "$arch" in
    x86_64)
        target=x86_64-unknown-ravenbsd
        host_triplet=x86_64-unknown-linux-gnu
        ld_script=$root/tests/libc/target/x86_64.ld
        fp_flags=
        linker_flavor=
        ;;
    aarch64)
        target=aarch64-unknown-ravenbsd
        host_triplet=aarch64-unknown-linux-gnu
        ld_script=$root/userland/linker/aarch64.ld
        fp_flags=-mgeneral-regs-only
        linker_flavor=-Wl,-m,aarch64linux
        ;;
    *)
        die "unsupported arch $arch (x86_64 or aarch64)"
        ;;
esac

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
    if grep -rqE '^-----BEGIN (RSA |EC |DSA |OPENSSH |ENCRYPTED )?PRIVATE KEY-----' "$source"; then
        die "vendored OpenSSH contains a private-key fixture rejected by push protection"
    fi
}

check_tools() {
    for tool in clang perl make strings llvm-ar llvm-objcopy; do
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
    if [ "$arch" = aarch64 ]; then
        builtins=/usr/lib/gcc-cross/aarch64-linux-gnu/11/libgcc.a
    else
        builtins=$resource/lib/linux/libclang_rt.builtins-x86_64.a
    fi
    [ -f "$builtins" ] || die "missing builtins: $builtins"
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
    # Git checkout mtimes are not ordered; the generated configure must not look older than
    # configure.ac/m4/*.m4 or OpenSSH refuses with `run autoreconf`. Touch the generated file for
    # the configure run, do not regenerate or modify the vendored source. Restore its mtime
    # afterwards so building one arch does not leave the other arch's staged binaries older than
    # a shared add-on input.
    stamp=$root/build/ssh/$arch/configure.stamp
    mkdir -p "$(dirname "$stamp")"
    touch -r "$source/configure" "$stamp" || die "cannot stamp $source/configure"
    touch "$source/configure"
    rm -rf "$build" "$t1_stage"
    mkdir -p "$build" "$t1_stage"
    cp -a "$t1/lib" "$t1_stage/lib"
    cp -a "$t1/include" "$t1_stage/include"
    (
        cd "$build"
        CC=clang \
        ac_cv_func_getline=yes ac_cv_func_getdelim=yes ac_cv_func_setsid=yes ac_cv_func_getcwd=yes \
        ac_cv_func_setresuid=yes ac_cv_func_setresgid=yes \
        ac_cv_func_snprintf=yes ac_cv_func_vsnprintf=yes ac_cv_func_arc4random_buf=yes \
        CFLAGS="--target=$target --sysroot=$sysroot -D__ravenbsd__=1 -D__unix__=1 -fno-stack-protector $fp_flags -I$t1_stage/include -include $compat/ssh_compat.h" \
        LDFLAGS="--target=$target --sysroot=$sysroot -fuse-ld=lld $linker_flavor -nostdlib -nostartfiles -static -T$ld_script -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 $sysroot/usr/lib/crt1.o $sysroot/usr/lib/crti.o -L$sysroot/usr/lib -L$t1_stage/lib" \
        LIBS="-L$sysroot/usr/lib -L$t1_stage/lib -lc -lclang_rt.builtins $sysroot/usr/lib/crtn.o" \
        "$source/configure" --host="$host_triplet" --build=x86_64-pc-linux-gnu \
            --prefix="$install" --sysconfdir="$install/etc/ssh" --libexecdir=/usr/libexec \
            --with-ssl-dir="$t1_stage" --without-openssl-header-check \
            --without-zlib --without-pam --without-selinux --without-audit --without-kerberos5 --without-libedit
    )
    touch -r "$stamp" "$source/configure" || die "cannot restore $source/configure mtime"
    rm -f "$stamp"
}

build() {
    if [ ! -f "$t1/lib/libcrypto.a" ] || [ ! -f "$t1/lib/libssl.a" ]; then
        log "T1 libcrypto/libssl missing; building T1 OpenSSL first"
        sh "$root/addons/tls/build.sh" --arch "$arch"
    fi
    check_t1
    stage_sysroot
    find_lld
    log "build: configuring OpenSSH 10.6p2 for $target"
    configure
    # The linux-gnu host selection forces USE_BTMP/_PATH_BTMP, but RavenBSD has no <utmp.h>.
    # Drop the generated btmp defines; OpenSSH then uses the available utmpx path and does not
    # compile the struct-utmp failure logger. The configure field probe also reports ut_addr in
    # utmpx.h, but RavenBSD's header has only ut_addr_v6; drop that false positive. This edits
    # build artifacts, not upstream source.
    config="$build/config.h"
    sed -e '/^#define USE_BTMP/d' -e '/^#define _PATH_BTMP/d' \
        -e '/^#define HAVE_ADDR_IN_UTMPX/d' "$config" > "$config.k212"
    mv "$config.k212" "$config"
    log "build: compiling (jobs=$jobs)"
    (
        cd "$build"
        make -j"$jobs" ssh sshd sshd-session sshd-auth ssh-keygen \
            EX_LIBS="-L$sysroot/usr/lib -L$t1/lib -lssl -lcrypto -lc -lclang_rt.builtins $sysroot/usr/lib/crtn.o -Wl,--error-limit=0"
    )
    strings "$build/sshd" | grep -Fq '/usr/libexec/sshd-session' ||
        die "sshd does not embed the target sshd-session path"
    strings "$build/sshd" | grep -Fq '/usr/libexec/sshd-auth' ||
        die "sshd does not embed the target sshd-auth path"
    mkdir -p "$install/bin" "$install/libexec"
    cp "$build/ssh" "$build/sshd" "$build/ssh-keygen" "$install/bin/"
    cp "$build/sshd-session" "$build/sshd-auth" "$install/libexec/"
    chmod 0755 "$install/libexec/sshd-session" "$install/libexec/sshd-auth"
    log "built: $install/bin/ssh $install/bin/sshd $install/bin/ssh-keygen"
    log "built: $install/libexec/sshd-session $install/libexec/sshd-auth"
    log "K195 staging: install libexec/sshd-session -> /usr/libexec/sshd-session; install libexec/sshd-auth -> /usr/libexec/sshd-auth"
}

build_version_wrapper() {
    [ -f "$build/sshd.o" ] || die "sshd.o missing; run --build first"
    llvm-objcopy --redefine-sym main=sshd_main "$build/sshd.o" "$build/sshd_version.o" ||
        die "cannot rename the upstream sshd main"
    clang --target="$target" --sysroot="$sysroot" \
        -D__ravenbsd__=1 -D__unix__=1 -fno-stack-protector $fp_flags \
        -I"$t1_stage/include" -include "$compat/ssh_compat.h" -std=gnu11 -O1 \
        -c "$version_src" -o "$build/ssh_version_main.o" ||
        die "cannot compile the ssh-version wrapper"
    clang --target="$target" --sysroot="$sysroot" -fuse-ld=lld $linker_flavor \
        -nostdlib -nostartfiles -static -T"$ld_script" \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" \
        "$build/ssh_version_main.o" "$build/sshd_version.o" \
        "$build/platform-listen.o" "$build/servconf.o" "$build/srclimit.o" \
        "$build/groupaccess.o" "$build/auth2-methods.o" "$build/dns.o" \
        "$build/fatal.o" "$build/compat.o" "$build/utf8.o" "$build/authfd.o" \
        "$build/canohost.o" "$build/ssh-pkcs11-client.o" "$build/ssh-sk-client.o" \
        -L"$build" -L"$build/openbsd-compat" -L"$sysroot/usr/lib" -L"$t1_stage/lib" \
        -lssh -lopenbsd-compat -lc -lclang_rt.builtins -lcrypto \
        "$sysroot/usr/lib/crtn.o" -Wl,--allow-multiple-definition -Wl,--error-limit=0 \
        -o "$install/bin/ssh-version" ||
        die "cannot link the ssh-version wrapper"
    log "built: $install/bin/ssh-version"
}

qtest_initfs() {
    [ -x "$install/bin/sshd" ] || build
    if [ ! -x "$install/bin/ssh-version" ] || [ "$version_src" -nt "$install/bin/ssh-version" ]; then
        build_version_wrapper
    fi
    init=$root/build/$arch/user/raven-init
    if [ ! -x "$init" ]; then
        cargo xtask build user --arch "$arch" -p raven-init
    fi
    output=$install/ssh-version-initfs.img
    cargo run -q -p raven-initfs --features std --example mkinitfs -- \
        "$output" \
        "/init=$init" \
        "/bin/ssh-version=$install/bin/ssh-version" \
        "/dev/null=$root/addons/ssh/qtest/dev-null" \
        "/etc/services.d/ssh-version.toml=$root/addons/ssh/qtest/ssh-version.toml" \
        "/etc/svcd.conf=$root/addons/ssh/qtest/svcd.conf"
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
        die "usage: sh addons/ssh/build.sh [--check|--build|--qtest]"
        ;;
esac
