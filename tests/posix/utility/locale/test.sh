#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# iconv, gencat and locale cases on fixed inputs.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo build -q -p iconv -p gencat -p locale
iconv_bin="$root/target/debug/iconv"
gencat_bin="$root/target/debug/gencat"
locale_bin="$root/target/debug/locale"
for binary in "$iconv_bin" "$gencat_bin" "$locale_bin"; do
    [ -x "$binary" ] || {
        echo "locale test: missing $binary" >&2
        exit 1
    }
done

work="$root/build/posix/utility/locale"
rm -rf "$work"
mkdir -p "$work"
trap 'rm -rf "$work"' EXIT

fixture="$root/tests/posix/utility/locale"

fail() {
    echo "locale test: FAIL $*" >&2
    exit 1
}

printf 'caf\303\251\n' >"$work/utf8"
printf 'caf\351\n' >"$work/latin1"
printf 'plain\n' >"$work/ascii"

"$iconv_bin" -f UTF-8 -t ISO-8859-1 "$work/utf8" >"$work/latin1-out" || fail "iconv to latin1"
cmp -s "$work/latin1-out" "$work/latin1" || fail "iconv latin1 bytes"
"$iconv_bin" -f ISO-8859-1 -t UTF-8 "$work/latin1" >"$work/utf8-out" || fail "iconv to utf8"
cmp -s "$work/utf8-out" "$work/utf8" || fail "iconv utf8 bytes"
"$iconv_bin" -f ASCII -t UTF-8 "$work/ascii" >"$work/ascii-out" || fail "iconv ascii"
cmp -s "$work/ascii-out" "$work/ascii" || fail "iconv ascii bytes"
"$iconv_bin" -f UTF-8 -t ISO-8859-1 -o "$work/latin1-o" "$work/utf8" || fail "iconv -o"
cmp -s "$work/latin1-o" "$work/latin1" || fail "iconv -o bytes"
"$iconv_bin" -f ISO-8859-1 -t UTF-8 <"$work/latin1" >"$work/stdin-out" || fail "iconv stdin"
cmp -s "$work/stdin-out" "$work/utf8" || fail "iconv stdin bytes"

if "$iconv_bin" -f UTF-8 -t ASCII "$work/utf8" >"$work/ascii-fail" 2>/dev/null; then
    fail "iconv converted non-ASCII to ASCII"
fi
[ ! -s "$work/ascii-fail" ] || fail "iconv wrote partial output"
if printf '\377' | "$iconv_bin" -f UTF-8 -t ASCII >/dev/null 2>&1; then
    fail "iconv accepted invalid UTF-8"
fi
if "$iconv_bin" -f UTF-16 -t UTF-8 "$work/utf8" >/dev/null 2>&1; then
    fail "iconv accepted UTF-16"
fi
if "$iconv_bin" -f UTF-8 "$work/utf8" >/dev/null 2>&1; then
    fail "iconv accepted a missing -t"
fi

"$gencat_bin" "$work/messages.cat" "$fixture/messages.msg" || fail "gencat"
[ -s "$work/messages.cat" ] || fail "empty catalog"
magic=$(od -An -tx1 -N4 "$work/messages.cat" | tr -d ' \n')
[ "$magic" = "ff88ff89" ] || fail "catalog magic: $magic"
"$gencat_bin" - "$fixture/messages.msg" >"$work/messages2.cat" || fail "gencat stdout"
cmp -s "$work/messages.cat" "$work/messages2.cat" || fail "gencat stdout differs"
printf '$set 1\n1 "unterminated\n' >"$work/bad.msg"
if "$gencat_bin" "$work/bad.cat" "$work/bad.msg" >/dev/null 2>&1; then
    fail "gencat accepted a bad source"
fi
if "$gencat_bin" >/dev/null 2>&1; then
    fail "gencat accepted no arguments"
fi

env -i LANG=C PATH="$PATH" "$locale_bin" >"$work/locale-c" || fail "locale C"
grep -qx 'LC_CTYPE="C"' "$work/locale-c" || fail "locale C LC_CTYPE"
grep -qx 'codeset=ANSI_X3.4-1968' "$work/locale-c" || fail "locale C codeset"
env -i LANG=C.UTF-8 PATH="$PATH" "$locale_bin" >"$work/locale-utf8" || fail "locale UTF-8"
grep -qx 'LC_CTYPE="C.UTF-8"' "$work/locale-utf8" || fail "locale UTF-8 LC_CTYPE"
grep -qx 'codeset=UTF-8' "$work/locale-utf8" || fail "locale UTF-8 codeset"

"$locale_bin" -a >"$work/locales" || fail "locale -a"
grep -qx 'C' "$work/locales" || fail "locale -a C"
grep -qx 'POSIX' "$work/locales" || fail "locale -a POSIX"
grep -qx 'C.UTF-8' "$work/locales" || fail "locale -a C.UTF-8"
"$locale_bin" -m >"$work/charmaps" || fail "locale -m"
grep -qx 'UTF-8' "$work/charmaps" || fail "locale -m UTF-8"
grep -qx 'ANSI_X3.4-1968' "$work/charmaps" || fail "locale -m ASCII"
"$locale_bin" -k >"$work/keywords" || fail "locale -k"
grep -qx 'LC_MESSAGES' "$work/keywords" || fail "locale -k LC_MESSAGES"
grep -qx 'codeset' "$work/keywords" || fail "locale -k codeset"

env -i LANG=C PATH="$PATH" "$locale_bin" LC_CTYPE >"$work/query" || fail "locale query"
grep -qx 'LC_CTYPE="C"' "$work/query" || fail "locale query value"
env -i LANG=C PATH="$PATH" "$locale_bin" LC_ALL >"$work/query-all" || fail "locale LC_ALL"
grep -qx 'LC_ALL="C"' "$work/query-all" || fail "locale LC_ALL value"
if env -i LANG=C PATH="$PATH" "$locale_bin" LC_UNKNOWN >/dev/null 2>&1; then
    fail "locale accepted an unknown category"
fi
if "$locale_bin" -a LC_CTYPE >/dev/null 2>&1; then
    fail "locale accepted -a with a category"
fi

echo "locale: ok"
