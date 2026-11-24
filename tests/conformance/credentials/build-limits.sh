#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S190 limits image: the release initfs plus /bin/cred-probe staged as a one-shot svcd
# service running the kernel/libc rlimit and rusage rows as `raven`. Only files under
# tests/conformance/credentials/** are added; the product tree is unchanged.
#
# Usage: sh tests/conformance/credentials/build-limits.sh
# Output: build/conformance/credentials/credentials-limits.img

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
out=build/conformance/credentials
mkdir -p "$out"

echo "cred-limits: probe"
sh tests/conformance/credentials/probe/build.sh

echo "cred-limits: release initfs"
cargo xtask initfs --arch "$arch" --profile release

echo "cred-limits: stage probe and service"
python3 tests/conformance/credentials/stage.py \
    --input "build/$arch/initfs.img" \
    --probe "$out/cred-probe" \
    --output "$out/initfs-limits.img" \
    --add /etc/services.d/cred-limits.toml 0644 \
        tests/conformance/credentials/cred-limits.service.toml \
    --replace /etc/svcd.conf 0644 tests/conformance/credentials/cred-limits.svcd.conf

python3 tests/conformance/credentials/stage.py --roundtrip "$out/initfs-limits.img"

echo "cred-limits: release image"
cargo xtask image --arch "$arch" --profile release \
    --cmdline "console=ttyS0 loglevel=info test-exit=1" \
    --initfs "$out/initfs-limits.img" \
    --output "$out/credentials-limits.img"

sha256sum "$out/credentials-limits.img"
echo "cred-limits: built $out/credentials-limits.img"
