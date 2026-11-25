#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S85f host-side unit test for scripts/smp-aarch64.sh. The SMP reliability report must attribute
# every run to the host load around it, so the matrix TSV has to carry load1 before and after each
# run. The test runs a copy of the real harness against stub cargo/qemu/git binaries and asserts
# the exact TSV shape (case arch iter rc duration_ms load1_before load1_after) plus the summary
# parser's output, without building or booting anything.
#
# Run: timeout 30 sh tests/qtest/cases/smp-aarch64-harness_ut.sh

set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
HARNESS="$ROOT/scripts/smp-aarch64.sh"

fail() {
    echo "smp-aarch64-harness: FAIL $*" >&2
    exit 1
}

[ -f "$HARNESS" ] || fail "missing harness $HARNESS"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

mkdir -p "$tmp/scripts" "$tmp/bin" "$tmp/build/qtest"
cp "$HARNESS" "$tmp/scripts/smp-aarch64.sh"

# Stub cargo: record the duration report the harness reads back, then succeed.
cat > "$tmp/bin/cargo" <<'STUB'
#!/bin/sh
case_name=""
prev=""
for arg in "$@"; do
    if [ "$prev" = "--case" ]; then
        case_name=$arg
    fi
    prev=$arg
done
if [ -n "$case_name" ]; then
    mkdir -p build/qtest
    printf '{"duration_ms": 7}\n' > "build/qtest/$case_name.json"
fi
exit 0
STUB

# Stub qemu: report TCG only so the harness records KVM as unavailable, never probed.
cat > "$tmp/bin/qemu-system-aarch64" <<'STUB'
#!/bin/sh
if [ "$1" = "-accel" ] && [ "$2" = "help" ]; then
    echo "tcg"
    exit 0
fi
exit 1
STUB

cat > "$tmp/bin/git" <<'STUB'
#!/bin/sh
echo "deadbee"
STUB

chmod +x "$tmp/bin/cargo" "$tmp/bin/qemu-system-aarch64" "$tmp/bin/git"

PATH="$tmp/bin:$PATH" sh "$tmp/scripts/smp-aarch64.sh" \
    --iterations 1 --soak 1 --timebox 60 --max-load 0 > "$tmp/run.log" 2>&1 ||
    fail "harness exited nonzero: $(cat "$tmp/run.log")"

tsv="$tmp/build/qtest/smp-aarch64.tsv"
[ -s "$tsv" ] || fail "missing TSV $tsv"

lines=$(wc -l < "$tsv" | tr -d ' ')
[ "$lines" -eq 24 ] || fail "expected 24 runs (3 fast + 1 soak + 4 cases x 5 cross-arch), got $lines"

LC_ALL=C awk -F '	' '
    NF != 7 { printf "line %d has %d fields, expected 7: %s\n", NR, NF, $0; bad = 1 }
    $6 !~ /^[0-9]+(\.[0-9]+)?$/ { printf "line %d load1_before is not numeric: %s\n", NR, $6; bad = 1 }
    $7 !~ /^[0-9]+(\.[0-9]+)?$/ { printf "line %d load1_after is not numeric: %s\n", NR, $7; bad = 1 }
    END { exit bad }
' "$tsv" || fail "TSV does not carry case/arch/iter/rc/duration/load1_before/load1_after"

summary="$tmp/build/qtest/smp-aarch64.summary.tsv"
[ -s "$summary" ] || fail "missing summary $summary"
header=$(head -1 "$summary")
[ "$header" = "case	arch	runs	fail	min_ms	p50_ms	p90_ms	max_ms" ] ||
    fail "summary header changed: $header"
rows=$(($(wc -l < "$summary" | tr -d ' ') - 1))
[ "$rows" -eq 8 ] || fail "expected 8 summary rows, got $rows"

grep -q 'smp-aarch64: fast_iterations=1 soak_iterations=1 cross_arch_iterations=5 timed_out=0 failed=0' "$tmp/run.log" ||
    fail "summary counters changed: $(tail -3 "$tmp/run.log")"

echo "smp-aarch64-harness: ok runs=$lines summary_rows=$rows"
