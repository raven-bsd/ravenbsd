#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P42 lex behaviour: generate a word scanner, build it with the host C compiler and run it.

set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../../../../.." && pwd)
target=$root/build/posix-utility/parsers

if [ -z "${LEX:-}" ]; then
    (
        cd "$root"
        CARGO_TARGET_DIR=$target cargo build --quiet --locked -p lex
    )
    LEX=$target/debug/lex
fi
if [ ! -x "$LEX" ]; then
    printf 'lex test: missing host binary\n' >&2
    exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp "$here/words.l" "$here/expected.txt" "$tmp/"
cd "$tmp"

"$LEX" -t words.l > lex.yy.c
cc -Wall -Wextra -Werror -o words lex.yy.c

printf 'abc123 42 "x" /* c */ ifx 123abc\n' | ./words > actual 2> error
if ! cmp -s expected.txt actual; then
    printf 'lex test: output mismatch\n' >&2
    printf '%s\n' '--- expected ---' >&2
    cat expected.txt >&2
    printf '%s\n' '--- actual ---' >&2
    cat actual >&2
    exit 1
fi
if [ -s error ]; then
    printf 'lex test: unexpected stderr\n' >&2
    cat error >&2
    exit 1
fi
printf 'ok: lex identifier/number/comment/start-condition group\n'
