#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S102 IRQ-lock audit regression.
#
# 1. The real tree must pass the inventory/document check.
# 2. A temporary fake tree proves that a new lock declaration, a new call
#    site, and a stale inventory row are each rejected.  The fake tree is
#    never inside kernel/src; the product tree is not modified.

set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
CHECK="$ROOT/tests/smp/irq-lock-audit/check.py"

echo "== real tree =="
python3 "$CHECK" --check

TMP=$(mktemp -d "${TMPDIR:-/tmp}/s102-irq-lock-audit.XXXXXX")
trap 'rm -rf "$TMP"' EXIT HUP INT TERM

mkdir -p "$TMP/kernel/src" "$TMP/docs" "$TMP/tests/smp/irq-lock-audit"
cp "$CHECK" "$TMP/tests/smp/irq-lock-audit/check.py"

cat > "$TMP/kernel/src/lib.rs" <<'RS'
static FIXTURE: IrqSpinLock<()> = IrqSpinLock::new(());
fn take() {
    let _ = FIXTURE.lock();
}
RS
cat > "$TMP/docs/irq-lock-audit.md" <<'MD'
# fixture audit
- `kernel/src/lib.rs:1` `FIXTURE`
MD

python3 "$TMP/tests/smp/irq-lock-audit/check.py" --write >/dev/null
python3 "$TMP/tests/smp/irq-lock-audit/check.py" --check >/dev/null
echo "fixture baseline: PASS"

cat >> "$TMP/kernel/src/lib.rs" <<'RS'
static AUDIT_PROBE: IrqSpinLock<()> = IrqSpinLock::new(());
fn probe() {
    let _ = AUDIT_PROBE.lock();
}
RS

if python3 "$TMP/tests/smp/irq-lock-audit/check.py" --check >"$TMP/neg-decl.log" 2>&1; then
    echo "fixture negative (new declaration): FAIL (accepted)" >&2
    exit 1
fi
grep -q "undocumented lock declaration: kernel/src/lib.rs:5" "$TMP/neg-decl.log"
echo "fixture negative (new declaration): PASS"

cat >> "$TMP/docs/irq-lock-audit.md" <<'MD'
- `kernel/src/lib.rs:5` `AUDIT_PROBE`
MD

if python3 "$TMP/tests/smp/irq-lock-audit/check.py" --check >"$TMP/neg-site.log" 2>&1; then
    echo "fixture negative (new site): FAIL (accepted)" >&2
    exit 1
fi
grep -q "new/changed lock site not in inventory" "$TMP/neg-site.log"
echo "fixture negative (new site): PASS"

# Re-record the probe site, then remove its call: the checker must report the
# stale inventory row.  Line 7 is the `AUDIT_PROBE.lock()` line in the fixture.
python3 "$TMP/tests/smp/irq-lock-audit/check.py" --write >/dev/null
sed -i '7d' "$TMP/kernel/src/lib.rs"
if python3 "$TMP/tests/smp/irq-lock-audit/check.py" --check >"$TMP/neg-stale.log" 2>&1; then
    echo "fixture negative (stale row): FAIL (accepted)" >&2
    exit 1
fi
grep -q "stale inventory row" "$TMP/neg-stale.log"
echo "fixture negative (stale row): PASS"

echo "irq-lock-audit: PASS"
