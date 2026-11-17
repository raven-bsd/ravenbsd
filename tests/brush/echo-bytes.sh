#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K26g byte-exact host check for the brush echo builtin.
#
# The shell pre-sizes the echo output buffer so the first allocation never goes through the
# zero-capacity growth path (which reuses a freed Tiny block and drops the first eight bytes).
# This script runs the host brush binary and compares the full output byte string, including the
# first eight bytes and an embedded NUL produced by `echo -e`.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
bin=${BRUSH_BIN:-$root/userland/brush/target/debug/brush}

if [ ! -x "$bin" ]; then
    (
        cd "$root/userland/brush"
        cargo build -p brush
    ) >/dev/null
fi
if [ ! -x "$bin" ]; then
    printf 'echo-bytes: no brush binary at %s\n' "$bin" >&2
    exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# A short line exercises the first eight bytes directly.
"$bin" -c 'echo 0123456789' > "$tmp/short"
printf '0123456789\n' > "$tmp/short.exp"
cmp "$tmp/short.exp" "$tmp/short" || {
    printf 'echo-bytes: short line mismatch\n' >&2
    od -An -tx1 "$tmp/short"
    exit 1
}
printf 'ok: short line (first eight bytes)\n'

# A longer line confirms the whole buffer is intact past the Tiny-block boundary.
text='abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789'
"$bin" -c "echo $text" > "$tmp/long"
printf '%s\n' "$text" > "$tmp/long.exp"
cmp "$tmp/long.exp" "$tmp/long" || {
    printf 'echo-bytes: long line mismatch\n' >&2
    exit 1
}
printf 'ok: long line\n'

# An embedded NUL from the octal escape must survive end to end.
"$bin" -c "echo -e 'a\\0b'" > "$tmp/nul"
printf 'a\0b\n' > "$tmp/nul.exp"
cmp "$tmp/nul.exp" "$tmp/nul" || {
    printf 'echo-bytes: NUL line mismatch\n' >&2
    od -An -tx1 "$tmp/nul"
    exit 1
}
printf 'ok: embedded NUL\n'
