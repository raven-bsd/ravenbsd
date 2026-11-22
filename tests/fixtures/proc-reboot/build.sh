#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build one PROC_REBOOT fixture mode and pack it as /init in its own initfs:
#
#   sh tests/fixtures/proc-reboot/build.sh denied     -> build/fixtures/proc-reboot/denied.img
#   sh tests/fixtures/proc-reboot/build.sh poweroff   -> build/fixtures/proc-reboot/poweroff.img

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

mode=${1:-denied}
case $mode in
    denied | poweroff) ;;
    *) echo "proc-reboot: unknown mode: $mode" >&2; exit 2 ;;
esac

bin=proc-reboot-$mode
out=build/fixtures/proc-reboot
mkdir -p "$out"
RUSTFLAGS="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/tests/fixtures/proc-reboot/linker.ld" \
    cargo build -p raven-proc-reboot --bin "$bin" --target x86_64-unknown-none --release
cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/$mode.img" \
    "/init=target/x86_64-unknown-none/release/$bin"
