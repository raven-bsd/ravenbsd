#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the K68 lifecycle fixture and pack build/fixtures/k68-svc/initfs.img.
# The image is self-contained: raven-init is PID 1, the fixture binary provides the worker,
# crash and client services, and there are no profile restart overrides.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/k68-svc
mkdir -p "$out"

RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld" \
    cargo build -p raven-init -p raven-svcctl -p raven-k68-svc --target x86_64-unknown-none

target_dir=${CARGO_TARGET_DIR:-$root/target}
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/x86_64-unknown-none/debug/raven-init" \
    /sbin/k68svc="$target_dir/x86_64-unknown-none/debug/raven-k68-svc" \
    /bin/svcctl="$target_dir/x86_64-unknown-none/debug/raven-svcctl" \
    /etc/services.d/worker.toml="$root/tests/fixtures/k68-svc/worker.toml" \
    /etc/services.d/crash.toml="$root/tests/fixtures/k68-svc/crash.toml" \
    /etc/services.d/client.toml="$root/tests/fixtures/k68-svc/client.toml" \
    /etc/svcd.conf="$root/tests/fixtures/k68-svc/svcd.conf"
