#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Reproducible mandoc check for libc gate 6.2.
#
# The lint list mirrors the `cases` of the 6.2 row in tests/libc/gate/rows.toml; the gate runs
# `mandoc -Tlint -Wwarning` on those pages. The script additionally renders every shipped page
# (`docs/man[1-9]/*.[1-9]`) with `-Tutf8`, so a parse/render failure is visible even when the
# lint level is quiet.
#
# Usage: sh scripts/check-mandoc.sh
#   MANDOC=/path/to/mandoc  use an explicit tool instead of PATH.
#
# Exit: 0 clean, 1 lint/render failure, 2 mandoc is not installed.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

mandoc=${MANDOC:-}
if [ -z "$mandoc" ]; then
    mandoc=$(command -v mandoc || true)
fi
if [ -z "$mandoc" ] || [ ! -x "$mandoc" ]; then
    echo "check-mandoc: mandoc not found" >&2
    echo "  install it with: sudo apt-get install -y mandoc  (Debian/Ubuntu package: mandoc)" >&2
    exit 2
fi

set -- \
    docs/man3/setlocale.3 \
    docs/man3/printf.3 \
    docs/man3/iconv.3 \
    docs/man3/assert.3 \
    docs/man3/fork.3 \
    docs/man3/posix_spawn.3 \
    docs/man3/issetugid.3 \
    docs/man3/arc4random.3 \
    docs/man3/malloc.3 \
    docs/man3/clock.3 \
    docs/man3/mmap.3 \
    docs/man3/open.3 \
    docs/man3/pledge.3 \
    docs/man3/pthread_create.3 \
    docs/man3/dlopen.3 \
    docs/man3/sysconf.3 \
    docs/man7/libc.7

version=$(dpkg-query -W -f='${Version}' mandoc 2>/dev/null || echo unknown)
echo "check-mandoc: mandoc ${version} (${mandoc})"
echo "check-mandoc: linting $# page(s) at -Wwarning"

if ! "$mandoc" -Tlint -Wwarning "$@"; then
    echo "check-mandoc: lint FAILED" >&2
    exit 1
fi

for page in "$@"; do
    if ! "$mandoc" -Tutf8 "$page" >/dev/null; then
        echo "check-mandoc: render FAILED: $page" >&2
        exit 1
    fi
done

rendered=$(find docs/man1 docs/man2 docs/man3 docs/man7 docs/man8 -type f 2>/dev/null | sort)
if [ -z "$rendered" ]; then
    echo "check-mandoc: no shipped man pages found" >&2
    exit 1
fi
render_count=$(printf '%s\n' "$rendered" | wc -l)
for page in $rendered; do
    if ! "$mandoc" -Tutf8 "$page" >/dev/null; then
        echo "check-mandoc: render FAILED: $page" >&2
        exit 1
    fi
done

echo "check-mandoc: ok ($# gate pages linted, $render_count rendered)"
