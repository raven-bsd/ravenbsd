#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# usage: toolchain/build.sh [--dry-run]
#
# Downloads the pinned Rust source tarball, verifies its SHA-256, applies the
# RavenBSD patches, runs `./x.py install`, installs into build/toolchain and
# registers it with rustup as `ravenbsd`.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
TOOLCHAIN=$ROOT/toolchain
BUILD=$ROOT/build
SRC=$BUILD/toolchain-src
PREFIX=$BUILD/toolchain

dry_run=0
for arg in "$@"; do
	case $arg in
		--dry-run) dry_run=1 ;;
		-h|--help)
			printf 'usage: %s [--dry-run]\n' "$0"
			exit 0
			;;
		*)
			printf 'error: unknown argument: %s\n' "$arg" >&2
			exit 2
			;;
	esac
done

version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$TOOLCHAIN/rust-version")
hash=$(sed -n 's/^sha256 = "\(.*\)"/\1/p' "$TOOLCHAIN/rust-version")
archive=rustc-$version-src.tar.xz
url=https://static.rust-lang.org/dist/$archive

show() {
	printf '+ %s\n' "$*"
}

mkdir -p "$BUILD"

if [ "$dry_run" -eq 1 ]; then
	show "download $url (sha256 $hash)"
else
	if [ ! -f "$BUILD/$archive" ]; then
		if command -v curl >/dev/null 2>&1; then
			curl -fL --output "$BUILD/$archive" "$url"
		else
			wget -O "$BUILD/$archive" "$url"
		fi
	fi
	actual=$(sha256sum "$BUILD/$archive" | awk '{print $1}')
	if [ "$actual" != "$hash" ]; then
		printf 'error: sha256 mismatch for %s: expected %s, got %s\n' \
			"$archive" "$hash" "$actual" >&2
		exit 1
	fi
fi

if [ "$dry_run" -eq 1 ]; then
	show "rm -rf $SRC"
	show "tar -xJf $BUILD/$archive -C $BUILD"
	show "mv $BUILD/rustc-$version-src $SRC"
else
	rm -rf "$SRC"
	tar -xJf "$BUILD/$archive" -C "$BUILD"
	mv "$BUILD/rustc-$version-src" "$SRC"
fi

for patch in "$TOOLCHAIN"/patches/*.patch; do
	if [ "$dry_run" -eq 1 ]; then
		show "patch -d $SRC -p1 --forward < $patch"
	else
		patch -d "$SRC" -p1 --forward < "$patch"
	fi
done

if [ "$dry_run" -eq 1 ]; then
	show "cp $TOOLCHAIN/bootstrap.toml $SRC/bootstrap.toml"
	show "(cd $SRC && BOOTSTRAP_SKIP_TARGET_SANITY=1 ./x.py install)"
	show "rm -rf $PREFIX"
	show "mv $SRC/build/toolchain $PREFIX"
	show "rustup toolchain link ravenbsd $PREFIX"
	exit 0
fi

cp "$TOOLCHAIN/bootstrap.toml" "$SRC/bootstrap.toml"

# The stage0 compiler does not know the target that the patched source adds; the
# stage1 compiler built from this tree does, so only the sanity pre-check needs
# to be skipped.
(
	cd "$SRC"
	BOOTSTRAP_SKIP_TARGET_SANITY=1 ./x.py install
)

rm -rf "$PREFIX"
mv "$SRC/build/toolchain" "$PREFIX"
rustup toolchain link ravenbsd "$PREFIX"

printf 'RavenBSD toolchain installed at %s (rustup toolchain link ravenbsd)\n' "$PREFIX"
