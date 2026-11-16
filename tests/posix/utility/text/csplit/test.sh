#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup csplit

printf 'a\nb\nc\nd\n' >"$TMPDIR/in"
mkdir "$TMPDIR/default" "$TMPDIR/prefix" "$TMPDIR/keep" "$TMPDIR/error"

(cd "$TMPDIR/default" && "$UTIL" "$TMPDIR/in" 3 >"$TMPDIR/default.out")
printf 'a\nb\n' >"$TMPDIR/want1"
printf 'c\nd\n' >"$TMPDIR/want2"
text_file_eq "$TMPDIR/want1" "$TMPDIR/default/xx00" "line-number split first file"
text_file_eq "$TMPDIR/want2" "$TMPDIR/default/xx01" "line-number split second file"
text_eq "$(printf '4\n4')" "$(cat "$TMPDIR/default.out")" "byte counts on stdout"

(cd "$TMPDIR/prefix" && "$UTIL" -f part "$TMPDIR/in" 2 >/dev/null)
printf 'a\n' >"$TMPDIR/want3"
printf 'b\nc\nd\n' >"$TMPDIR/want4"
text_file_eq "$TMPDIR/want3" "$TMPDIR/prefix/part00" "-f prefix first file"
text_file_eq "$TMPDIR/want4" "$TMPDIR/prefix/part01" "-f prefix second file"

set +e
(cd "$TMPDIR/keep" && "$UTIL" -k "$TMPDIR/in" 99 >/dev/null 2>&1)
keep_status=$?
set -e
[ "$keep_status" -ne 0 ] || text_fail "-k out-of-range must fail"
text_ok "-k out-of-range fails"
printf 'a\nb\nc\nd\n' >"$TMPDIR/keep/want"
text_file_eq "$TMPDIR/keep/want" "$TMPDIR/keep/xx00" "-k keeps the partial file"

set +e
(cd "$TMPDIR/error" && "$UTIL" "$TMPDIR/in" 99 >/dev/null 2>&1)
error_status=$?
set -e
[ "$error_status" -ne 0 ] || text_fail "out-of-range must fail"
text_ok "out-of-range fails"
[ ! -e "$TMPDIR/error/xx00" ] || text_fail "failed split left output files"
text_ok "failed split removes output files"
