#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S724: check the Rust target libc ABI against the authoritative model and the
# pinned FreeBSD donor.  The model/consistency leg is the generator check; the
# independent leg compiles the Rust target crate and a donor-header C probe for
# x86-64 and aarch64.  No C source is committed: the probe is generated below.
#
# usage: tests/conformance/abi-profile/check.sh [--no-donor]

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

donor=${RAVENBSD_RESOURCES:-}
if [ -z "$donor" ]; then
	for candidate in \
		"$root/../../ravenbsd-resources" \
		"$root/../ravenbsd-resources" \
		"$HOME/work/ravenos-workspace/ravenbsd-resources"; do
		if [ -f "$candidate/donors.lock" ]; then
			donor=$candidate
			break
		fi
	done
fi
if [ -z "$donor" ] || [ ! -f "$donor/donors.lock" ]; then
	printf 'abi-profile: cannot find the pinned donor checkout (set RAVENBSD_RESOURCES)\n' >&2
	exit 2
fi

python3 toolchain/gen-libc-consts.py generate --check
python3 toolchain/gen-libc-consts.py check
python3 -m unittest discover -s toolchain -p '*_ut.py'

for arch in x86_64 aarch64; do
	CARGO_TARGET_DIR="$root/build/abi-profile-probe" RUSTUP_TOOLCHAIN=ravenbsd \
		cargo build --offline --quiet \
		--manifest-path tests/conformance/abi-profile/rust-probe/Cargo.toml \
		--target "$arch-unknown-ravenbsd"
done

work=$(mktemp -d "${TMPDIR:-/tmp}/raven-abi-profile.XXXXXX")
trap 'rm -rf "$work"' EXIT
raw=$work/raw
mkdir -p "$raw"
git -C "$donor/freebsd" archive HEAD include sys/sys sys/netinet sys/netinet6 sys/net \
	sys/amd64/include sys/x86/include sys/arm64/include | tar -x -C "$raw"

python3 toolchain/gen-libc-consts.py emit-c-probe "$work/probe.c"
for arch in x86_64 aarch64; do
	tree=$work/donor-$arch
	mkdir -p "$tree/sys"
	cp -a "$raw/include/." "$tree/"
	cp -a "$raw/sys/sys/." "$tree/sys/"
	cp "$raw/sys/sys/errno.h" "$tree/errno.h"
	cp "$raw/sys/sys/fcntl.h" "$tree/fcntl.h"
	cp "$raw/sys/sys/poll.h" "$tree/poll.h"
	for name in netinet netinet6 net; do
		cp -a "$raw/sys/$name" "$tree/$name"
	done
	if [ "$arch" = x86_64 ]; then
		cp -a "$raw/sys/amd64/include" "$tree/machine"
		cp -a "$raw/sys/x86/include" "$tree/x86"
	else
		cp -a "$raw/sys/arm64/include" "$tree/machine"
	fi
	clang --target="$arch-unknown-freebsd14.0" -nostdinc -Wno-#warnings \
		-isystem "$(clang -print-resource-dir)/include" -isystem "$tree" \
		-D__BSD_VISIBLE=1 -fsyntax-only "$work/probe.c"
done

printf 'abi-profile: PASSED (x86_64, aarch64)\n'
