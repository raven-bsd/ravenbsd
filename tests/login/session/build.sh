#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the REL1 login/getty binaries and a test-only initfs based on the
# shell-test profile. The overlay replaces the rsh service with getty and adds
# the root account/profile/motd fixture; the production /etc files belong to
# REL1c.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=$root/build/login/session
target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-$root/target}

mkdir -p "$out"

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p login -p raven-getty --release --target "$target"

RAVEN_USER_RELEASE=1 cargo xtask initfs --arch x86_64 --profile shell-test >/dev/null
cargo build -p raven-b70-overlay

"$target_dir/debug/raven-b70-overlay" \
    "$root/build/x86_64/initfs.img" \
    "$out/login-initfs.img" \
    "/bin/login=$target_dir/$target/release/login:755" \
    "/sbin/getty=$target_dir/$target/release/getty:755" \
    "/etc/passwd=$root/tests/login/session/etc/passwd:644" \
    "/etc/group=$root/tests/login/session/etc/group:644" \
    "/etc/shadow=$root/tests/login/session/etc/shadow:644" \
    "/etc/profile=$root/tests/login/session/etc/profile:644" \
    "/etc/motd=$root/tests/login/session/etc/motd:644" \
    "/etc/services.d/rsh.toml=$root/tests/login/session/getty.toml:644" \
    "/etc/svcd.conf=$root/tests/login/session/svcd.conf:644"

echo "login-session: $out/login-initfs.img"
