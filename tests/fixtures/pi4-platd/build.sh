#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the P12 platd fixture:
#   /init                    raven-init (svcd)
#   /sbin/devd               devd with the root-device grant
#   /sbin/platd              the platform server, launched by devd
#   /bin/plat-client         the plat: scheme checks
# plus the P8 fixture DTB, a platd driver table and the service files.

set -eu

arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "pi4-platd fixture is x86_64-only (requested $arch)" >&2
    exit 2
fi

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/pi4-platd
mkdir -p "$out"
python3 - "$out" <<'PY'
import sys
from pathlib import Path

source = Path("tests/fixtures/pi4-devd/pi4-devd.dtb.hex")
text = "".join(line for line in source.read_text().splitlines() if not line.startswith("#"))
Path(sys.argv[1], "pi4-platd.dtb").write_bytes(bytes.fromhex(text))
PY

target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"

RUSTFLAGS="$rustflags" \
    cargo build -p raven-init -p raven-devd -p raven-platd -p raven-pi4-platd \
    --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$target_dir/$target/release/raven-init:0755" \
    "/sbin/devd=$target_dir/$target/release/raven-devd:0755" \
    "/sbin/platd=$target_dir/$target/release/raven-platd:0755" \
    "/bin/plat-client=$target_dir/$target/release/pi4-platd-client:0755" \
    "/etc/pi4.dtb=$out/pi4-platd.dtb:0644" \
    "/etc/drivers.toml=$root/tests/fixtures/pi4-platd/etc/drivers.toml:0644" \
    "/etc/services.d/devd.toml=$root/tests/fixtures/pi4-platd/etc/devd.toml:0644" \
    "/etc/services.d/plat-client.toml=$root/tests/fixtures/pi4-platd/etc/plat-client.toml:0644" \
    "/etc/svcd.conf=$root/tests/fixtures/pi4-platd/etc/svcd.conf:0644"
