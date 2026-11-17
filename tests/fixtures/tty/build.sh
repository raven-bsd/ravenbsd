#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the tty fixture. TTY_MODE is `pty`, `job` or `control`.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

mode=${TTY_MODE:-pty}
case "$mode" in
	pty|job|control) ;;
	*)
		printf 'TTY_MODE must be pty, job or control\n' >&2
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
		printf 'tty fixture: unsupported architecture %s\n' "$arch" >&2
		exit 2
		;;
esac

out=build/fixtures/tty
mkdir -p "$out"
printf '%s\n' "$mode" > "$out/mode-$mode"

target_dir=${CARGO_TARGET_DIR:-$root/target}
case "$target_dir" in
	/*) ;;
	*) target_dir="$root/$target_dir" ;;
esac

RUSTFLAGS="$rustflags" CARGO_TARGET_DIR="$target_dir" \
	cargo build -p raven-ptyd -p raven-ttyd -p raven-tty-fixture -p raven-initfsd \
	--target "$target" --release

cargo run -q -p raven-initfs --features std --example mkinitfs -- \
	"$out/initfs-$mode.img" \
	"/init=$target_dir/$target/release/raven-tty-init" \
	"/sbin/initfsd=$target_dir/$target/release/raven-initfsd:0755" \
	"/sbin/ptyd=$target_dir/$target/release/ptyd:0755" \
	"/sbin/ttyd=$target_dir/$target/release/ttyd:0755" \
	"/bin/tty-child=$target_dir/$target/release/raven-tty-child:0755" \
	"/bin/tty-ctl-child=$target_dir/$target/release/raven-tty-ctl-child:0755" \
	"/bin/tty-hup-child=$target_dir/$target/release/raven-tty-hup-child:0755" \
	"/etc/tty-mode=$out/mode-$mode:0644"
