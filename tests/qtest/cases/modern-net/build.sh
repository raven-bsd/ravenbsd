#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K202: build the release initfs (netd, getty/login, network tools), build the target `raven`
# admin binary, and repack the initfs with /bin/raven for the modern-net qtest.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

arch=x86_64
out=build/fixtures/modern-net
target=build/cargo/modern-net-image
mkdir -p "$out"

if [ ! -f build/x86_64/initfs.img ]; then
    cargo xtask initfs --arch "$arch" --profile release >/dev/null
fi
cargo xtask build user --arch "$arch" --release -p raven-init >/dev/null
sh tests/fixtures/modern-admin/build.sh "$arch" >/dev/null

cargo run --quiet --manifest-path tests/qtest/cases/modern-net/image/Cargo.toml \
    --target-dir "$target" --bin modern-net-image -- \
    build/x86_64/initfs.img "$out/initfs.img" \
    "/init=build/x86_64/user/raven-init" \
    "/bin/raven=build/fixtures/modern-admin/raven"
