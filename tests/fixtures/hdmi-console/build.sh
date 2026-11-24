#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K386 HDMI-console qtest fixture.
#
# Runs the host checks (configured firmware mode, raven-fb blitter/scroll, mailbox fallback and
# the kernel UART fallback invariants), then builds a minimal test initfs that boots the
# production init/getty/login path plus the target probe. The qtest dialogue then proves the
# boot banner, boot screen and login on the serial stream whose Pi 4 kernel path mirrors the same
# bytes to the HDMI display buffer.
#
# No product file is modified. The fixture is selected by RAVEN_QT_ARCH from the qtest harness.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        linker="$root/userland/linker/x86_64.ld"
        ;;
    aarch64)
        target=aarch64-unknown-none-softfloat
        linker="$root/userland/linker/aarch64.ld"
        ;;
    *)
        echo "hdmi-console: unsupported arch: $arch" >&2
        exit 2
        ;;
esac

out="$root/build/fixtures/hdmi-console"
mkdir -p "$out"
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$linker"

echo "hdmi-console: host checks" >&2
cargo run -q --release --manifest-path tests/fixtures/hdmi-console/host-check/Cargo.toml \
    --target-dir "$out/host-target"

echo "hdmi-console: target services ($arch)" >&2
cargo xtask build user --arch "$arch" --release \
    -p raven-init -p raven-rsh -p raven-consd -p raven-vfsd -p raven-ramfsd -p raven-initfsd \
    -p raven-logd -p raven-nulld -p raven-zerod -p raven-randd -p raven-getty -p login

echo "hdmi-console: target probe ($arch)" >&2
RUSTFLAGS="$rustflags" cargo build -q --release \
    --manifest-path tests/fixtures/hdmi-console/probe/Cargo.toml \
    --target "$target" --target-dir "$out/target"

echo "hdmi-console: packing initfs" >&2
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$root/build/$arch/user/raven-init:0755" \
    "/bin/login=$root/build/$arch/user/login:0755" \
    "/sbin/getty=$root/build/$arch/user/raven-getty:0755" \
    "/bin/rsh=$root/build/$arch/user/raven-rsh:0755" \
    "/bin/brush=$root/build/$arch/user/raven-rsh:0755" \
    "/sbin/logd=$root/build/$arch/user/raven-logd:0755" \
    "/sbin/consd=$root/build/$arch/user/raven-consd:0755" \
    "/sbin/vfsd=$root/build/$arch/user/raven-vfsd:0755" \
    "/sbin/ramfsd=$root/build/$arch/user/raven-ramfsd:0755" \
    "/sbin/initfsd=$root/build/$arch/user/raven-initfsd:0755" \
    "/sbin/nulld=$root/build/$arch/user/raven-nulld:0755" \
    "/sbin/zerod=$root/build/$arch/user/raven-zerod:0755" \
    "/sbin/randd=$root/build/$arch/user/raven-randd:0755" \
    "/sbin/hdmi-console-probe=$out/target/$target/release/hdmi-console-probe:0755" \
    "/etc/passwd=$root/tests/fixtures/hdmi-console/etc/passwd:0644" \
    "/etc/group=$root/tests/fixtures/hdmi-console/etc/group:0644" \
    "/etc/shadow=$root/tests/fixtures/hdmi-console/etc/shadow:0600" \
    "/etc/profile=$root/tests/fixtures/hdmi-console/etc/profile:0644" \
    "/etc/motd=$root/tests/fixtures/hdmi-console/etc/motd:0644" \
    "/etc/mounts=$root/etc/mounts:0644" \
    "/etc/rc.conf=$root/tests/fixtures/hdmi-console/rc.conf:0644" \
    "/etc/svcd.conf=$root/tests/fixtures/hdmi-console/svcd.conf:0644" \
    "/etc/raven/raven.toml=$root/etc/raven/raven.toml:0644" \
    "/etc/raven/profiles/base.toml=$root/etc/raven/profiles/base.toml:0644" \
    "/etc/raven/profiles/tiny.toml=$root/etc/raven/profiles/tiny.toml:0644" \
    "/etc/raven/profiles/server.toml=$root/etc/raven/profiles/server.toml:0644" \
    "/etc/services.d/logd.toml=$root/etc/services.d/logd.toml:0644" \
    "/etc/services.d/consd.toml=$root/etc/services.d/consd.toml:0644" \
    "/etc/services.d/vfsd.toml=$root/etc/services.d/vfsd.toml:0644" \
    "/etc/services.d/ramfsd.toml=$root/etc/services.d/ramfsd.toml:0644" \
    "/etc/services.d/initfsd.toml=$root/etc/services.d/initfsd.toml:0644" \
    "/etc/services.d/nulld.toml=$root/etc/services.d/nulld.toml:0644" \
    "/etc/services.d/zerod.toml=$root/etc/services.d/zerod.toml:0644" \
    "/etc/services.d/randd.toml=$root/etc/services.d/randd.toml:0644" \
    "/etc/services.d/getty.toml=$root/tests/fixtures/hdmi-console/getty.toml:0644" \
    "/etc/services.d/hdmi-console.toml=$root/tests/fixtures/hdmi-console/probe.toml:0644" \
    "/etc/hdmi-console/config.txt=$out/config.txt:0644"

echo "hdmi-console: fixture $arch $out/initfs.img" >&2
