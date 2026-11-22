#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K202/K219: build the release initfs (netd, getty/login, network tools), build the target
# `raven` admin binary, and repack the initfs with /usr/sbin/raven for the modern-net qtest.
#
# K219: never reuse the shared build/x86_64/initfs.img. modern-admin (or its qtest image build)
# can leave the admin/rsh initfs there, which makes modern-net boot `rsh: ready` instead of
# `login:`. Build the release initfs after the admin binary, copy it to the case-specific
# $out/release-initfs.img, and overlay from that copy.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

arch=x86_64
out=build/fixtures/modern-net
target=build/cargo/modern-net-image
mkdir -p "$out"

cargo xtask build user --arch "$arch" --release -p raven-init >/dev/null
sh tests/fixtures/modern-admin/build.sh "$arch" >/dev/null

echo "modern-net: building the case-specific release initfs"
RAVEN_USER_RELEASE=1 cargo xtask initfs --arch "$arch" --profile release >/dev/null
base=$out/release-initfs.img
cp build/x86_64/initfs.img "$base"

cargo run --quiet --manifest-path tests/qtest/cases/modern-net/image/Cargo.toml \
    --target-dir "$target" --bin modern-net-image -- \
    "$base" "$out/initfs.img" \
    "/init=build/x86_64/user/raven-init" \
    "/usr/sbin/raven=build/fixtures/modern-admin/raven"
