#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the procd fixture image. PROCD_MODE is `exec`, `signal` or `attrs`; it selects the client
# case through an initfs file and the output image name.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

mode=${PROCD_MODE:-exec}
case "$mode" in
	exec|signal|attrs) ;;
	*)
		printf 'PROCD_MODE must be exec, signal or attrs\n' >&2
		exit 2
		;;
esac

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
		printf 'procd fixture: unsupported architecture %s\n' "$arch" >&2
		exit 2
		;;
esac

out=build/fixtures/procd
mkdir -p "$out"
printf '%s\n' "$mode" > "$out/mode-$mode"

target_dir=${CARGO_TARGET_DIR:-$root/target}
case "$target_dir" in
	/*) ;;
	*) target_dir="$root/$target_dir" ;;
esac

RUSTFLAGS="$rustflags" CARGO_TARGET_DIR="$target_dir" \
	cargo build -p raven-procd -p raven-procd-fixture -p raven-initfsd \
	--target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
	"$out/initfs-$mode.img" \
	"/init=$target_dir/$target/release/raven-procd-init" \
	"/sbin/initfsd=$target_dir/$target/release/raven-initfsd:0755" \
	"/sbin/procd=$target_dir/$target/release/procd:0755" \
	"/bin/procd-child=$target_dir/$target/release/raven-procd-child:0755" \
	"/etc/procd-mode=$out/mode-$mode:0644"
