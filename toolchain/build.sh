#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# usage: toolchain/build.sh [--dry-run] [--stage 1|2]
#
# Fetch the pinned Rust source tarball, verify its SHA-256, apply the reviewed
# RavenBSD patch series, build the compiler for the host and install it under
# build/toolchain, then register it with rustup as `ravenbsd`.
#
# The script is idempotent: a verified archive, source stamp and installed
# prefix are reused.  A missing prerequisite, a wrong stage0 compiler or a hash
# mismatch fails with a clear message and never falls back to another release.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
TOOLCHAIN=$ROOT/toolchain
BUILD=$ROOT/build
SRC=$BUILD/toolchain-src
PREFIX=$BUILD/toolchain
STAMP_NAME=.raven-toolchain

stage=${TOOLCHAIN_STAGE:-1}
dry_run=0
while [ $# -gt 0 ]; do
	case $1 in
		--dry-run)
			dry_run=1
			shift
			;;
		--stage)
			shift
			stage=${1:-}
			shift
			;;
		--stage=*)
			stage=${1#--stage=}
			shift
			;;
		-h|--help)
			printf 'usage: %s [--dry-run] [--stage 1|2]\n' "$0"
			exit 0
			;;
		*)
			printf 'error: unknown argument: %s\n' "$1" >&2
			exit 2
			;;
	esac
done

case $stage in
	1|2) ;;
	*)
		printf 'error: --stage must be 1 or 2, got `%s`\n' "$stage" >&2
		exit 2
		;;
esac

version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$TOOLCHAIN/rust-version")
hash=$(sed -n 's/^sha256 = "\(.*\)"/\1/p' "$TOOLCHAIN/rust-version")
archive=rustc-$version-src.tar.xz
url=https://static.rust-lang.org/dist/$archive

show() {
	printf '+ %s\n' "$*"
}

require() {
	if ! command -v "$1" >/dev/null 2>&1; then
		printf 'error: missing host prerequisite: %s\n' "$1" >&2
		printf 'install it and rerun `make toolchain`\n' >&2
		exit 1
	fi
}

manifest() {
	{
		printf 'version=%s\n' "$version"
		printf 'archive-sha256=%s\n' "$hash"
		for patch in "$TOOLCHAIN"/patches/*.patch; do
			printf 'patch=%s %s\n' "${patch##*/}" \
				"$(sha256sum "$patch" | awk '{print $1}')"
		done
	} | sha256sum | awk '{print $1}'
}

link_toolchain() {
	if rustup toolchain list 2>/dev/null | grep -q '^ravenbsd'; then
		rustup toolchain unlink ravenbsd >/dev/null 2>&1 || true
	fi
	rustup toolchain link ravenbsd "$PREFIX"
}

if [ "$dry_run" -eq 1 ]; then
	show "download $url (sha256 $hash)"
	show "verify sha256 of $BUILD/$archive"
	show "rm -rf $SRC (when the source stamp does not match)"
	show "tar -xJf $BUILD/$archive -C $BUILD"
	show "mv $BUILD/rustc-$version-src $SRC"
	for patch in "$TOOLCHAIN"/patches/*.patch; do
		show "patch -d $SRC -p1 --forward < $patch"
	done
	show "cp $TOOLCHAIN/bootstrap.toml $SRC/bootstrap.toml"
	show "(cd $SRC && BOOTSTRAP_SKIP_TARGET_SANITY=1 ./x.py --set install.prefix=$PREFIX install --stage $stage)"
	show "rustup toolchain link ravenbsd $PREFIX"
	exit 0
fi

# Host prerequisites.  The toolchain build uses the host's standard Rust
# bootstrap and host C toolchain; neither becomes part of the RavenBSD scaffold.
require rustc
require rustup
require sha256sum
require tar
require xz
require patch
require python3
require cc
require git
if ! command -v curl >/dev/null 2>&1 && ! command -v wget >/dev/null 2>&1; then
	printf 'error: missing host prerequisite: curl or wget\n' >&2
	exit 1
fi

stage0=$(cd "$ROOT" && rustc --version | awk '{print $2}')
if [ "$stage0" != "$version" ]; then
	printf 'error: stage0 rustc is %s, pinned release is %s\n' "$stage0" "$version" >&2
	printf 'run from the repository root so rust-toolchain.toml selects the pinned toolchain\n' >&2
	exit 1
fi

fingerprint=$(manifest)
stamp="$PREFIX/$STAMP_NAME"

if [ -x "$PREFIX/bin/rustc" ] && [ -f "$stamp" ] \
	&& [ "$(cat "$stamp")" = "$fingerprint stage=$stage" ]; then
	printf 'toolchain: %s already installed at %s (stage %s)\n' \
		"$version" "$PREFIX" "$stage"
	link_toolchain
	exit 0
fi

mkdir -p "$BUILD"

if [ ! -f "$BUILD/$archive" ]; then
	printf 'toolchain: downloading %s\n' "$url"
	if command -v curl >/dev/null 2>&1; then
		curl -fL --output "$BUILD/$archive.part" "$url"
	else
		wget -O "$BUILD/$archive.part" "$url"
	fi
	mv "$BUILD/$archive.part" "$BUILD/$archive"
fi

actual=$(sha256sum "$BUILD/$archive" | awk '{print $1}')
if [ "$actual" != "$hash" ]; then
	rm -f "$BUILD/$archive"
	printf 'error: sha256 mismatch for %s: expected %s, got %s\n' \
		"$archive" "$hash" "$actual" >&2
	exit 1
fi
printf 'toolchain: archive sha256 ok (%s)\n' "$hash"

src_stamp="$SRC/.raven-toolchain-source"
if [ -f "$src_stamp" ] && [ "$(cat "$src_stamp")" = "$fingerprint" ] \
	&& [ -d "$SRC/library" ]; then
	printf 'toolchain: reusing patched source at %s\n' "$SRC"
else
	rm -rf "$SRC"
	tar -xJf "$BUILD/$archive" -C "$BUILD"
	mv "$BUILD/rustc-$version-src" "$SRC"
	for patch in "$TOOLCHAIN"/patches/*.patch; do
		printf 'toolchain: applying %s\n' "${patch##*/}"
		patch -d "$SRC" -p1 --forward < "$patch"
	done
	cp "$TOOLCHAIN/bootstrap.toml" "$SRC/bootstrap.toml"
	printf '%s\n' "$fingerprint" > "$src_stamp"
fi

# A prefix from an older stage or a failed install must not be mixed with the
# new component set; the build tree stays incremental.
rm -rf "$PREFIX"

# The stage0 compiler does not know the target that the patched source adds;
# the stage1 compiler built from this tree does, so only the sanity pre-check
# is skipped.
printf 'toolchain: building stage %s into %s\n' "$stage" "$PREFIX"
(
	cd "$SRC"
	BOOTSTRAP_SKIP_TARGET_SANITY=1 ./x.py --set "install.prefix=$PREFIX" \
		install --stage "$stage"
)

if [ ! -x "$PREFIX/bin/rustc" ]; then
	printf 'error: install finished but %s/bin/rustc is missing\n' "$PREFIX" >&2
	exit 1
fi
printf '%s stage=%s\n' "$fingerprint" "$stage" > "$stamp"
link_toolchain

printf 'RavenBSD toolchain %s installed at %s (rustup toolchain link ravenbsd)\n' \
	"$version" "$PREFIX"
