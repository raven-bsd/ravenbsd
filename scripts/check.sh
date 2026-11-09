#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# usage: scripts/check.sh [arch]
# The local gate every ticket must pass: stable guard, fmt, clippy (host + per target), host tests,
# provenance (when the checker exists). Replaced by `cargo xtask check` (ticket K01).
set -eu
. "$(dirname "$0")/lib.sh"

arch=${1:-x86_64}
have cargo || die "cargo not found: run 'make setup' first"
cd "$ROOT"
repro_env
status=0
step() { name=$1; shift; log "$name"; if "$@"; then :; else warn "FAILED: $name"; status=1; fi; }

guard() {
	# no nightly in tracked files (this script and docs are exempt)
	hits=$(git ls-files 2>/dev/null | grep -v -e '^scripts/' -e '\.md$' -e '^xtask/' | \
		xargs grep -nE '#!?\[feature\(|RUSTC_BOOTSTRAP|\+nightly|-Zbuild-std' 2>/dev/null || true)
	if [ -n "$hits" ]; then printf '%s\n' "$hits"; return 1; fi
	grep -qE '^channel *= *"[0-9]+\.[0-9]+\.[0-9]+"' rust-toolchain.toml
}

step "stable guard" guard
step "fmt" cargo fmt --all -- --check
step "clippy (host)" cargo clippy --all-targets -- -D warnings
step "tests (host)" cargo test
if [ -d crates/raven-provenance ]; then
	step "provenance" cargo run -q -p raven-provenance -- check
else
	log "provenance: skipped (checker not merged yet, ticket B02)"
fi

if [ -f kernel/Cargo.toml ]; then
	t=$(triple_for kernel "$arch")
	step "clippy kernel $arch" env RUSTFLAGS="$(rustflags_for kernel "$arch")" cargo clippy -p "$(crate_name kernel)" \
		--target "$t" --target-dir "$ROOT/build/cargo/kernel-$arch" -- -D warnings
fi
if [ -f boot/loader/Cargo.toml ]; then
	t=$(triple_for loader "$arch")
	step "clippy loader $arch" env RUSTFLAGS="$(rustflags_for loader "$arch")" cargo clippy -p "$(crate_name boot/loader)" \
		--target "$t" --target-dir "$ROOT/build/cargo/loader-$arch" -- -D warnings
fi
dirs=$(user_crate_dirs)
if [ -n "$dirs" ]; then
	t=$(triple_for user "$arch")
	pkgs=""
	for d in $dirs; do pkgs="$pkgs -p $(crate_name "$d")"; done
	# shellcheck disable=SC2086
	step "clippy user programs $arch" env RUSTFLAGS="$(rustflags_for user "$arch")" cargo clippy $pkgs \
		--target "$t" --target-dir "$ROOT/build/cargo/user-$arch" -- -D warnings
fi

if [ "$status" -eq 0 ]; then log "check: all steps passed"; else warn "check: FAILED"; fi
exit "$status"
