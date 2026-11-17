#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup tee

printf 'hello\n' >"$TMPDIR/in"

text_eq "hello" "$("$UTIL" "$TMPDIR/a" "$TMPDIR/b" <"$TMPDIR/in")" "stdout copy"
text_eq "hello" "$(cat "$TMPDIR/a")" "first file"
text_eq "hello" "$(cat "$TMPDIR/b")" "second file"

printf 'more\n' | "$UTIL" -a "$TMPDIR/a" >/dev/null
text_eq "$(printf 'hello\nmore')" "$(cat "$TMPDIR/a")" "-a append"

text_eq "stdin" "$(printf 'stdin\n' | "$UTIL")" "no file operand"

printf 'a\000b\n' >"$TMPDIR/nul"
printf 'a\000b\n' >"$TMPDIR/want-nul"
"$UTIL" "$TMPDIR/nulout" <"$TMPDIR/nul" >"$TMPDIR/nulstdout"
text_file_eq "$TMPDIR/want-nul" "$TMPDIR/nulout" "NUL byte copy to file"
text_file_eq "$TMPDIR/want-nul" "$TMPDIR/nulstdout" "NUL byte copy to stdout"
text_status nonzero "unwritable file" sh -c 'printf x | "$1" /nonexistent/out' sh "$UTIL"
