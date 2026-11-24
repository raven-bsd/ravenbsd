#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S179 conformance image: the release initfs plus /bin/cred-probe, wrapped in a normal
# release image. Only files under tests/conformance/credentials/** are added; the product tree is
# unchanged.
#
# Usage: sh tests/conformance/credentials/build.sh
# Output: build/conformance/credentials/credentials.img

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
out=build/conformance/credentials
mkdir -p "$out"

echo "cred-image: probe"
sh tests/conformance/credentials/probe/build.sh

echo "cred-image: release initfs"
cargo xtask initfs --arch "$arch" --profile release

echo "cred-image: inject probe"
python3 tests/conformance/credentials/stage.py \
    --input "build/$arch/initfs.img" \
    --probe "$out/cred-probe" \
    --output "$out/initfs.img"

python3 tests/conformance/credentials/stage.py --roundtrip "$out/initfs.img"

echo "cred-image: release image"
cargo xtask image --arch "$arch" --profile release \
    --cmdline "console=ttyS0 loglevel=info test-exit=1" \
    --initfs "$out/initfs.img" \
    --output "$out/credentials.img"

sha256sum "$out/credentials.img"
echo "cred-image: built $out/credentials.img"
