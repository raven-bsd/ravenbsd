#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the P2e procd fixture image. PROCD_MODE is `exec` or `signal`; it selects the client case
# through an initfs file and the output image name.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

mode=${PROCD_MODE:-exec}
case "$mode" in
	exec|signal) ;;
	*)
		printf 'PROCD_MODE must be exec or signal\n' >&2
		exit 2
		;;
esac

out=build/fixtures/procd
mkdir -p "$out"
printf '%s\n' "$mode" > "$out/mode-$mode"

target=x86_64-unknown-none
target_dir=${CARGO_TARGET_DIR:-$root/target}
case "$target_dir" in
	/*) ;;
	*) target_dir="$root/$target_dir" ;;
esac
rustflags="-C code-model=small -C relocation-model=static -C lto=no -C link-arg=-T$root/userland/linker/x86_64.ld"

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
