#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K462 checked list: the raw pre-enum dispatch sites in kernel/src/syscall/mod.rs must match
# raw-dispatch.list exactly, every list number must still be at its definition file:line, every
# fuzz/child entry must have a RAW_<name> reference in src/main.rs, and every excluded entry must
# have no call site plus an audit entry. Run by build.sh (so the qtest pre-hook checks it) and by
# run.sh before the image build.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

list=tests/smp/syscall-fuzz/raw-dispatch.list
source_file=kernel/src/syscall/mod.rs
fixture=tests/smp/syscall-fuzz/src/main.rs
tmp=$(mktemp -d "${TMPDIR:-/tmp}/raw-check.XXXXXX")
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

fail() {
    echo "raw-check: FAIL $*" >&2
    exit 1
}

[ -f "$list" ] || fail "missing $list"
[ -f "$source_file" ] || fail "missing $source_file"

# 1. The pre-enum region: from dispatch_inner to the enum match.
awk '/^fn dispatch_inner/{on=1} on && /match Syscall::from_usize\(number\)/{exit} on{print}' \
    "$source_file" > "$tmp/region.rs"

# 2. Every `if number == <expr>` is one direct site; dtrace and the personality branch are
#    separate dispatch mechanisms and are checked explicitly below.
grep -o 'if number == [^ ]* {' "$tmp/region.rs" | sed 's/^if number == //; s/ {$//' \
    | LC_ALL=C sort > "$tmp/source-direct.txt"
awk -F'\t' '!/^#/ && NF>=6 && $5=="direct" {print $1}' "$list" | LC_ALL=C sort \
    > "$tmp/list-direct.txt"
diff -u "$tmp/source-direct.txt" "$tmp/list-direct.txt" > "$tmp/direct.diff" \
    || { cat "$tmp/direct.diff" >&2; fail "direct dispatch list differs from $source_file"; }

grep -q 'if dtrace::is_control(number)' "$tmp/region.rs" \
    || fail "dtrace::is_control is no longer before the enum"
grep -q 'dtrace::is_control' "$tmp/region.rs" || fail "dtrace dispatch missing"
grep -q 'linux::trap(&caller, frame)' "$tmp/region.rs" \
    || fail "linux::trap is no longer before the enum"
grep -q 'trap_x86_64' "$source_file" || fail "x86_64 Linux trap entry is missing"
awk -F'\t' '!/^#/ && NF>=6 && $5=="is_control" {n++} END {if (n != 6) exit 1}' "$list" \
    || fail "the dtrace list must have exactly six numbers"
awk -F'\t' '!/^#/ && NF>=6 && $5=="personality" {print $6}' "$list" \
    | grep -qx 'LINUX_TRAP' || fail "the Linux trap entry is missing"

# 3. Each number is still at the named definition line, and the fixture references it.
entries=0
fuzz=0
child=0
excluded=0
while IFS='	' read -r expression number definition disposition dispatch name reason; do
    case "$expression" in
        ''|'#'*) continue ;;
    esac
    [ -n "$name" ] || fail "row without a name: $expression"
    entries=$((entries + 1))
    case "$disposition" in
        fuzz) fuzz=$((fuzz + 1)) ;;
        child) child=$((child + 1)) ;;
        exclude) excluded=$((excluded + 1)) ;;
        *) fail "unknown disposition '$disposition' for $expression" ;;
    esac

    if [ "$dispatch" != "personality" ]; then
        file=${definition%:*}
        line=${definition##*:}
        [ -f "$file" ] || fail "missing definition file $file"
        actual=$(sed -n "${line}p" "$file")
        expected="pub const ${name}: usize = ${number};"
        printf '%s\n' "$actual" | tr 'A-F' 'a-f' | grep -q "$(printf '%s' "$expected" | tr 'A-F' 'a-f')" \
            || fail "$definition does not define $expected (found: $actual)"
    fi

    case "$dispatch" in
        direct) ident="RAW_${name}" ;;
        is_control) ident="RAW_DTRACE_${name}" ;;
        personality) ident="RAW_LINUX_TRAP" ;;
        *) fail "unknown dispatch '$dispatch' for $expression" ;;
    esac

    if [ "$disposition" = "exclude" ]; then
        [ -n "$reason" ] || fail "excluded $name has no reason"
        if grep -qE "(^|[^[:alnum:]_])${ident}([^[:alnum:]_]|$)" "$fixture"; then
            fail "excluded $name still has a $ident call reference"
        fi
        grep -q "\"${name}\"" "$fixture" \
            || fail "excluded $name is not in RAW_EXCLUDED"
    else
        grep -qE "(^|[^[:alnum:]_])${ident}([^[:alnum:]_]|$)" "$fixture" \
            || fail "$expression is listed but $ident is not referenced in $fixture"
    fi
done < "$list"

# 4. The reboot exclusion is proven by the handler body, not just a comment.
grep -q 'fn reboot' kernel/src/syscall/process_ctl.rs || fail "process_ctl::reboot is missing"
awk '/pub fn reboot/{on=1} on && /poweroff|reset|halt/{found=1} END{exit found?0:1}' \
    kernel/src/syscall/process_ctl.rs || fail "reboot handler no longer has a machine-ending path"

echo "raw-check: ok entries=$entries fuzz=$fuzz child=$child excluded=$excluded"
