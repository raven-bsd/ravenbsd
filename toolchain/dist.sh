#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K548 rev351: build the distributable ravenbsd toolchain.
#
# usage: toolchain/dist.sh [--output DIR]
#
# The installed prefix is relocatable (the rustc/cargo launchers use $ORIGIN/../lib), so a plain
# tar.xz of build/toolchain is the distributable. Writes the tarball, SHA256SUMS, install.sh and a
# METADATA file (version, source tarball hash, size, host prerequisites and the from-source path).

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
prefix=${RAVEN_TOOLCHAIN_PREFIX:-$root/build/toolchain}
out=$root/toolchain/dist

while [ $# -gt 0 ]; do
	case "$1" in
		--output)
			shift
			out=${1:?--output needs a directory}
			shift
			;;
		--output=*)
			out=${1#--output=}
			shift
			;;
		-h|--help)
			printf 'usage: %s [--output DIR]\n' "$0"
			exit 0
			;;
		*)
			printf 'error: unknown argument: %s\n' "$1" >&2
			exit 2
			;;
	esac
done

if [ ! -x "$prefix/bin/rustc" ]; then
	printf 'error: no installed toolchain at %s; run `make toolchain` first\n' "$prefix" >&2
	exit 1
fi

version=$("$prefix/bin/rustc" --version | awk '{print $2}')
name=ravenbsd-toolchain-$version-x86_64-unknown-linux-gnu
mkdir -p "$out"

# Stage with hard links (cheap) so the install bookkeeping can be sanitized without touching the
# live prefix. `lib/rustlib/install.log` and the `manifest-*` files record the absolute build
# prefix; they are unused by a rustup-linked toolchain, and the shipped tarball must carry no
# host-specific absolute path.
stage=$root/build/toolchain-dist-stage
rm -rf "$stage"
mkdir -p "$stage"
cp -al "$prefix/." "$stage/"
trap 'rm -rf "$stage"' EXIT HUP INT TERM

rm -f "$stage/lib/rustlib/install.log"
for manifest in "$stage"/lib/rustlib/manifest-*; do
	[ -f "$manifest" ] || continue
	sed -i "s|file:$prefix/|file:|" "$manifest"
done

tar -C "$stage" -cf - . | xz -T0 -6 > "$out/$name.tar.xz"
sha=$(sha256sum "$out/$name.tar.xz" | awk '{print $1}')
size=$(wc -c < "$out/$name.tar.xz")
printf '%s  %s\n' "$sha" "$name.tar.xz" > "$out/SHA256SUMS"

cp "$root/toolchain/install.sh" "$out/install.sh"
chmod 0755 "$out/install.sh"

source_version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/toolchain/rust-version")
source_sha=$(sed -n 's/^sha256 = "\(.*\)"/\1/p' "$root/toolchain/rust-version")

{
	printf 'name=%s\n' "$name"
	printf 'version=%s\n' "$version"
	printf 'host=x86_64-unknown-linux-gnu\n'
	printf 'tarball=%s.tar.xz\n' "$name"
	printf 'sha256=%s\n' "$sha"
	printf 'size_bytes=%s\n' "$size"
	printf 'source_version=%s\n' "$source_version"
	printf 'source_tarball_sha256=%s\n' "$source_sha"
	printf 'from_source=make toolchain (toolchain/build.sh; rustc-1.97.1-src.tar.xz)\n'
	printf 'host_paths=none (lib/rustlib/install.log dropped; manifest-* rewritten relative)\n'
	printf 'install=sh install.sh --tarball %s.tar.xz --prefix <dir>\n' "$name"
	printf 'prerequisites=rustup; stock rustc 1.97.1; sha256sum tar xz patch python3 cc git curl; ~10 GB for a from-source build; gcc-aarch64-linux-gnu for aarch64\n'
} > "$out/METADATA"

printf '%s\n' "$out/$name.tar.xz"
