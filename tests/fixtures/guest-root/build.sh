#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the S777 guest-root fixture and pack build/fixtures/guest-root/initfs.img.
#
# The guest runs the same service set as the S750 target runner: initfsd serves `/`, and a second
# ramfs instance carries the donor `/root` mode (0700). The G1 wrapper performs the boot-time
# `/root` setup; this probe starts the services and verifies the result through `file:`.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/guest-root
mkdir -p "$out"
arch=${RAVEN_QT_ARCH:-x86_64}
if [ "$arch" != x86_64 ]; then
    echo "guest-root fixture: x86_64 only" >&2
    exit 1
fi
target=x86_64-unknown-none
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"
target_dir=${CARGO_TARGET_DIR:-target}
manifest=tests/fixtures/guest-root/Cargo.toml

# The G1 service binaries and the wrapper (including its `/root` setup path) are the same
# artifacts the S750 target runner uses.
sh tests/fixtures/g1/build-guest.sh
guest=build/fixtures/g1/guest/bin

RUSTFLAGS="$rustflags" \
    cargo build --manifest-path "$manifest" --target-dir "$target_dir" \
    -p raven-guest-root-fixture --target "$target" --release

# vfsd reads the mount table from the initfs. Only the initfs root is mounted: the probe mounts
# the 0700 ramfs at /root through the wrapper, which is the behaviour under test.
printf '/ initfs:\n' > "$out/etc-mounts"

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    /init="$target_dir/$target/release/raven-guest-root-fixture" \
    /sbin/initfsd="$guest/initfsd" \
    /sbin/ramfsd="$guest/ramfsd" \
    /sbin/vfsd="$guest/vfsd" \
    /sbin/g1wrap="$guest/g1wrap" \
    /etc/mounts="$out/etc-mounts"

# The fixture is a standalone package; its generated lockfile is not part of the tree.
rm -f tests/fixtures/guest-root/Cargo.lock
