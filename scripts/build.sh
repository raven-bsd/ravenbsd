#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# usage: scripts/build.sh <dev|release> [arch]
#   dev      debug build: no optimisation, full debug info, overflow checks (big binaries, easy debugging)
#   release  smallest possible binaries: opt-level z, fat LTO, one codegen unit, panic=abort, stripped
# Builds everything that exists in the tree: host crates, the kernel, the UEFI loader and the user programs.
# Components that are not written yet are skipped with a note. Output: build/<arch>/.
set -eu
. "$(dirname "$0")/lib.sh"

profile=${1:-dev}
arch=${2:-x86_64}
case "$profile" in
	dev) pflag="" ;;
	release) pflag="--release" ;;
	*) die "profile must be dev or release, got '$profile'" ;;
esac
have cargo || die "cargo not found: run 'make setup' first"
cd "$ROOT"
repro_env
out="$ROOT/build/$arch"
mkdir -p "$out/user"
pdir=$([ "$profile" = release ] && echo release || echo debug)

log "host crates ($profile)"
# shellcheck disable=SC2086
cargo build $pflag

if [ -f kernel/Cargo.toml ]; then
	t=$(triple_for kernel "$arch")
	log "kernel $arch ($t, $profile)"
	# shellcheck disable=SC2086
	RUSTFLAGS="$(rustflags_for kernel "$arch")" cargo build $pflag -p "$(crate_name kernel)" \
		--target "$t" --target-dir "$ROOT/build/cargo/kernel-$arch"
	cp "$ROOT/build/cargo/kernel-$arch/$t/$pdir/$(crate_name kernel)" "$out/kernel.elf"
else
	log "kernel: not written yet, skipped"
fi

if [ -f boot/loader/Cargo.toml ]; then
	t=$(triple_for loader "$arch")
	name=$(crate_name boot/loader)
	log "loader $arch ($t, $profile)"
	# shellcheck disable=SC2086
	RUSTFLAGS="$(rustflags_for loader "$arch")" cargo build $pflag -p "$name" \
		--target "$t" --target-dir "$ROOT/build/cargo/loader-$arch"
	cp "$ROOT/build/cargo/loader-$arch/$t/$pdir/$name.efi" "$out/loader.efi"
else
	log "loader: not written yet, skipped"
fi

dirs=$(user_crate_dirs)
if [ -n "$dirs" ]; then
	t=$(triple_for user "$arch")
	pkgs=""
	for d in $dirs; do pkgs="$pkgs -p $(crate_name "$d")"; done
	log "user programs $arch ($t, $profile):$pkgs"
	# shellcheck disable=SC2086
	RUSTFLAGS="$(rustflags_for user "$arch")" cargo build $pflag $pkgs \
		--target "$t" --target-dir "$ROOT/build/cargo/user-$arch"
	for d in $dirs; do
		n=$(crate_name "$d")
		f="$ROOT/build/cargo/user-$arch/$t/$pdir/$n"
		[ -f "$f" ] && cp "$f" "$out/user/$n"
	done
else
	log "user programs: none yet, skipped"
fi

log "artifacts in build/$arch:"
find "$out" -type f -exec ls -l {} + 2>/dev/null | awk '{printf "  %10d  %s\n", $5, $9}' || true
log "done ($profile, $arch, build id: $RAVEN_BUILD_ID)"
