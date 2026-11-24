#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S199 SIGACTION guarded-buffer conformance image: the release initfs plus
# /bin/sigact-guard and its one-shot svcd service, wrapped in a normal release image. Only files
# under tests/conformance/signals/** are added; the product tree is unchanged.
#
# Usage: sh tests/conformance/signals/build.sh
# Output: build/conformance/signals/signals.img

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
out=build/conformance/signals
mkdir -p "$out"

echo "signals-image: probe"
sh tests/conformance/signals/probe/build.sh

echo "signals-image: release initfs"
cargo xtask initfs --arch "$arch" --profile release

echo "signals-image: inject probe and one-shot service"
cargo build -q -p raven-b70-overlay
overlay=${CARGO_TARGET_DIR:-target}/debug/raven-b70-overlay
"$overlay" "build/$arch/initfs.img" "$out/initfs.img" \
    "/bin/sigact-guard=$out/sigact-guard:0755" \
    "/etc/services.d/sigact-guard.toml=$root/tests/conformance/signals/sigact-guard.service.toml:0644" \
    "/etc/svcd.conf=$root/tests/conformance/signals/sigact.svcd.conf:0644"

echo "signals-image: release image"
cargo xtask image --arch "$arch" --profile release \
    --cmdline "console=ttyS0 loglevel=info test-exit=1" \
    --initfs "$out/initfs.img" \
    --output "$out/signals.img"

sha256sum "$out/signals.img"
echo "signals-image: built $out/signals.img"
