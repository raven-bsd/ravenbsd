#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# crontab host cases: install/list/remove, validation and atomic replacement.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo build -q -p crontab
crontab_bin="$root/target/debug/crontab"
[ -x "$crontab_bin" ] || {
    echo "crontab test: missing $crontab_bin" >&2
    exit 1
}

work="$root/build/posix/utility/crontab"
rm -rf "$work"
mkdir -p "$work"
trap 'rm -rf "$work"' EXIT

export CRON_SPOOL="$work/spool"
export USER=root

fail() {
    echo "crontab test: FAIL $*" >&2
    exit 1
}

spool="$CRON_SPOOL/root"

cat >"$work/valid" <<'EOF'
# a comment
*/5 1-3 1,15 * 1-5 echo hello
0 0 * * 0 echo sunday
EOF

"$crontab_bin" "$work/valid" || fail "install"
[ -f "$spool" ] || fail "spool missing"
out=$("$crontab_bin" -l)
printf '%s\n' "$out" | grep -q 'echo hello' || fail "list hello"
printf '%s\n' "$out" | grep -q 'echo sunday' || fail "list sunday"

cat >"$work/bad" <<'EOF'
60 * * * * echo bad
EOF
if "$crontab_bin" "$work/bad"; then
    fail "invalid minute accepted"
fi
out=$("$crontab_bin" -l)
printf '%s\n' "$out" | grep -q 'echo hello' || fail "invalid install replaced spool"

cat >"$work/reboot" <<'EOF'
@reboot echo unsupported
EOF
if "$crontab_bin" "$work/reboot"; then
    fail "@reboot accepted"
fi

"$crontab_bin" -r || fail "remove"
if "$crontab_bin" -l >/dev/null 2>&1; then
    fail "list after remove succeeded"
fi

cat >"$work/replace" <<'EOF'
10 * * * * echo replaced
EOF
"$crontab_bin" "$work/replace" || fail "install replacement"
out=$("$crontab_bin" -l)
[ "$out" = "10 * * * * echo replaced" ] || fail "replacement: $out"
echo "crontab: ok"
