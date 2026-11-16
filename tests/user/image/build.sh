#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the U5 user-tools qtest initfs: the seven U2 tools, a writable ramfs /etc, the
# log/initfs/ramfs/vfs/console servers and rsh. su/passwd are packed 4755 to exercise the
# U3b setuid path. The custom init copies the account files onto the ramfs before rsh starts.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/user/image
mkdir -p "$out"

target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-target}
release_dir="$target_dir/$target/release"
# sha2 0.11.1 hits an LLVM codegen bug on x86_64-unknown-none; the portable backend is enough
# for the test hashes and lets su/passwd link.
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld --cfg sha2_backend=\"soft\""

RUSTFLAGS="$rustflags" cargo build --release --target "$target" \
    -p raven-logd -p raven-initfsd -p raven-ramfsd -p raven-nulld -p raven-zerod -p raven-randd \
    -p raven-vfsd -p raven-consd -p raven-rsh \
    -p useradd -p adduser -p passwd -p groupadd -p su -p id -p whoami -p raven-user-image

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
    "$out/initfs.img" \
    "/init=$release_dir/user-image-init" \
    "/sbin/logd=$release_dir/raven-logd" \
    "/sbin/initfsd=$release_dir/raven-initfsd" \
    "/sbin/ramfsd=$release_dir/raven-ramfsd" \
    "/sbin/nulld=$release_dir/raven-nulld" \
    "/sbin/zerod=$release_dir/raven-zerod" \
    "/sbin/randd=$release_dir/raven-randd" \
    "/sbin/vfsd=$release_dir/raven-vfsd" \
    "/sbin/consd=$release_dir/raven-consd" \
    "/bin/rsh=$release_dir/raven-rsh" \
    "/bin/useradd=$release_dir/useradd" \
    "/bin/adduser=$release_dir/adduser" \
    "/bin/passwd=$release_dir/passwd:4755" \
    "/bin/groupadd=$release_dir/groupadd" \
    "/bin/su=$release_dir/su:4755" \
    "/bin/id=$release_dir/id" \
    "/bin/whoami=$release_dir/whoami" \
    "/etc/passwd=$root/etc/passwd:0644" \
    "/etc/group=$root/etc/group:0644" \
    "/etc/shadow=$root/etc/shadow:0600"

printf 'user-tools image: %s\n' "$out/initfs.img"
