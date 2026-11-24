#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S189 service image: the release initfs plus /bin/cred-probe staged as a one-shot svcd
# service, wrapped in a normal release image. This path does not use a shell, so the S187 rows run
# even while the brush external-command path is broken. Only files under
# tests/conformance/credentials/** are added; the product tree is unchanged.
#
# Usage: sh tests/conformance/credentials/build-service.sh
# Output: build/conformance/credentials/credentials-service.img

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
out=build/conformance/credentials
mkdir -p "$out"

echo "cred-service: probe"
sh tests/conformance/credentials/probe/build.sh

echo "cred-service: release initfs"
cargo xtask initfs --arch "$arch" --profile release

echo "cred-service: stage probe and service"
python3 tests/conformance/credentials/stage.py \
    --input "build/$arch/initfs.img" \
    --probe "$out/cred-probe" \
    --output "$out/initfs-service.img" \
    --add /etc/services.d/cred-umask.toml 0644 \
        tests/conformance/credentials/cred-umask.service.toml \
    --replace /etc/svcd.conf 0644 tests/conformance/credentials/cred.svcd.conf

python3 tests/conformance/credentials/stage.py --roundtrip "$out/initfs-service.img"

echo "cred-service: release image"
cargo xtask image --arch "$arch" --profile release \
    --cmdline "console=ttyS0 loglevel=info test-exit=1" \
    --initfs "$out/initfs-service.img" \
    --output "$out/credentials-service.img"

sha256sum "$out/credentials-service.img"
echo "cred-service: built $out/credentials-service.img"
