#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S194: build the release initfs and overlay a /etc/profile that exports RUST_BACKTRACE=full.
# The diagnostic image keeps the release userland intact; only the profile gains the backtrace
# switch so a brush panic names its exact std call path on the serial console.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

arch=${1:-x86_64}
[ "$arch" = x86_64 ] || { printf 's194-release-trace: only x86_64\n' >&2; exit 2; }

cargo xtask initfs --arch "$arch" --profile release >/dev/null

fixture=$root/build/fixtures/s194-release
mkdir -p "$fixture"
cargo build -q -p raven-b70-overlay
"$root/target/debug/raven-b70-overlay" "$root/build/$arch/initfs.img" "$fixture/initfs.img" \
    "/etc/profile=$root/tests/libc/signals/s194-shell-profile:0644"
printf 's194-release-trace: staged %s\n' "$fixture/initfs.img"
