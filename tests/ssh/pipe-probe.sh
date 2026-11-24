#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K195 bounded platform probe: does the target libc `pipe(2)` work?
#
# Upstream OpenSSH session.c `do_exec_no_pty` allocates a command session's
# stdio with three `pipe(2)` calls, so the 0.1.0 `ssh guest uname -a` gate needs
# the libc primitive. This script builds tests/ssh/pipe-probe.c against the K200
# sysroot, boots it in a minimal initfs and prints the guest transcript.
#
# Exit codes:
#   0  pipe(2) works on the guest (data round-trips)
#   1  pipe(2) is missing (the documented K195 blocker: libc returns ENOSYS)
#   2  the probe could not run (missing sysroot/firmware/tools); run
#      `sh addons/ssh/build.sh` first
#
# Prerequisites: `sh addons/ssh/build.sh` (sysroot, OpenSSH libs) and a built
# `build/x86_64/user/raven-init` + kernel/loader from `cargo xtask image`.
#
# Usage: sh tests/ssh/pipe-probe.sh
# Outer acceptance timeout: 300 s (build + boot).

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

arch=x86_64
sysroot=$root/build/ssh/$arch/sysroot
out=$root/build/ssh
init=$root/build/$arch/user/raven-init
devnull=$root/tests/ssh/dev-null.keep

die() {
    echo "pipe-probe: $*" >&2
    exit 2
}

[ -d "$sysroot/usr/lib" ] || die "missing $sysroot; run sh addons/ssh/build.sh first"
[ -x "$init" ] || die "missing $init; build the release image once (cargo xtask image)"
command -v clang >/dev/null 2>&1 || die "clang is required"
command -v qemu-system-x86_64 >/dev/null 2>&1 || die "qemu-system-x86_64 is required"

# The link is lld-based; the distributed ld.lld may live in the Rust toolchain.
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
command -v ld.lld >/dev/null 2>&1 || die "ld.lld is required"

code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
[ -f "$code" ] || code=/usr/share/OVMF/OVMF_CODE.fd
vars_src=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
[ -f "$vars_src" ] || vars_src=/usr/share/OVMF/OVMF_VARS.fd
[ -f "$code" ] && [ -f "$vars_src" ] || die "OVMF firmware not found"

mkdir -p "$out"
probe=$out/pipe-probe
initfs=$out/pipe-probe-initfs.img
image=$out/pipe-probe.img
serial=$out/pipe-probe-serial.log

echo "pipe-probe: compile"
clang --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" \
    -D__ravenbsd__=1 -D__unix__=1 -fno-stack-protector -std=gnu11 -O1 \
    -c tests/ssh/pipe-probe.c -o "$out/pipe-probe.o"
clang --target=x86_64-unknown-ravenbsd --sysroot="$sysroot" -fuse-ld=lld \
    -nostdlib -nostartfiles -static -T tests/libc/target/x86_64.ld \
    -Wl,-z,max-page-size=4096 -Wl,-z,common-page-size=4096 \
    "$sysroot/usr/lib/crt1.o" "$sysroot/usr/lib/crti.o" \
    "$out/pipe-probe.o" -L"$sysroot/usr/lib" -lc -lclang_rt.builtins \
    "$sysroot/usr/lib/crtn.o" -o "$probe"

echo "pipe-probe: initfs"
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$initfs" \
    "/init=$init" \
    "/bin/pipe-probe=$probe" \
    "/dev/null=$devnull" \
    "/etc/services.d/pipe-probe.toml=$root/tests/ssh/pipe-probe.service.manifest" \
    "/etc/svcd.conf=$root/tests/ssh/pipe-probe.svcd.conf"

echo "pipe-probe: image"
cargo xtask image --arch "$arch" --profile base \
    --cmdline "console=ttyS0 test-exit=1" \
    --initfs "$initfs" --output "$image" >/dev/null

echo "pipe-probe: boot"
vars=$(mktemp "${TMPDIR:-/tmp}/raven-pipe-probe-XXXXXX")
cp "$vars_src" "$vars"
trap 'rm -f "$vars"' EXIT HUP INT TERM
: >"$serial"
timeout 120 qemu-system-x86_64 -machine q35 -m 512M -smp 2 \
    -serial "file:$serial" -display none -monitor none -no-reboot \
    -drive "if=pflash,format=raw,readonly=on,file=$code" \
    -drive "if=pflash,format=raw,file=$vars" \
    -drive "format=raw,file=$image" \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
    -nic none || true

echo "pipe-probe: transcript"
tail -n 12 "$serial"

if tr -d '\r' <"$serial" | grep -qx 'pipe-probe: ok'; then
    echo "pipe-probe: PASS (pipe(2) works)"
    exit 0
fi
if tr -d '\r' <"$serial" | grep -qx 'pipe-probe: ENOSYS'; then
    echo "pipe-probe: BLOCKED (libc pipe(2) returns ENOSYS; see tests/ssh/pipe-probe.sh)"
    exit 1
fi
echo "pipe-probe: ERROR (no probe result in $serial)" >&2
exit 2
