#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S190 limits image: the release initfs with the no_std Rust limits probe packed as
# /init, wrapped in a normal release image. The probe runs the kernel and libc rlimit/rusage rows
# as `raven` without a shell or a service manager. Only files under
# tests/conformance/credentials/** are added; the product tree is unchanged.
#
# Usage: sh tests/conformance/credentials/build-limits.sh
# Output: build/conformance/credentials/credentials-limits.img

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=x86_64
case "$arch" in
    x86_64)
        target=x86_64-unknown-none
        rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
        ;;
    *)
        echo "cred-limits: unsupported architecture $arch" >&2
        exit 1
        ;;
esac
out=build/conformance/credentials
mkdir -p "$out"
target_dir=${CARGO_TARGET_DIR:-target}

echo "cred-limits: probe"
RUSTFLAGS="$rustflags" \
    cargo build --manifest-path tests/conformance/credentials/limits-probe/Cargo.toml \
    --target-dir "$target_dir" -p raven-s190-limits --target "$target" --release

# The probe is a standalone package; its generated lockfile is not part of the tree.
rm -f tests/conformance/credentials/limits-probe/Cargo.lock

echo "cred-limits: release initfs"
cargo xtask initfs --arch "$arch" --profile release

echo "cred-limits: pack the probe as /init"
python3 tests/conformance/credentials/stage.py \
    --input "build/$arch/initfs.img" \
    --output "$out/initfs-limits.img" \
    --replace /init 0755 "$target_dir/$target/release/raven-s190-limits" \
    --add /etc/services.d/cred-limits.toml 0644 \
        tests/conformance/credentials/cred-limits.service.toml \
    --replace /etc/svcd.conf 0644 tests/conformance/credentials/cred-limits.svcd.conf

python3 tests/conformance/credentials/stage.py --roundtrip "$out/initfs-limits.img"

echo "cred-limits: release image"
cargo xtask image --arch "$arch" --profile release \
    --cmdline "console=ttyS0 loglevel=info test-exit=1" \
    --initfs "$out/initfs-limits.img" \
    --output "$out/credentials-limits.img"

sha256sum "$out/credentials-limits.img"
echo "cred-limits: built $out/credentials-limits.img"
