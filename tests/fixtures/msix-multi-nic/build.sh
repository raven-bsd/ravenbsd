#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S199 two-virtio-net initfs:
#   /init                 raven-init (svcd)
#   /sbin/pcid            the PCI enumerator with the pcid-only `root` grant
#   /sbin/devd            devd staged from the real etc/services.d/devd.toml
#   /sbin/virtio-net      the modern virtio-net function driver
# plus the two-function driver table and the one-shot svcd configuration.

set -eu
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "msix-multi-nic fixture is x86_64-only (requested $arch)" >&2
    exit 2
fi
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/msix-multi-nic
mkdir -p "$out"

cargo xtask build user --arch x86_64 --release \
    -p raven-init -p raven-devd -p raven-pcid -p virtio-net

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build --manifest-path tests/fixtures/msix-multi-nic/probe/Cargo.toml \
    --target x86_64-unknown-none --target-dir "$out/probe-target"
cp "$out/probe-target/x86_64-unknown-none/debug/msix-multi-nic-probe" "$out/msix-probe"
rm -f tests/fixtures/msix-multi-nic/probe/Cargo.lock

# Stage the real manifest so the grant under test is the product file. The sequential
# two-function case only changes the devd mode token and adds the driver's `nic:` scheme to the
# readiness set so the probe starts after the first function has registered it.
sed -e 's/^args = .*/args = ["devd", "test"]/' \
    -e 's/^provides = .*/provides = ["scheme:nic", "scheme:dev"]/' \
    "$root/etc/services.d/devd.toml" > "$out/devd.toml"
if ! grep -q '^args = \["devd", "test"\]$' "$out/devd.toml" \
    || ! grep -q '^provides = \["scheme:nic", "scheme:dev"\]$' "$out/devd.toml"; then
    echo "msix-multi-nic fixture: could not set the staged devd test mode" >&2
    exit 2
fi

user=build/x86_64/user
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$user/raven-init:0755" \
    "/sbin/pcid=$user/raven-pcid:0755" \
    "/sbin/devd=$user/raven-devd:0755" \
    "/sbin/virtio-net=$user/virtio-net:0755" \
    "/sbin/msix-probe=$out/msix-probe:0755" \
    "/etc/services.d/pcid.toml=$root/etc/services.d/pcid.toml:0644" \
    "/etc/services.d/devd.toml=$out/devd.toml:0644" \
    "/etc/services.d/msix-probe.toml=$root/tests/fixtures/msix-multi-nic/etc/msix-probe.toml:0644" \
    "/etc/drivers.toml=$root/tests/fixtures/msix-multi-nic/etc/drivers.toml:0644" \
    "/etc/svcd.conf=$root/tests/fixtures/msix-multi-nic/etc/svcd.conf:0644"
