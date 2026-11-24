#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K548 rev351: install the distributable ravenbsd toolchain.
#
# usage: install.sh [--tarball FILE] [--prefix DIR]
#
# Extracts the relocatable toolchain prefix and registers it with rustup as `ravenbsd`.
# The default prefix is $HOME/.ravenbsd/toolchain. No host-specific absolute path is written into
# the prefix (the rustc/cargo launchers use $ORIGIN/../lib).

set -eu

prefix=${RAVEN_TOOLCHAIN_PREFIX:-${HOME:-/tmp}/.ravenbsd/toolchain}
tarball=

while [ $# -gt 0 ]; do
	case "$1" in
		--prefix)
			shift
			prefix=${1:?--prefix needs a directory}
			shift
			;;
		--prefix=*)
			prefix=${1#--prefix=}
			shift
			;;
		--tarball)
			shift
			tarball=${1:?--tarball needs a file}
			shift
			;;
		--tarball=*)
			tarball=${1#--tarball=}
			shift
			;;
		-h|--help)
			printf 'usage: %s [--tarball FILE] [--prefix DIR]\n' "$0"
			exit 0
			;;
		*)
			printf 'error: unknown argument: %s\n' "$1" >&2
			exit 2
			;;
	esac
done

if ! command -v rustup >/dev/null 2>&1; then
	printf 'error: rustup is required (https://rustup.rs)\n' >&2
	exit 1
fi

if [ -z "$tarball" ]; then
	tarball=$(ls "$(dirname "$0")"/ravenbsd-toolchain-*.tar.xz 2>/dev/null | head -1)
fi
if [ -z "$tarball" ] || [ ! -f "$tarball" ]; then
	printf 'error: tarball not found; pass --tarball FILE\n' >&2
	exit 1
fi

dir=$(CDPATH= cd -- "$(dirname "$tarball")" && pwd)
base=$(basename "$tarball")
if [ -f "$dir/SHA256SUMS" ]; then
	(cd "$dir" && sha256sum -c SHA256SUMS --ignore-missing)
fi

mkdir -p "$prefix"
tar -C "$prefix" -xJf "$dir/$base"

rustup toolchain link ravenbsd "$prefix"
rustup run ravenbsd rustc --version
