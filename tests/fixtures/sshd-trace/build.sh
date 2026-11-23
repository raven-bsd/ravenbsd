#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K547: build the release initfs and overlay the DEBUG3 sshd service, the debug
# log path, the static private-tap network drop-in and the loopback control
# helpers. Then wrap the overlay initfs in the release image. The release
# userland and the upstream sshd binary are unchanged; only the test initfs
# differs. run.sh boots this image over an isolated tap and compares the host
# tap client with the guest loopback client.

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

# The shared build/<arch>/initfs.img is addressed by path only; another lane may leave the base
# profile there. This fixture needs the release profile (sshd, ptyd, ssh host keys). Regenerate
# only when the packed image lacks ptyd: a rebuild is minutes, the overlay below is seconds.
if ! strings build/x86_64/initfs.img 2>/dev/null | grep -q '/etc/services.d/ptyd.toml'; then
    echo "sshd-trace: rebuilding the release initfs" >&2
    cargo xtask initfs --arch x86_64 --profile release
fi

out=build/fixtures/sshd-trace
mkdir -p "$out"

sysroot=build/ssh/x86_64/sysroot
if [ ! -f "$sysroot/usr/lib/libc.a" ]; then
    sysroot=build/x86_64/sysroot
fi
for file in crt1.o crti.o crtn.o libc.a libclang_rt.builtins.a; do
    if [ ! -f "$sysroot/usr/lib/$file" ]; then
        echo "sshd-trace: missing $sysroot/usr/lib/$file (run sh addons/ssh/build.sh)" >&2
        exit 2
    fi
done

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
clang --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -D__ravenbsd__=1 -D__unix__=1 -fno-stack-protector -std=gnu11 -O1 \
    -c tests/fixtures/sshd-trace/accept-probe.c -o "$out/accept-probe.o"
clang --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
    -nostdlib -nostartfiles -static -Ttests/libc/target/x86_64.ld \
    -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
    "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/accept-probe.o" \
    -L"$sysroot/usr/lib" -Wl,-z,relro -Wl,-z,now -Wl,-z,noexecstack \
    -Wl,-z,retpolineplt -pie -lc -lclang_rt.builtins \
    "$sysroot/usr/lib/crtn.o" -o "$out/accept-probe"
clang --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -D__ravenbsd__=1 -D__unix__=1 -fno-stack-protector -std=gnu11 -O1 \
    -c tests/fixtures/sshd-trace/logdump.c -o "$out/logdump.o"
clang --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
    -nostdlib -nostartfiles -static -Ttests/libc/target/x86_64.ld \
    -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
    "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/logdump.o" \
    -L"$sysroot/usr/lib" -Wl,-z,relro -Wl,-z,now -Wl,-z,noexecstack \
    -Wl,-z,retpolineplt -pie -lc -lclang_rt.builtins \
    "$sysroot/usr/lib/crtn.o" -o "$out/logdump"

cargo build -q -p raven-b70-overlay
overlay=${CARGO_TARGET_DIR:-target}/debug/raven-b70-overlay
"$overlay" build/x86_64/initfs.img "$out/initfs.img" \
    "/etc/ssh/sshd_config=$root/tests/fixtures/sshd-trace/sshd_config:0644" \
    "/etc/services.d/sshd.toml=$root/tests/fixtures/sshd-trace/sshd.service.toml:0644" \
    "/etc/services.d/sshd-trace-accept.toml=$root/tests/fixtures/sshd-trace/sshd-trace-accept.service.toml:0644" \
    "/etc/raven/conf.d/network.toml=$root/tests/fixtures/sshd-trace/network-static.toml:0644" \
    "/sbin/sshd=$root/build/ssh/x86_64/bin/sshd:0755" \
    "/bin/ssh=$root/build/ssh/x86_64/bin/ssh:0755" \
    "/usr/libexec/sshd-session=$root/build/ssh/x86_64/libexec/sshd-session:0755" \
    "/usr/libexec/sshd-auth=$root/build/ssh/x86_64/libexec/sshd-auth:0755" \
    "/bin/sshd-trace-askpass=$out/askpass:0755" \
    "/bin/sshd-trace-accept=$out/accept-probe:0755" \
    "/bin/sshd-trace-logdump=$out/logdump:0755" \
    "/etc/ssh/trace-client.sh=$root/tests/fixtures/sshd-trace/trace-client.sh:0644"

image=$out/sshd-trace.img
if [ ! -f "$image" ] || [ "$out/initfs.img" -nt "$image" ]; then
    cargo xtask image --arch x86_64 --profile release \
        --initfs "$out/initfs.img" --output "$image"
fi

echo "sshd-trace: image $image"
