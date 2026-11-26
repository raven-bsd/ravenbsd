#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S715: build the sibling-exit probe as the one-shot libc-test service. The probe runs as init's
# service and keeps one user thread and one interruptible kernel thread running while the main
# thread exits; init must exit with the probe status.
#
# Usage: sh tests/conformance/memory-map/build-sibling.sh

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
out=$root/build/fixtures/s715-sibling-exit
mkdir -p "$out" "$root/build/libc-test"

sh tests/conformance/memory-map/probe/build.sh
cp "$root/build/conformance/memory-map/mmap-probe" "$root/build/libc-test/libc-test"
: > "$root/build/libc-test/libc-child"
: > "$root/build/libc-test/libplug.so"
: > "$root/build/libc-test/libc.so.1"
: > "$root/build/libc-test/ld-elf.so.1"
sh "$root/tests/libc/target/shape-check.sh" "$root/build/libc-test/libc-test"

cargo xtask initfs --arch "$arch" --profile libc-test >/dev/null
base=$root/build/"$arch"/initfs.img
cargo build -q -p raven-b70-overlay
"$root/target/debug/raven-b70-overlay" "$base" "$out/initfs.img" \
    "/bin/libc-test=$root/build/libc-test/libc-test:0755" \
    "/etc/svcd.conf=$root/tests/conformance/memory-map/probe/sibling-exit.svcd.conf:0644" \
    "/etc/services.d/libc-test.toml=$root/tests/conformance/memory-map/probe/sibling-exit.service.toml:0644"
printf 's715-sibling-exit: staged %s\n' "$out/initfs.img"
