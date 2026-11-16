#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1 header evidence: compile assert.h and exercise the assert macro with the POSIX.1-2024
# feature macros. This is the command named by docs/posix/evidence/P1.toml.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cc=${CC:-cc}

if ! command -v "$cc" >/dev/null 2>&1; then
    echo "posix headers: $cc not found" >&2
    exit 1
fi

tmp=$(mktemp -d "${TMPDIR:-/tmp}/posix-headers.XXXXXX")
trap 'rm -rf "$tmp"' EXIT

printf '%s\n' \
    '#include <assert.h>' \
    'int main(void) { assert(1); return 0; }' >"$tmp/assert.c"

"$cc" -fsyntax-only -std=c17 -D_POSIX_C_SOURCE=202405L -D_XOPEN_SOURCE=800 \
    -I "$root/libc/ravenlibc/include" \
    -I "$root/libc/ravenlibc/arch/x86_64/include" \
    "$tmp/assert.c"

echo "posix headers: PASS (assert.h)"
