#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S177 memory-map conformance image: the release initfs plus /bin/mmap-probe, wrapped in
# a normal release image. Only files under tests/conformance/memory-map/** are added; the product
# tree is unchanged.
#
# Usage: sh tests/conformance/memory-map/build.sh
# Output: build/conformance/memory-map/memory-map.img

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
out=build/conformance/memory-map
mkdir -p "$out"

echo "mmap-image: probe"
sh tests/conformance/memory-map/probe/build.sh

echo "mmap-image: release initfs"
cargo xtask initfs --arch "$arch" --profile release

echo "mmap-image: inject probe"
cargo build -q -p raven-b70-overlay
overlay=${CARGO_TARGET_DIR:-target}/debug/raven-b70-overlay
"$overlay" "build/$arch/initfs.img" "$out/initfs.img" \
    "/bin/mmap-probe=$out/mmap-probe:0755" \
    "/bin/s177-shell=$out/s177-shell:0755" \
    "/bin/s177-hostkeys=$out/s177-hostkeys:0755" \
    "/etc/passwd=$root/tests/conformance/memory-map/probe/passwd:0644" \
    "/etc/services.d/ssh-hostkeys.toml=$root/tests/conformance/memory-map/probe/ssh-hostkeys.service.toml:0644"

echo "mmap-image: release image"
cargo xtask image --arch "$arch" --profile release \
    --cmdline "console=ttyS0 loglevel=info test-exit=1" \
    --initfs "$out/initfs.img" \
    --output "$out/memory-map.img"

sha256sum "$out/memory-map.img"
echo "mmap-image: built $out/memory-map.img"
