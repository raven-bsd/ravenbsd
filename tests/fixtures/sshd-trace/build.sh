#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K498: build the release initfs and overlay it with a DEBUG3 sshd service, the
# debug log path, the ELF readpassphrase helper and the loopback client script.
# Then wrap the overlay initfs in the release image. The release userland and the
# upstream sshd binary are unchanged; only the test initfs differs. The qtest
# boots this image and drives one guest loopback password login plus one command.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

for file in build/ssh/x86_64/bin/sshd build/ssh/x86_64/bin/ssh \
    build/ssh/x86_64/libexec/sshd-session build/ssh/x86_64/libexec/sshd-auth; do
    if [ ! -x "$file" ]; then
        echo "sshd-trace: missing $file; run sh addons/ssh/build.sh" >&2
        exit 2
    fi
done
sysroot=build/ssh/x86_64/sysroot
if [ ! -f "$sysroot/usr/lib/libc.a" ]; then
    echo "sshd-trace: missing $sysroot; run sh addons/ssh/build.sh" >&2
    exit 2
fi

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

if [ ! -f build/x86_64/initfs.img ]; then
    echo "sshd-trace: building the release initfs" >&2
    cargo xtask initfs --arch x86_64 --profile release
fi

out=build/fixtures/sshd-trace
mkdir -p "$out"

find_lld || {
    echo "sshd-trace: ld.lld is required (host PATH or the Rust toolchain)" >&2
    exit 2
}
clang --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -D__ravenbsd__=1 -D__unix__=1 -fno-stack-protector -std=gnu11 -O1 \
    -c tests/fixtures/sshd-trace/askpass.c -o "$out/askpass.o"
clang --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
    -nostdlib -nostartfiles -static -Ttests/libc/target/x86_64.ld \
    -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
    "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/askpass.o" \
    -L"$sysroot/usr/lib" -Wl,-z,relro -Wl,-z,now -Wl,-z,noexecstack \
    -Wl,-z,retpolineplt -pie -lc -lclang_rt.builtins \
    "$sysroot/usr/lib/crtn.o" -o "$out/askpass"

cargo build -q -p raven-b70-overlay
overlay=${CARGO_TARGET_DIR:-target}/debug/raven-b70-overlay
"$overlay" build/x86_64/initfs.img "$out/initfs.img" \
    "/etc/ssh/sshd_config=$root/tests/fixtures/sshd-trace/sshd_config:0644" \
    "/etc/services.d/sshd.toml=$root/tests/fixtures/sshd-trace/sshd.service.toml:0644" \
    "/bin/sshd-trace-askpass=$out/askpass:0755" \
    "/etc/ssh/trace-client.sh=$root/tests/fixtures/sshd-trace/trace-client.sh:0644"

image=$out/sshd-trace.img
if [ ! -f "$image" ] || [ "$out/initfs.img" -nt "$image" ]; then
    cargo xtask image --arch x86_64 --profile release \
        --initfs "$out/initfs.img" --output "$image"
fi

echo "sshd-trace: image $image"
