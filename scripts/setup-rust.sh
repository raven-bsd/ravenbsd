#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Installs rustup (if missing) and the toolchain pinned in rust-toolchain.toml, with its components and
# cross targets. Stable only (DECISIONS.md DR-02). Distribution independent. Honours DRY=1.
set -eu
. "$(dirname "$0")/lib.sh"
DRY=${DRY:-0}

run() { if [ "$DRY" -eq 1 ]; then echo "+ $*"; else "$@"; fi; }

if ! have rustup; then
	log "installing rustup (no default toolchain; the pinned one is installed next)"
	have curl || die "curl is required to install rustup"
	if [ "$DRY" -eq 1 ]; then
		echo "+ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none --profile minimal"
	else
		curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none --profile minimal
		# shellcheck disable=SC1091
		. "$HOME/.cargo/env"
	fi
fi

channel=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$ROOT/rust-toolchain.toml" | head -n 1)
[ -n "$channel" ] || die "no channel in rust-toolchain.toml"
case "$channel" in
	[0-9]*.[0-9]*.[0-9]*) ;;
	*) die "rust-toolchain.toml channel must be a pinned stable version (x.y.z), got '$channel'" ;;
esac

components=$(sed -n 's/^components *= *\[\(.*\)\]/\1/p' "$ROOT/rust-toolchain.toml" | tr -d ' "' | tr ',' ' ')
targets=$(awk '/^targets *= *\[/{f=1;next} f&&/\]/{f=0} f{print}' "$ROOT/rust-toolchain.toml" | tr -d ' ",')
cflags=""; for c in $components; do cflags="$cflags -c $c"; done
tflags=""; for t in $targets; do tflags="$tflags -t $t"; done

log "installing Rust $channel (components:$cflags; targets:$tflags)"
# shellcheck disable=SC2086
run rustup toolchain install "$channel" --profile minimal --no-self-update $cflags $tflags
if [ "$DRY" -eq 0 ]; then
	(cd "$ROOT" && rustc --version && cargo --version)
fi
