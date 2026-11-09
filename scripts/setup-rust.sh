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

# rust-toolchain.toml pins builds INSIDE $ROOT only. The host's DEFAULT toolchain is the host's
# business: project scripts NEVER install, uninstall or re-default it (only the pinned $channel
# above is installed, which rustup would do on any in-repo cargo run anyway). Probe cargo outside
# the repo read-only; a corrupt default (component metadata present, binary missing) fails with
# "the 'cargo' binary ... is not applicable to the 'stable-...' toolchain" — warn and leave the
# fix to the host owner.
log "checking the default toolchain's cargo outside the repo (read-only probe)"
tmpd=$(mktemp -d)
trap 'rm -rf "$tmpd"' EXIT
if (cd "$tmpd" && cargo --version >/dev/null 2>&1 && rustc --version >/dev/null 2>&1); then
	printf '  ok       default-toolchain cargo %s\n' "$(cd "$tmpd" && cargo --version)"
else
	warn "default-toolchain cargo is broken OUTSIDE the repo (host issue, e.g. corrupt rustup component)."
	warn "Project scripts never modify the host/default toolchain. Fix the host manually, e.g.:"
	warn "  rustup toolchain uninstall <default-toolchain> && rustup toolchain install <default-toolchain>"
	warn "or work inside the repo (rust-toolchain.toml pins $channel) / with an explicit +$channel."
fi
