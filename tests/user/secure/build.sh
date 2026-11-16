#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the U3e user-secure fixture image. The image carries setuid su/passwd, the filesystem and
# console services, a writable /etc ramfs mount, and the account fixtures.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/user-secure
mkdir -p "$out"

target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-$root/target}
case "$target_dir" in
	/*) ;;
	*) target_dir="$root/$target_dir" ;;
esac
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"

RUSTFLAGS="$rustflags" CARGO_TARGET_DIR="$target_dir" \
	cargo build -p su -p passwd -p raven-initfsd -p raven-ramfsd -p raven-vfsd -p raven-consd \
	--target "$target" --release

(
	cd tests/user/secure
	RUSTFLAGS="$rustflags" CARGO_TARGET_DIR="$target_dir" \
		cargo build -p raven-user-secure-fixture --target "$target" --release
)

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
	"$out/initfs.img" \
	"/init=$target_dir/$target/release/raven-user-secure-init" \
	"/bin/runner=$target_dir/$target/release/raven-user-secure-runner:0755" \
	"/bin/brush=$target_dir/$target/release/raven-user-secure-shell:0755" \
	"/bin/idprobe=$target_dir/$target/release/raven-user-secure-probe:0755" \
	"/bin/envprobe=$target_dir/$target/release/raven-user-secure-probe:0755" \
	"/bin/su=$target_dir/$target/release/su:4755" \
	"/bin/passwd=$target_dir/$target/release/passwd:4755" \
	"/sbin/initfsd=$target_dir/$target/release/raven-initfsd:0755" \
	"/sbin/ramfsd=$target_dir/$target/release/raven-ramfsd:0755" \
	"/sbin/vfsd=$target_dir/$target/release/raven-vfsd:0755" \
	"/sbin/consd=$target_dir/$target/release/raven-consd:0755" \
	"/etc/passwd=$root/tests/user/secure/fixtures/passwd:0644" \
	"/etc/group=$root/tests/user/secure/fixtures/group:0644" \
	"/etc/shadow=$root/tests/user/secure/fixtures/shadow:0600" \
	"/etc/mounts=$root/tests/user/secure/fixtures/mounts:0644"
