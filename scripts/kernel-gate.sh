#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# kernel-gate.sh — the reproducible QEMU kernel gate (S80a).
#
# Runs the fixed list of release-relevant qtest cases below, one at a time in the foreground, with
# `cargo xtask qtest --case <case> --arch <arch>`. Each run's exit status and, on failure, the
# failing expectation are written to docs/kernel-gate.md together with the current commit hash and
# a UTC timestamp. The script exits 0 only when every case passes; a timeout counts as a failure,
# never a pass. Raspberry Pi 4 hardware is out of scope: nothing here boots or claims a Pi 4.
#
# The case list is fixed on purpose. Extend it in a ticket that owns the new case; do not add
# ad-hoc cases at run time. Owners are the lanes that own the subsystem, so a failing row can be
# routed without re-reading the log.
#
# usage: sh scripts/kernel-gate.sh [--list]
#   --list   print the case table and exit (no QEMU run)
#
# The full serial/build log of each case is kept under target/kernel-gate/ (not committed).
# The exclusive-host preflight refuses other QEMU/xtask/cargo/rustc or load > 2.0. Set
# S107_WAIT_FOR_IDLE=<seconds> to wait for an idle host, or S107_MAX_LOAD to change the bound.
set -u

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root" || exit 2
. "$root/scripts/lib/mgmt.sh"

doc=
log_dir=target/kernel-gate
wait_idle=${S107_WAIT_FOR_IDLE:-0}
default_max_load() {
    n=$(nproc 2>/dev/null || echo 4)
    case "$n" in ''|*[!0-9]*) n=4 ;; esac
    m=$((n / 4))
    [ "$m" -lt 2 ] && m=2
    echo "$m"
}
max_load=${S107_MAX_LOAD:-$(default_max_load)}

# case | arch | owner | subsystem note
cases='boot-uname|x86_64|stuart|boot/uname
kernel-early-boot|x86_64|stuart|boot
exc-int3|x86_64|stuart|exceptions
exc-div0|x86_64|stuart|exceptions
exc-pagefault|x86_64|stuart|exceptions
exc-doublefault|x86_64|stuart|exceptions
kernel-paging|x86_64|stuart|paging
kernel-timer|x86_64|stuart|timer
time-entropy|x86_64|lance|time-entropy
time-entropy-cyclic|x86_64|lance|time-entropy
smp-user|x86_64|stuart|smp
smp-user-off|x86_64|stuart|smp
irq-dist|x86_64|stuart|irq
irq-dist-off|x86_64|stuart|irq
locking|x86_64|lance|locking
lockstat|x86_64|lance|locking
ipc-donation|x86_64|lance|ipc
kmem-frames|x86_64|mark|allocator
kmem-frames-nopcpu|x86_64|mark|allocator
proc-death-cleanup|x86_64|stuart|process
trace-context|x86_64|tim|trace
release-login|x86_64|kevin|release
boot-uname-aarch64|aarch64|dave|boot/uname
kernel-early-boot-aarch64|aarch64|dave|boot
kernel-exc-aarch64|aarch64|dave|exceptions
kernel-paging-aarch64|aarch64|dave|paging
kernel-timer-aarch64|aarch64|dave|timer'

if [ "${1:-}" = "--list" ]; then
    printf '%s\n' "$cases" | while IFS='|' read -r name arch owner note; do
        printf '%-28s %-8s %-7s %s\n' "$name" "$arch" "$owner" "$note"
    done
    exit 0
fi

if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
    sed -n '6,19p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
fi

doc=$(mgmt_path architecture/product-data/kernel-gate.md) || exit 2

mkdir -p "$log_dir" || exit 2
sh tests/smp/boot/host-idle.sh \
    --context "kernel-gate" \
    --wait "$wait_idle" \
    --max-load "$max_load" \
    --state-file "$log_dir/host-state.txt" || exit 2

commit=$(git rev-parse HEAD 2>/dev/null) || { echo "kernel-gate: not a git checkout" >&2; exit 2; }
short=$(git rev-parse --short=12 HEAD 2>/dev/null) || short=$commit
generated=$(date -u +%Y-%m-%dT%H:%M:%SZ)

cases_file=$log_dir/cases.txt
printf '%s\n' "$cases" > "$cases_file" || exit 2
results=$log_dir/results.txt
: > "$results" || exit 2

while IFS='|' read -r name arch owner note; do
    [ -n "$name" ] || continue
    log="$log_dir/$name.$arch.log"
    printf 'kernel-gate: %-28s %-8s ' "$name" "$arch"
    # stdin is /dev/null: neither cargo nor QEMU may consume the remaining case lines.
    if cargo xtask qtest --case "$name" --arch "$arch" </dev/null >"$log" 2>&1; then
        status=pass
        detail=ok
        printf 'pass\n'
    else
        exit_status=$(sed -n 's/.*exit status \([0-9][0-9]*\).*/\1/p' "$log" | tail -n 1)
        if grep -q 'timed out after' "$log"; then
            status='FAIL (timeout)'
            detail=$(grep -m1 'timed out after' "$log")
        else
            status=FAIL
            unmatched=$(awk '
                /^--- unmatched expectations ---$/ { mode = 1; next }
                /^--- /                            { mode = 0; next }
                mode && NF                         { print; exit }
            ' "$log")
            forbidden=$(awk '
                /^--- forbidden hits ---$/ { mode = 1; next }
                /^--- /                    { mode = 0; next }
                mode && NF                 { print; exit }
            ' "$log")
            detail=""
            [ -n "$exit_status" ] && detail="exit=$exit_status"
            [ -n "$unmatched" ] && detail="$detail unmatched=$unmatched"
            [ -n "$forbidden" ] && detail="$detail forbidden=$forbidden"
            if [ -z "$detail" ]; then
                detail=$(grep -v '^[[:space:]]*$' "$log" | tail -n 1)
            fi
        fi
        printf '%s\n' "$status"
    fi
    detail=$(printf '%s' "$detail" | tr '\n' ' ' | tr -s ' ' | sed 's/|/\\|/g' | cut -c1-180)
    printf '%s|%s|%s|%s|%s|%s\n' "$name" "$arch" "$owner" "$note" "$status" "$detail" >> "$results"
done < "$cases_file"

# The loop above runs in the main shell (input is a file, not a pipe through the loop body), so
# the counters are usable here.
total=$(grep -c . "$results" 2>/dev/null || true)
total=${total:-0}
passed=$(grep -c '|pass|' "$results" 2>/dev/null || true)
passed=${passed:-0}
failed=$((total - passed))
timed_out=$(grep -c 'FAIL (timeout)' "$results" 2>/dev/null || true)
timed_out=${timed_out:-0}

if [ "$failed" -eq 0 ]; then
    verdict=PASS
else
    verdict=FAIL
fi

{
    echo '<!-- SPDX-License-Identifier: BSD-3-Clause -->'
    echo '<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->'
    echo
    echo '# Kernel QEMU gate'
    echo
    echo "Generated by \`sh scripts/kernel-gate.sh\` at $generated from commit \`$short\`"
    echo "(\`$commit\`)."
    echo
    echo "Result: **$verdict** — $total cases, $passed passed, $failed failed ($timed_out timed out)."
    echo
    echo 'Scope: the release-relevant kernel cases below, x86_64 plus the aarch64 cases that exist'
    echo 'in `tests/qtest/cases/`. Raspberry Pi 4 hardware is **out of scope**; this gate makes no Pi 4'
    echo 'claim. Each row is one `cargo xtask qtest --case <case> --arch <arch>` run in the foreground.'
    echo 'A timeout is a failure. Full serial/build logs are in `target/kernel-gate/<case>.<arch>.log`'
    echo '(not committed).'
    echo
    echo '| Case | Arch | Owner | Subsystem | Result | Failure |'
    echo '|---|---|---|---|---|---|'
    while IFS='|' read -r name arch owner note status detail; do
        printf '| %s | %s | %s | %s | **%s** | %s |\n' \
            "$name" "$arch" "$owner" "$note" "$status" "$detail"
    done < "$results"
    echo
    echo '## Failures by owner'
    echo
    if [ "$failed" -eq 0 ]; then
        echo 'None.'
    else
        owners=$(awk -F'|' '$5 != "pass" { print $3 }' "$results" | sort -u)
        for owner in $owners; do
            echo "### $owner"
            echo
            awk -F'|' -v owner="$owner" \
                '$3 == owner && $5 != "pass" { printf "- `%s` (%s): %s — %s\n", $1, $2, $5, $6 }' \
                "$results"
            echo
        done
    fi
} > "$doc" || exit 2

if [ "$failed" -ne 0 ]; then
    echo "kernel-gate: FAIL ($failed of $total cases failed); table: $doc" >&2
    exit 1
fi
echo "kernel-gate: PASS ($total cases); table: $doc"
exit 0
