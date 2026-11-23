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

arch=${RAVEN_QT_ARCH:-${1:-x86_64}}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        linker=$root/userland/linker/x86_64.ld
        ;;
    aarch64)
        target=aarch64-unknown-none-softfloat
        linker=$root/userland/linker/aarch64.ld
        ;;
    *)
        echo "disk-tools: unsupported arch $arch" >&2
        exit 2
        ;;
esac

out=build/fixtures/disk-tools
tools=$out/$arch/tools
payload=$out/blank.img
image=$out/scratch.img

echo "disk-tools: building static tools for $arch"
rm -rf "$tools"
mkdir -p "$tools"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$linker" \
    cargo build -p lsblk -p newfs -p gpart -p raven-svcctl -p raven-mount -p raven-umount \
        --target "$target" --release
target_dir=${CARGO_TARGET_DIR:-target}
for tool in lsblk newfs gpart; do
    cp "$target_dir/$target/release/$tool" "$tools/$tool"
done
cp "$target_dir/$target/release/raven-svcctl" "$tools/disk-tool-svcctl"
cp "$target_dir/$target/release/raven-mount" "$tools/disk-tool-mount"
cp "$target_dir/$target/release/raven-umount" "$tools/disk-tool-umount"

# The UFS demo profile still stages the legacy arch-less path; keep it as an x86_64 mirror.
if [ "$arch" = x86_64 ]; then
    rm -rf "$out/tools"
    cp -a "$tools" "$out/tools"
fi

echo "disk-tools: creating $image (16 MiB blank payload + GPT)"
rm -f "$payload" "$image"
truncate -s 16M "$payload"
cargo run -q -p raven-k54-mkimage -- "$payload" "$image"
rm -f "$payload"
echo "disk-tools: scratch disk ready"
