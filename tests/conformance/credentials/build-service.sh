#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S179 service image: the release initfs plus /bin/cred-probe staged as a one-shot svcd
# service, wrapped in a normal release image. This path does not use a shell, so the whole
# credential table runs even while the brush external-command path is broken. Only files under
# tests/conformance/credentials/** are added; the product tree is unchanged.
#
# Usage: sh tests/conformance/credentials/build-service.sh [--mutation]
# Output: build/conformance/credentials/credentials-service.img
#         build/conformance/credentials/credentials-service-mutation.img (--mutation)

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

mutation=0
case "${1:-}" in
    "")
        ;;
    --mutation)
        mutation=1
        ;;
    *)
        echo "usage: sh tests/conformance/credentials/build-service.sh [--mutation]" >&2
        exit 2
        ;;
esac

arch=x86_64
out=build/conformance/credentials
mkdir -p "$out"

extra=""
suffix=""
if [ "$mutation" = 1 ]; then
    extra="--add /etc/cred-mutation 0644 tests/conformance/credentials/cred-mutation.marker"
    suffix="-mutation"
fi

echo "cred-service: probe"
sh tests/conformance/credentials/probe/build.sh

echo "cred-service: release initfs"
cargo xtask initfs --arch "$arch" --profile release

echo "cred-service: stage probe and service"
python3 tests/conformance/credentials/stage.py \
    --input "build/$arch/initfs.img" \
    --probe "$out/cred-probe" \
    --output "$out/initfs-service$suffix.img" \
    $extra \
    --add /etc/services.d/cred-service.toml 0644 \
        tests/conformance/credentials/cred-service.service.toml \
    --replace /etc/svcd.conf 0644 tests/conformance/credentials/cred.svcd.conf

python3 tests/conformance/credentials/stage.py --roundtrip "$out/initfs-service$suffix.img"

echo "cred-service: release image"
cargo xtask image --arch "$arch" --profile release \
    --cmdline "console=ttyS0 loglevel=info test-exit=1" \
    --initfs "$out/initfs-service$suffix.img" \
    --output "$out/credentials-service$suffix.img"

sha256sum "$out/credentials-service$suffix.img"
echo "cred-service: built $out/credentials-service$suffix.img"
