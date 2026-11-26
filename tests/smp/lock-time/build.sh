#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S85a SMP lock/time/IPI fixture and pack build/smp/lock-time/initfs.img.
# `leak` and `leak-early` also build the boot-service image used by the frame-window cases.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

mode=isolated
case "${1:-}" in
    leak|leak-early) mode=$1 ;;
    x86_64|aarch64|riscv64*) ;;
esac
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    printf 'skipped: smp lock-time is x86_64 only\n'
    exit 77
fi

out=build/smp/lock-time
mkdir -p "$out"
CARGO_TARGET_DIR="$out/target" \
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/smp/lock-time/linker.ld" \
    cargo build --manifest-path tests/smp/lock-time/Cargo.toml --target x86_64-unknown-none --release

target="$out/target/x86_64-unknown-none/release/smp-lock-time-init"
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" /init="$target" /bin/smp-lock-time="$target"

if [ "$mode" = isolated ]; then
    exit 0
fi

# The boot-service cases run the same fixture as a svcd service beside the core boot services,
# so the kernel's `s36-lockcheck` diagnostic samples a real boot window. The early variant has no
# service dependency and starts before the resolver; the settled variant waits for the resolver
# and vfsd, so only this fixture's churn is inside the measurement window.
base=build/x86_64/initfs.img
if ! strings "$base" 2>/dev/null | grep -q '/sbin/vfsd'; then
    cargo xtask initfs --arch x86_64 --profile base
    base=build/x86_64/initfs.img
fi
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -q -p resolvd --target x86_64-unknown-none --release
resolvd=target/x86_64-unknown-none/release/resolvd
cargo build -q -p raven-b70-overlay
overlay=${CARGO_TARGET_DIR:-target}/debug/raven-b70-overlay
image="$out/leak-initfs.img"
manifest=tests/smp/lock-time/leak.service.toml
if [ "$mode" = leak-early ]; then
    image="$out/leak-early-initfs.img"
    manifest=tests/smp/lock-time/leak-early.service.toml
fi
"$overlay" "$base" "$image" \
    "/bin/smp-lock-time=$target:0755" \
    "/sbin/resolvd=$resolvd:0755" \
    "/etc/services.d/resolvd.toml=$root/tests/smp/lock-time/leak-resolvd.service.toml:0644" \
    "/etc/services.d/libc-test.toml=$root/tests/smp/lock-time/leak-no-libc-test.service.toml:0644" \
    "/etc/services.d/smp-lock-time-leak.toml=$manifest:0644" \
    "/etc/svcd.conf=$root/tests/smp/lock-time/leak.svcd.conf:0644"
