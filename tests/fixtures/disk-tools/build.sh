#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S110 disk-tools fixture:
#
#   * static lsblk/newfs/gpart and the same-source svcctl/mount/umount controls, staged by
#     `profiles/release.d/75-disk-tools.toml`. The release userland is dynamic PIE, and the
#     libc `posix_spawn` path in `crates/raven-spawn` refuses an interpreter image
#     (`spawn_image_with_fdmap` returns NotFound), so a login-shell tool must be static. This is
#     the same static staging S99 uses for the UFS demo tools.
#   * a 16 MiB blank payload wrapped in a one-partition GPT by the repository's own
#     `raven-k54-mkimage`. No filesystem is written here; the guest `newfs` formats the device.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/disk-tools
tools=$out/tools
payload=$out/blank.img
image=$out/scratch.img

echo "disk-tools: building static tools"
rm -rf "$tools"
mkdir -p "$tools"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p lsblk -p newfs -p gpart -p raven-svcctl -p raven-mount -p raven-umount \
        --target x86_64-unknown-none --release
target_dir=${CARGO_TARGET_DIR:-target}
for tool in lsblk newfs gpart; do
    cp "$target_dir/x86_64-unknown-none/release/$tool" "$tools/$tool"
done
cp "$target_dir/x86_64-unknown-none/release/raven-svcctl" "$tools/disk-tool-svcctl"
cp "$target_dir/x86_64-unknown-none/release/raven-mount" "$tools/disk-tool-mount"
cp "$target_dir/x86_64-unknown-none/release/raven-umount" "$tools/disk-tool-umount"

echo "disk-tools: creating $image (16 MiB blank payload + GPT)"
rm -f "$payload" "$image"
truncate -s 16M "$payload"
cargo run -q -p raven-k54-mkimage -- "$payload" "$image"
rm -f "$payload"
echo "disk-tools: scratch disk ready"
