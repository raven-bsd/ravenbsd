#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K549 test: the served-copy refresher detects drift, refreshes only the
# management copies, and records hashes that match sha256sum.
#
# The test uses a throw-away product/mgmt pair. It never reads or writes the
# real architecture/product-data tree.

set -eu

repo=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
script="$repo/scripts/served-copies.sh"
base=$(mktemp -d /tmp/opencode/k549-served.XXXXXX)
trap 'rm -rf "$base"' EXIT

product="$base/product"
mgmt="$base/mgmt"
data="$mgmt/architecture/product-data"
manifest="$data/SERVED.tsv"
head=deadbeefdeadbeefdeadbeefdeadbeefdeadbeef

fail() {
    printf 'served-copies test: FAIL: %s\n' "$*" >&2
    exit 1
}

run_check() {
    set +e
    out=$(env SERVED_COPIES_HEAD="$head" sh "$script" --check --root "$product" --mgmt "$mgmt" 2>&1)
    status=$?
    set -e
}

run_refresh() {
    set +e
    out=$(env SERVED_COPIES_HEAD="$head" sh "$script" --refresh --root "$product" --mgmt "$mgmt" 2>&1)
    status=$?
    set -e
}

check_hashes() {
    awk -F'\t' '!/^#/ && NF { print $1 "\t" $5 }' "$manifest" > "$base/hashes"
    while IFS="$(printf '\t')" read -r served hash; do
        [ -f "$mgmt/$served" ] || fail "missing served file $served"
        actual=$(sha256sum "$mgmt/$served" | awk '{ print $1 }')
        [ "$actual" = "$hash" ] || fail "manifest hash mismatch for $served: $hash != $actual"
    done < "$base/hashes"
}

mkdir -p "$product/tests/release/port-register" "$product/tests/abi" "$data"

cat > "$product/tests/release/port-register/register.tsv" <<'EOF'
file	area	status	basis	evidence	owner	proposed_ticket	source	upstream	license
crates/a.rs	crates	COVERED	sibling	crates/a_ut.rs	bob	-	provenance+header	bsd	BSD-2-Clause
libc/b.h	libc	INDIRECT	crate	tests/libc/b_ut.rs	kevin	-	provenance+header	musl	MIT
EOF

cat > "$product/tests/abi/abi-v1.txt" <<'EOF'
SYSCALL 0x01 EXIT argc=1 ret=!
SYSCALL 0x02 YIELD argc=0 ret=0
EOF

cat > "$mgmt/architecture/release-path.tsv" <<'EOF'
# area	step	capability	state	detail	owner
ssh	1. TCP reachable	TCP reachable	done	evidence	kevin
release	2. soak	24 h soak	blocked	owner	owner
EOF

cat > "$data/kernel-gate.md" <<'EOF'
<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Kernel QEMU gate

Status: **PASS** at `deadbeefdeadbeefdeadbeefdeadbeefdeadbeef`.

Result: **PASS** — 1 case, 1 passed, 0 failed.
EOF

cat > "$data/posix-conformance.md" <<'EOF'
<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# POSIX.1-2024 self-certification

- Commit: `cafebabecafebabecafebabecafebabecafebabe`
- Result: **PASS**
- Counts: 1 pass, 0 fail, 0 skip
EOF

cat > "$data/syscall-abi.md" <<'EOF'
<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Native syscall ABI (v1 freeze)

## Syscalls

| Number | Name | Args | Return | Label |
|---|---|---|---|---|
| `0x01` | `EXIT` | 1 | `!` | Committed |
| `0x02` | `YIELD` | 0 | `0` | Committed |

## Wire structs

| Type | Size | Align | Label |
|---|---|---|---|
| `Handle` | 4 | — | Committed |
EOF

[ -f "$script" ] || fail "scripts/served-copies.sh is missing"

run_check
[ "$status" -eq 1 ] || fail "first check must fail (got $status): $out"
printf '%s\n' "$out" | grep -q 'port-register.md' || fail "first check did not name port-register.md: $out"
printf '%s\n' "$out" | grep -q 'ssh-gate.tsv' || fail "first check did not name ssh-gate.tsv: $out"
printf '%s\n' "$out" | grep -q 'SERVED.tsv' || fail "first check did not name SERVED.tsv: $out"

run_refresh
[ "$status" -eq 0 ] || fail "refresh failed (got $status): $out"
run_check
[ "$status" -eq 0 ] || fail "check after refresh failed (got $status): $out"
[ -f "$manifest" ] || fail "refresh did not write SERVED.tsv"
rows=$(awk '!/^#/ && NF { n++ } END { print n + 0 }' "$manifest")
[ "$rows" -eq 5 ] || fail "expected 5 manifest rows, got $rows"
check_hashes
if printf '%s\n' "$out" | grep -q 'port-register.md'; then
    fail "current check still names port-register.md: $out"
fi
printf '%s\n' "$out" | grep -q 'WARN.*posix-conformance.md' || fail "current check lost the stale POSIX warning: $out"

before=$(sha256sum "$data/port-register.md" | awk '{ print $1 }')
cp "$product/tests/release/port-register/register.tsv" "$base/register.original"
printf 'crates/new.rs\tcrates\tCOVERED\tsibling\tcrates/new_ut.rs\tbob\t-\tprovenance+header\tbsd\tBSD-2-Clause\n' \
    >> "$product/tests/release/port-register/register.tsv"
run_check
[ "$status" -eq 1 ] || fail "check after a one-line source change must fail (got $status): $out"
printf '%s\n' "$out" | grep -q 'port-register.md' || fail "source drift did not name port-register.md: $out"
run_refresh
[ "$status" -eq 0 ] || fail "refresh after source drift failed (got $status): $out"
run_check
[ "$status" -eq 0 ] || fail "check after refreshing source drift failed (got $status): $out"
after=$(sha256sum "$data/port-register.md" | awk '{ print $1 }')
[ "$before" != "$after" ] || fail "the one-line source change did not change the served hash"
check_hashes

cp "$base/register.original" "$product/tests/release/port-register/register.tsv"
run_refresh
[ "$status" -eq 0 ] || fail "refresh after restoring the source failed (got $status): $out"
run_check
[ "$status" -eq 0 ] || fail "check after restoring the source failed (got $status): $out"
restored=$(sha256sum "$data/port-register.md" | awk '{ print $1 }')
[ "$restored" = "$before" ] || fail "restored served hash differs from the original: $before != $restored"
check_hashes

printf 'tampered\n' >> "$data/syscall-abi.md"
run_check
[ "$status" -eq 1 ] || fail "check after a served-copy edit must fail (got $status): $out"
printf '%s\n' "$out" | grep -q 'syscall-abi.md' || fail "served edit did not name syscall-abi.md: $out"
run_refresh
[ "$status" -eq 0 ] || fail "refresh after a served-copy edit failed (got $status): $out"
run_check
[ "$status" -eq 0 ] || fail "check after repairing the served copy failed (got $status): $out"
check_hashes

printf 'served-copies test: PASS\n'
