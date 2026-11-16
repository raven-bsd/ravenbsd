#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup printf

text_eq "hello" "$("$UTIL" '%s\n' hello)" "%s conversion"
text_eq "42 0007 ff 10" "$("$UTIL" '%d %04d %x %o\n' 42 7 255 8)" "integer conversions"
text_eq "$(printf 'a\tb\nc\\d')" "$("$UTIL" 'a\tb\nc\\d\n')" "backslash escapes"
text_eq "$(printf 'a\tb')" "$("$UTIL" '%b' 'a\tb\n')" "%b escape argument"
text_eq "$(printf 'a b\nc d')" "$("$UTIL" '%s %s\n' a b c d)" "format reuse"
text_eq "|0|" "$("$UTIL" '%s|%d|%s\n')" "missing arguments default"
text_eq "AAB" "$("$UTIL" 'A\101B\n')" "octal escape"
printf 'a\000b' >"$TMPDIR/want-nul"
"$UTIL" 'a\000b' >"$TMPDIR/out-nul"
text_file_eq "$TMPDIR/want-nul" "$TMPDIR/out-nul" "NUL octal escape"
text_status nonzero "invalid numeric argument" "$UTIL" '%d\n' abc
text_status nonzero "missing format" "$UTIL"
