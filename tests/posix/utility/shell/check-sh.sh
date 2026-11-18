#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P1f11 utility-shell evidence: brush in POSIX mode must run the POSIX shell utility surface.
# This is host evidence for `util.sh`; rsh is the rescue shell and is not used.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

brush=${BRUSH_BIN:-"$root/userland/brush/target/debug/brush"}
if [ ! -x "$brush" ]; then
    cargo build --offline --locked --manifest-path userland/brush/Cargo.toml -p brush --bin brush
fi

tmp=$(mktemp -d "${TMPDIR:-/tmp}/p1c-utility-sh.XXXXXX")
trap 'rm -rf "$tmp"' 0 1 2 15

out=$("$brush" --posix -c '
set -eu
hello() { printf "%s\n" "$1"; }
hello world
x=$(printf abc)
[ "$x" = abc ] || exit 21
for i in 1 2 3; do printf "%s" "$i"; done
printf "\n"
case foo in f*) printf "case-ok\n";; *) exit 22;; esac
if [ 3 -gt 2 ]; then printf "cmp-ok\n"; else exit 23; fi
printf "PIPE\n" | while IFS= read -r line; do printf "%s\n" "$line"; done
false || printf "or-ok\n"
')
expected='world
123
case-ok
cmp-ok
PIPE
or-ok'
if [ "$out" != "$expected" ]; then
    printf 'FAIL: brush POSIX output does not match\nexpected:\n%s\ngot:\n%s\n' "$expected" "$out" >&2
    exit 1
fi

printf '#!/bin/sh\nprintf "script-ok\\n"\n' >"$tmp/script.sh"
out=$("$brush" --posix "$tmp/script.sh")
[ "$out" = "script-ok" ] || {
    printf 'FAIL: brush script-file invocation returned `%s`\n' "$out" >&2
    exit 1
}

set +e
"$brush" --posix -c 'exit 7'
status=$?
set -e
[ "$status" -eq 7 ] || {
    printf 'FAIL: brush exit status is %s, expected 7\n' "$status" >&2
    exit 1
}

printf 'p1c-utility-sh: brush --posix passed\n'
