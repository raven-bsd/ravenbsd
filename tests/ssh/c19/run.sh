#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# C19 conformance red/green runner (test-only). Builds the two fixtures against
# the current libc sysroot, stages them into a copy of an existing release
# image's initfs, assembles a test image, boots it with KVM and prints the
# C19 lines. No product source is changed; no image-wide userland rebuild.
#
# Usage:
#   RAVEN_SSH_IMAGE=build/qtest/ssh-login.img \
#   RAVEN_C19_SYSROOT=build/ssh/x86_64/sysroot \
#   sh tests/ssh/c19/run.sh
#
# The current tree must be clean for `cargo xtask image`; the overlay output is
# kept under build/ssh/c19/.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"
base=${RAVEN_SSH_IMAGE:-build/qtest/ssh-login.img}
sysroot=${RAVEN_C19_SYSROOT:-build/ssh/x86_64/sysroot}
out=${RAVEN_C19_OUT:-build/ssh/c19}
image=$out/c19.img
initfs=$out/initfs.img
overlayed=$out/initfs-c19.img

[ -f "$base" ] || { echo "c19: missing base image $base" >&2; exit 2; }
[ -d "$sysroot" ] || { echo "c19: missing sysroot $sysroot" >&2; exit 2; }
[ -x target/debug/raven-b70-overlay ] || {
    echo "c19: missing target/debug/raven-b70-overlay" >&2; exit 2
}
mkdir -p "$out"

lld=$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/gcc-ld
build_one() {
    name=$1
    src=$2
    PATH="$lld:$PATH" clang --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
        -D__ravenbsd__=1 -D__unix__=1 -fno-stack-protector -std=gnu11 -O1 \
        -c "$src" -o "$out/$name.o"
    PATH="$lld:$PATH" clang --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
        -fuse-ld=lld -nostdlib -nostartfiles -static -Ttests/libc/target/x86_64.ld \
        -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
        "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" "$out/$name.o" \
        -L"$sysroot/usr/lib" -Wl,-z,relro -Wl,-z,now -Wl,-z,noexecstack \
        -Wl,-z,retpolineplt -pie -lc -lclang_rt.builtins \
        "$sysroot/usr/lib/crtn.o" -o "$out/$name"
}

build_one c19-signal tests/ssh/c19/c19_signal.c
build_one c19-exec tests/ssh/c19/c19_exec.c

mcopy -o -i "$base@@1048576" ::/raven/initfs.img "$initfs"
target/debug/raven-b70-overlay "$initfs" "$overlayed" \
    "/bin/c19-signal=$out/c19-signal:0755" \
    "/bin/c19-exec=$out/c19-exec:0755" \
    "/etc/services.d/c19-signal.toml=tests/ssh/c19/c19-signal.service.toml:0644" \
    "/etc/services.d/c19-exec.toml=tests/ssh/c19/c19-exec.service.toml:0644" \
    > "$out/overlay.log" 2>&1

timeout 600 cargo xtask image --arch x86_64 --profile release \
    --initfs "$overlayed" --output "$image" > "$out/image.log" 2>&1

vars=$out/vars.fd
serial=$out/serial.log
cp /usr/share/OVMF/OVMF_VARS_4M.fd "$vars"
: > "$serial"
qemu-system-x86_64 -machine q35 -m 512M -smp 4 -enable-kvm -cpu host \
    -serial "file:$serial" -display none -monitor none -no-reboot -snapshot \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd \
    -drive if=pflash,format=raw,file="$vars" \
    -drive format=raw,file="$image" \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 &
qpid=$!
trap 'kill "$qpid" 2>/dev/null || true; wait "$qpid" 2>/dev/null || true' EXIT
i=0
while [ "$i" -lt 200 ]; do
    if grep -q "C19 SIGNAL PASS\|C19 SIGNAL FAIL" "$serial" 2>/dev/null &&
        grep -q "C19 EXEC PASS\|C19 EXEC FAIL" "$serial" 2>/dev/null; then
        break
    fi
    i=$((i + 1))
    sleep 0.4
done
grep -a "C19 " "$serial" | tail -30
grep -q "C19 SIGNAL PASS" "$serial" && grep -q "C19 EXEC PASS" "$serial"
