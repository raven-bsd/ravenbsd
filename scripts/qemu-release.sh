#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Run the REL2b QEMU release login gate and keep its serial transcript.
# The release profile and login session are product inputs from REL2a/REL1; this script does not
# synthesize them.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64) ;;
    *)
        echo "qemu-release: only x86_64 is supported, got $arch" >&2
        exit 2
        ;;
esac

cargo xtask qtest --case release-login --arch "$arch"

out=build/release
mkdir -p "$out"
cp build/qtest/release-login.log "$out/qemu-release.log"
sh tests/fixtures/release/check-transcript.sh "$out/qemu-release.log"
printf 'qemu-release: transcript %s\n' "$out/qemu-release.log"
