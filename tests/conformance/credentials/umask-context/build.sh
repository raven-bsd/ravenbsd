#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S730 credential conformance fixture and pack
# build/conformance/credentials/umask-context/initfs.img. `/init`, `/cred-client` and
# `/umask-client` are the same image; the program selects its role from argv[0]. The S179/S189
# credential image build stays in the parent directory and is not touched.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

out=build/conformance/credentials/umask-context
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
        ;;
    aarch64)
        target=aarch64-unknown-none-softfloat
        rustflags="-C relocation-model=static -C force-frame-pointers=yes -C link-arg=-T$root/userland/linker/aarch64.ld"
        ;;
    *)
        echo "cred-conformance fixture: unsupported architecture $arch" >&2
        exit 2
        ;;
esac
target_dir=${CARGO_TARGET_DIR:-$root/target}
binary="$target_dir/$target/release/raven-cred-conformance"

RUSTFLAGS="$rustflags" \
    cargo build --manifest-path "$root/tests/conformance/credentials/umask-context/Cargo.toml" \
    --target "$target" --release --target-dir "$target_dir"

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$binary:0755" \
    "/cred-client=$binary:0755" \
    "/umask-client=$binary:0755"
