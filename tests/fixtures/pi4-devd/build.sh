#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the P8 devd platform-launch fixture:
#   /init                 raven-init (svcd)
#   /sbin/devd            devd with the root-device grant
#   /sbin/fake-platform   the synthetic platform driver
# plus the fixture DTB, drivers.toml, service manifest and svcd.conf.

set -eu
arch=${RAVEN_QT_ARCH:-aarch64}
if [ "$arch" != aarch64 ]; then
    echo "pi4-devd fixture is aarch64-only (requested $arch)" >&2
    exit 2
fi
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/pi4-devd
mkdir -p "$out"
python3 - "$out" <<'PY'
import sys
from pathlib import Path

source = Path("tests/fixtures/pi4-devd/pi4-devd.dtb.hex")
text = "".join(line for line in source.read_text().splitlines() if not line.startswith("#"))
Path(sys.argv[1], "pi4-devd.dtb").write_bytes(bytes.fromhex(text))
PY

target=aarch64-unknown-none-softfloat
target_dir=${CARGO_TARGET_DIR:-target}
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/aarch64.ld"

RUSTFLAGS="$rustflags" \
    cargo build -p raven-init -p raven-devd -p pi4-devd-fake-driver \
    --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-init:0755" \
    "/sbin/devd=$target_dir/$target/release/raven-devd:0755" \
    "/sbin/fake-platform=$target_dir/$target/release/fake-platform:0755" \
    "/etc/pi4.dtb=$out/pi4-devd.dtb:0644" \
    "/etc/drivers.toml=$root/tests/fixtures/pi4-devd/etc/drivers.toml:0644" \
    "/etc/services.d/devd.toml=$root/tests/fixtures/pi4-devd/etc/devd.toml:0644" \
    "/etc/svcd.conf=$root/tests/fixtures/pi4-devd/etc/svcd.conf:0644"
