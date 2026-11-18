#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Release-login loader freshness gate.
#
# The supported ld-elf.so.1 contract is `--image-base=0x1000`: the loader's first PT_LOAD is at
# 0x1000 and its self-relocation reads its header at AT_BASE + 0x1000. Rebuild the loader from
# the current archive, verify that shape, and record its hash for the qtest evidence. A stale
# vaddr-0 loader (the release-image regression shape) fails here with a loud message instead of
# turning into an unexplained login timeout.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

arch=${1:-x86_64}
loader="build/$arch/ld-elf.so.1"

cargo xtask libc-link --arch "$arch"

if [ ! -f "$loader" ]; then
    printf 'release-login: missing loader %s\n' "$loader" >&2
    exit 2
fi

first_load=$(readelf -lW "$loader" | awk '/^  LOAD / {print; exit}')
case "$first_load" in
    *0x0000000000001000*) ;;
    *)
        printf 'release-login: loader is not the supported --image-base=0x1000 shape\n' >&2
        printf 'release-login: first LOAD: %s\n' "$first_load" >&2
        exit 3
        ;;
esac

hash=$(sha256sum "$loader" | awk '{print $1}')
mkdir -p build/fixtures/login
printf '%s  %s\n' "$hash" "$loader" > build/fixtures/login/loader.sha256
printf 'release-login: loader base=0x1000 sha256=%s\n' "$hash"
