#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build the same K247 post-auth fork fixture with the host C compiler and glibc. This is the
# Linux baseline for the Raven qtest; it does not touch the Raven static build.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

out=build/fixtures/post-auth-fork/host
mkdir -p "$out"

CC=${CC:-cc}
command -v "$CC" >/dev/null 2>&1 || {
    echo "post-auth-fork host: $CC not found" >&2
    exit 77
}

"$CC" -std=gnu11 -O1 -g -Wall -Wextra -Werror \
    -DPOST_AUTH_FORK_HOST=1 \
    "$root/tests/fixtures/post-auth-fork/main.c" \
    -o "$out/post-auth-fork"

echo "post-auth-fork host: built $out/post-auth-fork"
