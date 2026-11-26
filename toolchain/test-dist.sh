#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K548 rev351: test-first gate for the distributable ravenbsd toolchain.
#
# 1. Build the tarball with toolchain/dist.sh.
# 2. Unlink the local `ravenbsd` toolchain (build/toolchain).
# 3. Install the tarball into a clean temp prefix with toolchain/install.sh.
# 4. Build toolchain/smoke for x86_64-unknown-ravenbsd using only that prefix.
# 5. Restore the local link.
#
# The local build/toolchain must not be needed for step 4.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
have_local=0

restore() {
	rustup toolchain unlink ravenbsd >/dev/null 2>&1 || true
	if [ "$have_local" = 1 ]; then
		rustup toolchain link ravenbsd "$root/build/toolchain" >/dev/null 2>&1 || true
	fi
	rm -rf "$work"
}
trap restore EXIT HUP INT TERM

if rustup toolchain list | grep -q '^ravenbsd'; then
	have_local=1
fi

if [ ! -f "$root/toolchain/dist.sh" ]; then
	printf 'FAIL: toolchain/dist.sh is missing (the local toolchain is not distributable)\n' >&2
	exit 1
fi

sh "$root/toolchain/dist.sh" --output "$work/dist"
tarball=$(ls "$work"/dist/ravenbsd-toolchain-*.tar.xz 2>/dev/null | head -1)
[ -n "$tarball" ] || {
	printf 'FAIL: dist.sh produced no tarball\n' >&2
	exit 1
}

rustup toolchain unlink ravenbsd >/dev/null 2>&1 || true
prefix=$work/prefix
sh "$work/dist/install.sh" --tarball "$tarball" --prefix "$prefix"

link=$(rustup toolchain list -v | awk '$1 == "ravenbsd" { print $2; exit }')
case "$link" in
	"$prefix"*) ;;
	*)
		printf 'FAIL: ravenbsd links to %s, expected the temp prefix %s\n' "$link" "$prefix" >&2
		exit 1
		;;
esac

# The shipped prefix must not carry the build host's absolute paths.
if grep -rIl -e "$root" -e "${HOME:-/nonexistent-home}" "$prefix" >/dev/null 2>&1; then
	printf 'FAIL: the installed prefix contains a host-specific absolute path\n' >&2
	grep -rIl -e "$root" -e "${HOME:-/nonexistent-home}" "$prefix" >&2 || true
	exit 1
fi

cd "$root/toolchain/smoke"
rm -rf target
RUSTUP_TOOLCHAIN=ravenbsd CARGO_TARGET_DIR="$work/smoke-target" \
	cargo build --target x86_64-unknown-ravenbsd
smoke=$work/smoke-target/x86_64-unknown-ravenbsd/debug/smoke
readelf -h "$smoke" | grep -q 'Type:.*EXEC' || {
	printf 'FAIL: smoke is not a fixed-address EXEC (the target CRT/link script was not used): %s\n' "$smoke" >&2
	exit 1
}
readelf -h "$smoke" | grep -Eq 'Entry point address:[[:space:]]+0x[1-9a-fA-F]' || {
	printf 'FAIL: smoke has no entry point: %s\n' "$smoke" >&2
	exit 1
}

printf 'toolchain-dist: smoke %s\n' "$(readelf -h "$smoke" | awk '/Type:|Entry point address:/ { gsub(/^ +| +$/, ""); printf "%s ", $0 }')"
printf 'toolchain-dist: PASS\n'
