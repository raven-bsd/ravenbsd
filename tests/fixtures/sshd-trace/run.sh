#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K498: build the DEBUG3 sshd overlay initfs and the release image, then run the
# host/guest trace over a private QEMU user-net host forward on port 2498. The
# shared Ken matrix tap/bridge is not used.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

sh tests/fixtures/sshd-trace/build.sh

out=build/fixtures/sshd-trace
image=$out/sshd-trace.img
if [ ! -f "$image" ] || [ "$out/initfs.img" -nt "$image" ]; then
    cargo xtask image --arch x86_64 --profile release \
        --initfs "$out/initfs.img" --output "$image"
fi

python3 tests/fixtures/sshd-trace/trace.py
