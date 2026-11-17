#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the kernel spawn-attribute fixture image (fd map, signal mask, pgroup).
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

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
		printf 'proc-spawn-attrs: unsupported architecture %s\n' "$arch" >&2
		exit 2
		;;
esac

out=build/fixtures/proc-spawn-attrs
mkdir -p "$out"

target_dir=${CARGO_TARGET_DIR:-$root/target}
case "$target_dir" in
	/*) ;;
	*) target_dir="$root/$target_dir" ;;
esac

RUSTFLAGS="$rustflags" CARGO_TARGET_DIR="$target_dir" \
	cargo build -p raven-proc-spawn-attrs --target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
	"$out/initfs.img" \
	"/init=$target_dir/$target/release/raven-proc-attrs-init" \
	"/bin/proc-attrs-child=$target_dir/$target/release/raven-proc-attrs-child:0755"
