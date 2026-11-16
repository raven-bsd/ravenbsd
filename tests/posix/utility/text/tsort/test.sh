#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
. "$(dirname "$0")/../lib-text.sh"
text_setup tsort

printf 'a b\nb c\n' >"$TMPDIR/chain"
printf 'a b\nb c\na c\n' >"$TMPDIR/graph"

text_eq "$(printf 'a\nb\nc')" "$("$UTIL" "$TMPDIR/chain")" "chain order"
text_eq "$(printf 'a\nb\nc')" "$("$UTIL" "$TMPDIR/graph")" "graph dependencies"
text_eq "$(printf 'a\nb\nc')" "$("$UTIL" <"$TMPDIR/chain")" "standard input"
text_status nonzero "cycle" sh -c 'printf "a b\nb a\n" | "$1"' sh "$UTIL"
text_status nonzero "odd token count" sh -c 'printf "a\n" | "$1"' sh "$UTIL"
text_status nonzero "missing file" "$UTIL" "$TMPDIR/missing"
