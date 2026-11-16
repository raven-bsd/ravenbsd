#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the B23 file fixture and pack build/fixtures/linuxd-files/initfs.img:
#   /init             = the controller (starts initfsd, ramfsd, vfsd, linuxd)
#   /sbin/linuxd      = the Linux personality server
#   /sbin/initfsd     = the initfs: server
#   /sbin/ramfsd      = the ramfs: server
#   /sbin/vfsd        = the file: namespace server
#   /hello            = the static musl file probe
#   /data.txt         = the initfs file the probe reads
#   /etc/mounts       = `/` on initfs: and `/tmp` on ramfs:
#
# A missing `musl-gcc` fails the qtest instead of skipping it.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/linuxd-files
mkdir -p "$out"

if ! command -v musl-gcc >/dev/null 2>&1; then
    echo "linuxd-files: musl-gcc is not installed (install musl-tools; the case is not skipped)" >&2
    exit 1
fi

musl-gcc -static -O0 -o "$out/hello" tests/fixtures/linuxd/files.c

target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build \
    -p linuxd \
    -p raven-initfsd \
    -p raven-ramfsd \
    -p raven-vfsd \
    --target "$target" --release

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    CARGO_TARGET_DIR="$root/$target_dir" \
    cargo build --manifest-path tests/fixtures/linuxd/files-controller/Cargo.toml \
    --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-b23-linux-files" \
    /sbin/linuxd="$target_dir/$target/release/linuxd" \
    /sbin/initfsd="$target_dir/$target/release/raven-initfsd" \
    /sbin/ramfsd="$target_dir/$target/release/raven-ramfsd" \
    /sbin/vfsd="$target_dir/$target/release/raven-vfsd" \
    /hello="$out/hello" \
    /data.txt="$root/tests/fixtures/linuxd/files-data.txt" \
    /etc/mounts="$root/tests/fixtures/linuxd/files-mounts"
