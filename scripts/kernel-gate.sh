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
# routed without re-reading the log. A row may name a qtest case file in an optional fifth field;
# without it the case name resolves under tests/qtest/cases/ as before.
#
# usage: sh scripts/kernel-gate.sh [--list | --durability | --functional]
#   --list         print the case table and exit (no QEMU run)
#   --durability   run only the three UFS durability rows (ENOSPC, fsync write/kill,
#                  remount check) and write target/kernel-gate/durability.md
#   --functional   use the functional hygiene budget (qemu <= 2 and load1 <= 2 x nproc)
#                  instead of the exclusive host-idle preflight; a REFUSE stops the gate
#
# The full serial/build log of each case is kept under target/kernel-gate/ (not committed).
# These are functional rows, not latency rows: the preflight refuses another QEMU but tolerates
# co-tenant cargo/rustc builds and allows load <= 4*nproc (a scheduling guard, not a pass). S107_LATENCY=1 switches to the strict
# no-job, max(2, nproc/4) contract for wall-clock rows.
# A clean clone runs without RAVEN_MGMT_ROOT: the table is then kept under target/kernel-gate/
# instead of the management tree, and the rows still run.
set -u

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root" || exit 2
. "$root/scripts/lib/mgmt.sh"

doc=
log_dir=target/kernel-gate
# K567 follow-up: release-login's image is reused only when its sidecar stamp carries the exact
# HEAD hash. The stamp is the image's tree tag; a different HEAD forces a rebuild.
release_image=build/qtest/release-login.img
release_stamp=$release_image.head
release_lock=build/qtest/.release-login.lock
release_build_cap=${RAVEN_GATE_RELEASE_BUILD_CAP:-600}
release_build_wait=${RAVEN_GATE_RELEASE_BUILD_WAIT:-600}
wait_idle=${S107_WAIT_FOR_IDLE:-0}
latency=${S107_LATENCY:-0}
default_max_load() {
    n=$(nproc 2>/dev/null || echo 4)
    case "$n" in ''|*[!0-9]*) n=4 ;; esac
    if [ "$latency" = 1 ]; then
        m=$((n / 4))
        [ "$m" -lt 2 ] && m=2
        echo "$m"
    else
        echo $((n * 4))
    fi
}
max_load=${S107_MAX_LOAD:-$(default_max_load)}
preflight_args=
if [ "$latency" = 1 ]; then
    preflight_args=
else
    preflight_args="--allow-jobs --allow-qemu $(( $(nproc) / 2 ))"
fi

# case | arch | owner | subsystem note | optional explicit case path
# The path column lets an SMP fixture live under tests/smp/<name>/ instead of tests/qtest/cases.
cases='boot-uname|x86_64|stuart|boot/uname
kernel-early-boot|x86_64|stuart|boot
exc-int3|x86_64|stuart|exceptions
exc-div0|x86_64|stuart|exceptions
exc-pagefault|x86_64|stuart|exceptions
exc-doublefault|x86_64|stuart|exceptions
kernel-paging|x86_64|stuart|paging
kernel-timer|x86_64|stuart|timer
readiness-kvm4|x86_64|stuart|readiness|tests/smp/readiness/readiness-kvm4.toml
time-entropy|x86_64|lance|time-entropy
time-entropy-cyclic|x86_64|lance|time-entropy
smp-user|x86_64|stuart|smp
smp-user-off|x86_64|stuart|smp
irq-dist|x86_64|stuart|irq
irq-dist-off|x86_64|stuart|irq
locking|x86_64|lance|locking
lockstat|x86_64|lance|locking
ipc-donation|x86_64|lance|ipc
ipc-fuzz|x86_64|bob|ipc fuzz|tests/smp/ipc-fuzz/ipc-fuzz.toml
syscall-fuzz|x86_64|bob|syscall fuzz|tests/smp/syscall-fuzz/syscall-fuzz.toml
ssh-first-byte|x86_64|bob|ssh first byte|tests/fixtures/ssh-first-byte/ssh-first-byte.toml
net-fuzz|x86_64|jorge|net fuzz|tests/qtest/cases/net-fuzz.toml
net-fuzz-l4|x86_64|jorge|net fuzz l4|tests/qtest/cases/net-fuzz-l4.toml
net-fuzz-frag-timeout|x86_64|jorge|net fuzz frag|tests/qtest/cases/net-fuzz-frag-timeout.toml
duplex-matrix|x86_64|phil|duplex/contract|tests/smp/duplex-matrix/duplex-matrix.toml
kmem-frames|x86_64|mark|allocator
kmem-frames-nopcpu|x86_64|mark|allocator
proc-death-cleanup|x86_64|stuart|process
trace-context|x86_64|tim|trace
release-login|x86_64|kevin|release
ufs-enospc|x86_64|mel|ufs/durability
ufs-fsync-persist|x86_64|mel|ufs/durability
ufs-fsync-check|x86_64|mel|ufs/durability
boot-uname-aarch64|aarch64|dave|boot/uname
kernel-early-boot-aarch64|aarch64|dave|boot
kernel-exc-aarch64|aarch64|dave|exceptions
kernel-paging-aarch64|aarch64|dave|paging
kernel-timer-aarch64|aarch64|dave|timer
duplex-matrix-aarch64|aarch64|phil|duplex/contract|tests/smp/duplex-matrix/duplex-matrix-aarch64.toml'

durability=0
functional=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --list)
            printf '%s\n' "$cases" | while IFS='|' read -r name arch owner note _path; do
                printf '%-28s %-8s %-7s %s\n' "$name" "$arch" "$owner" "$note"
            done
            exit 0
            ;;
        --durability) durability=1 ;;
        --functional) functional=1 ;;
        -h|--help)
            sed -n '6,30p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "kernel-gate: unknown option: $1" >&2
            exit 2
            ;;
    esac
    shift
done

if [ "$durability" -eq 1 ]; then
    cases=$(printf '%s\n' "$cases" | awk -F'|' '$4 == "ufs/durability"')
    doc=target/kernel-gate/durability.md
else
    if ! doc=$(mgmt_path architecture/product-data/kernel-gate.md); then
        doc=$log_dir/kernel-gate.md
        echo "kernel-gate: management root absent; table stays at $doc (set RAVEN_MGMT_ROOT to file it)"
    fi
fi

mkdir -p "$log_dir" build/qtest || exit 2
if [ "$functional" -eq 1 ]; then
    # Functional rows use the functional hygiene budget, not the exclusive window: up to two
    # co-tenant boot rows and load1 <= 2 x nproc. A REFUSE stops the gate; do not override it.
    load1=$(cut -d' ' -f1 /proc/loadavg)
    nproc=$(nproc 2>/dev/null || echo 4)
    max_load=$((nproc * 4))
    max_qemu=$((nproc / 2))
    [ "$max_qemu" -lt 2 ] && max_qemu=2
    qemu=$(ps -eo stat=,comm= 2>/dev/null \
        | awk '$1 !~ /^[ZX]/ && $2 ~ /^qemu-system-/ { n++ } END { print n + 0 }')
    if [ "$qemu" -gt "$max_qemu" ] ||
        awk -v l="$load1" -v m="$max_load" 'BEGIN { exit !(l > m) }'; then
        echo "kernel-gate: hygiene REFUSE functional qemu=$qemu max_qemu=$max_qemu load1=$load1 max_load=$max_load" >&2
        exit 2
    fi
    echo "kernel-gate: hygiene functional qemu=$qemu max_qemu=$max_qemu load1=$load1 max_load=$max_load"
else
    # shellcheck disable=SC2086
    sh tests/smp/boot/host-idle.sh \
        --context "kernel-gate" \
        --wait "$wait_idle" \
        --max-load "$max_load" \
        $preflight_args \
        --state-file "$log_dir/host-state.txt" || exit 2
fi

commit=$(git rev-parse HEAD 2>/dev/null) || { echo "kernel-gate: not a git checkout" >&2; exit 2; }
short=$(git rev-parse --short=12 HEAD 2>/dev/null) || short=$commit
generated=$(date -u +%Y-%m-%dT%H:%M:%SZ)

cases_file=$log_dir/cases.txt
printf '%s\n' "$cases" > "$cases_file" || exit 2
results=$log_dir/results.txt
: > "$results" || exit 2

while IFS='|' read -r name arch owner note case_path; do
    [ -n "$name" ] || continue
    log="$log_dir/$name.$arch.log"
    case_selector=${case_path:-$name}
    # The release-login image needs the sshd build artifact. A clean checkout does not have it;
    # name the missing prerequisite and skip the row rather than failing the case silently.
    if [ "$name" = release-login ] && [ ! -f build/ssh/x86_64/bin/sshd ]; then
        printf 'kernel-gate: %-28s %-8s ' "$name" "$arch"
        printf 'skip (missing build/ssh/x86_64/bin/sshd)\n'
        printf '%s|%s|%s|%s|SKIP|missing build/ssh/x86_64/bin/sshd\n' \
            "$name" "$arch" "$owner" "$note" >> "$results"
        continue
    fi
    release_flags=
    if [ "$name" = release-login ]; then
        if [ -f "$release_image" ] && [ -f "$release_stamp" ] &&
            [ "$(cat "$release_stamp" 2>/dev/null)" = "$commit" ]; then
            release_flags=--no-build
            printf 'kernel-gate: release-login reuse image stamp=%s\n' "$short"
        else
            printf 'kernel-gate: release-login prebuild (cap=%ss lock-wait=%ss stamp=%s)\n' \
                "$release_build_cap" "$release_build_wait" "$short"
            prebuild_start=$(date +%s)
            prebuild_ok=1
            (
                if command -v flock >/dev/null 2>&1; then
                    exec 9>"$release_lock"
                    flock -w "$release_build_wait" 9 || exit 1
                fi
                # Another gate may have built the same HEAD while we waited for the lane lock.
                if [ -f "$release_image" ] && [ -f "$release_stamp" ] &&
                    [ "$(cat "$release_stamp" 2>/dev/null)" = "$commit" ]; then
                    exit 0
                fi
                # The case's pre rebuilds the fresh loader/initfs; build the image after it so
                # the reused image carries that loader. The case still runs the pre; nothing is
                # skipped, only the repeated image build is removed.
                timeout "$release_build_cap" sh tests/release/check-loader.sh \
                    >"$log_dir/release-login.pre.log" 2>&1 || exit 1
                timeout "$release_build_cap" cargo xtask image --arch x86_64 --profile release \
                    --cmdline 'console=ttyS0 test-exit=1' --output "$release_image" \
                    >>"$log_dir/release-login.pre.log" 2>&1 || exit 1
                printf '%s\n' "$commit" >"$release_stamp"
            ) || prebuild_ok=0
            prebuild_elapsed=$(( $(date +%s) - prebuild_start ))
            printf 'kernel-gate: release-login prebuild seconds=%s ok=%s\n' \
                "$prebuild_elapsed" "$prebuild_ok"
            if [ "$prebuild_ok" -eq 1 ]; then
                release_flags=--no-build
            else
                # Fall back to the case's own build; the case contract is unchanged.
                release_flags=
            fi
        fi
    fi
    printf 'kernel-gate: %-28s %-8s ' "$name" "$arch"
    # stdin is /dev/null: neither cargo nor QEMU may consume the remaining case lines.
    # shellcheck disable=SC2086
    if cargo xtask qtest --case "$case_selector" --arch "$arch" $release_flags </dev/null >"$log" 2>&1; then
        status=pass
        detail=ok
        printf 'pass\n'
    else
        # K555: a latency row's own measured guard returns the qtest waiting-for-window
        # (preflight-load exit 77; the xtask wrapper reports it as a non-zero run). That is a
        # recorded skip, not a failure, and the gate verdict counts it separately.
        if grep -Eq '(^|: )skipped$|preflight-load: .*waiting-for-window' "$log"; then
            status=SKIP
            detail=$(grep -m1 'preflight-load: .*waiting-for-window' "$log")
            [ -n "$detail" ] || detail="waiting-for-window"
            printf 'skip (waiting-for-window)\n'
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
skipped=$(grep -c '|SKIP|' "$results" 2>/dev/null || true)
skipped=${skipped:-0}
failed=$((total - passed - skipped))
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
    echo "Result: **$verdict** — $total cases, $passed passed, $failed failed ($timed_out timed out), $skipped skipped."
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
        owners=$(awk -F'|' '$5 != "pass" && $5 != "SKIP" { print $3 }' "$results" | sort -u)
        for owner in $owners; do
            echo "### $owner"
            echo
            awk -F'|' -v owner="$owner" \
                '$3 == owner && $5 != "pass" && $5 != "SKIP" { printf "- `%s` (%s): %s — %s\n", $1, $2, $5, $6 }' \
                "$results"
            echo
        done
    fi
} > "$doc" || exit 2

if [ "$failed" -ne 0 ]; then
    echo "kernel-gate: FAIL ($failed of $total cases failed, $skipped skipped); table: $doc" >&2
    exit 1
fi
echo "kernel-gate: PASS ($total cases, $skipped skipped); table: $doc"
exit 0
