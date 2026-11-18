#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Deterministic 24-hour fault-injection soak evidence checker.
#
# A release 24 h soak must be one complete run of:
#
#   /usr/bin/time -p sh scripts/fault-injection.sh \
#       --profile release --arch x86_64 --soak 86400
#
# This checker refuses a short run, an incomplete/interrupted log, a missing log, a failed
# iteration, a crash count above zero, and a log without an identifiable arch/profile/commit.
# It never runs QEMU.
#
# Usage:
#   sh tests/scripts/check-soak-24h.sh --log FILE [--qtest-log FILE]
#                                      [--arch ARCH] [--profile PROFILE] [--commit SHA]
#   sh tests/scripts/check-soak-24h.sh --self-test
#
# Evidence sources:
#   * `fault-injection: run ...` lines give the completed iteration count.
#   * `fault-injection: soak done pass=... fail=... first_failure=...` is the final summary.
#   * elapsed comes from `elapsed_s=`/`elapsed=` metadata, a `time -p` `real` line, a
#     `--soak` budget on a captured command line or `budget_s=`, or a recovery-time lower bound.
#   * arch/profile/commit come from log tokens (`arch=`, `profile=`, `commit=`), a captured
#     command line, a qtest serial log with the kernel banner, or the explicit options.
#
# Exit codes: 0 complete clean 24 h evidence; 1 evidence refused; 2 usage or unreadable input.

set -eu

min_seconds=86400
log=
qtest_log=
expected_arch=x86_64
expected_profile=release
arch_arg=
profile_arg=
commit_arg=
self_test=0

usage() {
    cat <<'EOF'
usage: sh tests/scripts/check-soak-24h.sh --log FILE [--qtest-log FILE]
                                          [--arch ARCH] [--profile PROFILE] [--commit SHA]
       sh tests/scripts/check-soak-24h.sh --self-test

  --log FILE        foreground output of the fault-injection soak (required).
  --qtest-log FILE  last qtest serial log; its kernel banner supplies arch and commit.
  --arch ARCH       expected arch (default x86_64); also supplies it if the log lacks it.
  --profile PROFILE expected profile (default release).
  --commit SHA      expected commit; also supplies it if the log lacks it.
  --self-test       run the fixture checks and exit; opens no QEMU case.
EOF
}

usage_error() {
    printf 'soak-24h-check: %s\n' "$*" >&2
    exit 2
}

refuse() {
    printf 'soak-24h-check: FAIL %s\n' "$*" >&2
    exit 1
}

is_uint() {
    case $1 in
        '' | *[!0-9]*) return 1 ;;
        *) return 0 ;;
    esac
}

while [ "$#" -gt 0 ]; do
    case $1 in
        --log)
            [ "$#" -ge 2 ] || usage_error "--log needs a value"
            log=$2
            shift 2
            ;;
        --qtest-log)
            [ "$#" -ge 2 ] || usage_error "--qtest-log needs a value"
            qtest_log=$2
            shift 2
            ;;
        --arch)
            [ "$#" -ge 2 ] || usage_error "--arch needs a value"
            arch_arg=$2
            expected_arch=$2
            shift 2
            ;;
        --profile)
            [ "$#" -ge 2 ] || usage_error "--profile needs a value"
            profile_arg=$2
            expected_profile=$2
            shift 2
            ;;
        --commit)
            [ "$#" -ge 2 ] || usage_error "--commit needs a value"
            commit_arg=$2
            shift 2
            ;;
        --self-test)
            self_test=1
            shift
            ;;
        --help | -h)
            usage
            exit 0
            ;;
        *)
            usage_error "unknown argument: $1"
            ;;
    esac
done

# Last `key=value` token in a log file (commas and quotes stripped).
token_of() {
    key=$1
    file=$2
    awk -v key="$key" '
        {
            for (i = 1; i <= NF; i++) {
                if (index($i, key "=") == 1) {
                    value = substr($i, length(key) + 2)
                }
            }
        }
        END {
            gsub(/[",]/, "", value)
            if (value != "") print value
        }
    ' "$file"
}

# The captured command line, if the log includes one.
command_line() {
    grep -E 'fault-injection\.sh' "$1" | tail -1
}

# Value following `--flag` on a captured command line.
command_flag() {
    flag=$1
    file=$2
    line=$(command_line "$file")
    [ -n "$line" ] || return 0
    printf '%s\n' "$line" | awk -v flag="$flag" '
        {
            for (i = 1; i < NF; i++) {
                if ($i == flag) value = $(i + 1)
            }
        }
        END {
            gsub(/[",]/, "", value)
            if (value != "") print value
        }
    '
}

# Seconds from a soak duration: <n>, <n>s, <n>m, <n>h or <a>h<b>m.
parse_duration() {
    value=$1
    case $value in
        '') return 1 ;;
        *h*m)
            hours=${value%h*m}
            minutes=${value#*h}
            minutes=${minutes%m}
            case $hours in '' | *[!0-9]*) return 1 ;; esac
            case $minutes in '' | *[!0-9]*) return 1 ;; esac
            [ "${#hours}" -le 5 ] || return 1
            [ "${#minutes}" -le 5 ] || return 1
            printf '%s\n' $((hours * 3600 + minutes * 60))
            ;;
        *h)
            number=${value%h}
            case $number in '' | *[!0-9]*) return 1 ;; esac
            [ "${#number}" -le 5 ] || return 1
            printf '%s\n' $((number * 3600))
            ;;
        *m)
            number=${value%m}
            case $number in '' | *[!0-9]*) return 1 ;; esac
            [ "${#number}" -le 9 ] || return 1
            printf '%s\n' $((number * 60))
            ;;
        *s)
            number=${value%s}
            case $number in '' | *[!0-9]*) return 1 ;; esac
            [ "${#number}" -le 9 ] || return 1
            printf '%s\n' "$number"
            ;;
        *)
            case $value in '' | *[!0-9]*) return 1 ;; esac
            [ "${#value}" -le 9 ] || return 1
            printf '%s\n' "$value"
            ;;
    esac
}

self_test() {
    root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
    fixtures=$root/tests/scripts/fixtures/soak-24h
    script=$root/tests/scripts/check-soak-24h.sh
    failures=0

    pass() {
        printf 'soak-24h-check self-test: ok   %s\n' "$1"
    }

    fail() {
        printf 'soak-24h-check self-test: FAIL %s\n' "$1" >&2
        failures=$((failures + 1))
    }

    expect_exit() {
        expected=$1
        description=$2
        shift 2
        if "$@" >/dev/null 2>&1; then
            actual=0
        else
            actual=$?
        fi
        if [ "$actual" -eq "$expected" ]; then
            pass "$description"
        else
            fail "$description (exit $actual, expected $expected)"
        fi
    }

    expect_message() {
        expected=$1
        description=$2
        shift 2
        output=$("$@" 2>&1) || true
        case $output in
            *"$expected"*) pass "$description" ;;
            *) fail "$description (no '$expected' in output)" ;;
        esac
    }

    expect_exit 0 "shell syntax" sh -n "$script"
    expect_exit 0 "good 24 h log" \
        sh "$script" --log "$fixtures/good.log" --qtest-log "$fixtures/qtest.log"
    expect_exit 0 "good log with explicit arch/profile/commit" \
        sh "$script" --log "$fixtures/good.log" --arch x86_64 --profile release --commit 1aa900c3
    expect_exit 0 "real-only log with qtest sidecar" \
        sh "$script" --log "$fixtures/real-only.log" --qtest-log "$fixtures/qtest.log" --profile release
    expect_exit 1 "short run" \
        sh "$script" --log "$fixtures/short.log" --qtest-log "$fixtures/qtest.log"
    expect_exit 2 "missing log" sh "$script" --log "$fixtures/missing.log"
    expect_exit 1 "crash run" \
        sh "$script" --log "$fixtures/crash.log" --qtest-log "$fixtures/qtest.log"
    expect_exit 1 "first-failure run" \
        sh "$script" --log "$fixtures/first-failure.log" --qtest-log "$fixtures/qtest.log"
    expect_exit 1 "incomplete log" \
        sh "$script" --log "$fixtures/incomplete.log" --qtest-log "$fixtures/qtest.log"
    expect_exit 1 "no elapsed evidence" \
        sh "$script" --log "$fixtures/no-elapsed.log" --qtest-log "$fixtures/qtest.log"
    expect_exit 1 "iteration count mismatch" \
        sh "$script" --log "$fixtures/iteration-mismatch.log" --qtest-log "$fixtures/qtest.log"
    expect_exit 1 "arch mismatch" \
        sh "$script" --log "$fixtures/good.log" --qtest-log "$fixtures/qtest.log" --arch aarch64
    expect_exit 1 "profile mismatch" \
        sh "$script" --log "$fixtures/good.log" --qtest-log "$fixtures/qtest.log" --profile debug
    expect_exit 1 "commit mismatch" \
        sh "$script" --log "$fixtures/good.log" --qtest-log "$fixtures/qtest.log" --commit deadbeef
    expect_exit 2 "--log without a value" sh "$script" --log
    expect_exit 2 "unknown argument" sh "$script" --wat

    expect_message "elapsed 1801s is below 86400s" "short message" \
        sh "$script" --log "$fixtures/short.log" --qtest-log "$fixtures/qtest.log"
    expect_message "crashes=1" "crash message" \
        sh "$script" --log "$fixtures/crash.log" --qtest-log "$fixtures/qtest.log"
    expect_message "first_failure=2026-11-18T01:02:03Z" "first-failure message" \
        sh "$script" --log "$fixtures/first-failure.log" --qtest-log "$fixtures/qtest.log"
    expect_message "missing final soak summary" "incomplete message" \
        sh "$script" --log "$fixtures/incomplete.log" --qtest-log "$fixtures/qtest.log"

    if [ "$failures" -ne 0 ]; then
        printf 'soak-24h-check: self-test FAILED (%s checks)\n' "$failures" >&2
        return 1
    fi
    printf 'soak-24h-check: self-test PASS (good/real-only/short/missing/crash/first-failure/incomplete/no-elapsed/iteration-mismatch)\n'
    return 0
}

if [ "$self_test" -eq 1 ]; then
    self_test
    exit $?
fi

[ -n "$log" ] || usage_error "--log FILE is required (or use --self-test)"
[ -r "$log" ] || usage_error "log not found or unreadable: $log"
[ -s "$log" ] || refuse "empty soak log: $log"

# Metadata from the log itself and from a captured command line.
log_arch=$(token_of arch "$log")
cmd_arch=$(command_flag --arch "$log")
log_profile=$(token_of profile "$log")
cmd_profile=$(command_flag --profile "$log")
log_commit=$(token_of commit "$log")
if [ -z "$log_commit" ]; then
    log_commit=$(sed -n 's/^# *commit: *\([0-9a-f][0-9a-f]*\).*/\1/p' "$log" | tail -1)
fi

qtest_arch=
qtest_commit=
if [ -n "$qtest_log" ]; then
    [ -r "$qtest_log" ] || usage_error "qtest log not found or unreadable: $qtest_log"
    banner=$(sed -n 's/^RavenBSD kernel 0\.1 (\([^)]*\)) \([0-9a-f][0-9a-f]*\).*/\1 \2/p' "$qtest_log" | tail -1)
    if [ -n "$banner" ]; then
        qtest_arch=${banner% *}
        qtest_commit=${banner#* }
    fi
fi

check_matches() {
    name=$1
    expected=$2
    shift 2
    found=0
    for value in "$@"; do
        [ -n "$value" ] || continue
        found=1
        [ "$value" = "$expected" ] || refuse "$name mismatch: $value != $expected"
    done
    [ "$found" -eq 1 ] || refuse "missing $name evidence (log, qtest log or --$name)"
}

check_matches arch "$expected_arch" "$log_arch" "$cmd_arch" "$qtest_arch" "$arch_arg"
check_matches profile "$expected_profile" "$log_profile" "$cmd_profile" "$profile_arg"

commit=
for value in "$log_commit" "$qtest_commit" "$commit_arg"; do
    [ -n "$value" ] || continue
    case $value in
        *[!0-9a-f]*) refuse "invalid commit evidence: $value" ;;
    esac
    [ "${#value}" -ge 7 ] && [ "${#value}" -le 40 ] || refuse "invalid commit evidence: $value"
    if [ -z "$commit" ]; then
        commit=$value
    elif [ "$commit" != "$value" ]; then
        # A short id and the full id are the same commit when one is a prefix of the other.
        case $commit in
            "$value"*) ;;
            *)
                case $value in
                    "$commit"*) commit=$value ;;
                    *) refuse "commit mismatch: $commit != $value" ;;
                esac
                ;;
        esac
    fi
done
[ -n "$commit" ] || refuse "missing commit evidence (log, qtest log or --commit)"

# Final summary and completed iteration count. Check completeness before elapsed: a live or
# interrupted soak has no summary, and that is a clearer refusal than a missing wall-clock line.
summary=$(grep -E '^fault-injection: soak done pass=[0-9]+ fail=[0-9]+ first_failure=[^[:space:]]+$' "$log" | tail -1 || true)
[ -n "$summary" ] || refuse "missing final soak summary (incomplete log)"
last_line=$(grep '^fault-injection:' "$log" | tail -1 || true)
[ "$last_line" = "$summary" ] || refuse "soak output continues after the summary (incomplete log)"

pass=$(printf '%s\n' "$summary" | sed -n 's/.* pass=\([0-9][0-9]*\) fail=.*/\1/p')
fail=$(printf '%s\n' "$summary" | sed -n 's/.* fail=\([0-9][0-9]*\) first_failure=.*/\1/p')
first_failure=$(printf '%s\n' "$summary" | sed -n 's/.* first_failure=\([^[:space:]]*\)$/\1/p')
is_uint "$pass" || refuse "invalid pass count in the summary: $summary"
is_uint "$fail" || refuse "invalid fail count in the summary: $summary"
[ -n "$first_failure" ] || refuse "invalid first_failure in the summary: $summary"

run_count=$(grep -cE '^fault-injection: run pass=[0-9]+ fail=[0-9]+ recovery_ms=[0-9]+$' "$log" || true)
is_uint "$run_count" || refuse "invalid iteration count: $run_count"
[ "$run_count" -gt 0 ] || refuse "no complete soak iterations"
[ $((pass + fail)) -eq "$run_count" ] ||
    refuse "iteration count mismatch: run lines=$run_count pass+fail=$((pass + fail))"
[ "$pass" -gt 0 ] || refuse "no passing iterations (pass=0)"

runs=$(token_of runs "$log")
if [ -n "$runs" ]; then
    is_uint "$runs" || refuse "invalid runs value: $runs"
    [ "$runs" -eq "$run_count" ] || refuse "runs=$runs but the log has $run_count run lines"
fi

# Elapsed seconds. Prefer an explicit elapsed value; the smallest credible value wins.
elapsed_s=$(token_of elapsed_s "$log")
[ -n "$elapsed_s" ] || elapsed_s=$(token_of elapsed "$log")
[ -n "$elapsed_s" ] || elapsed_s=$(token_of duration_s "$log")
real_s=$(awk '/^real[[:space:]]+[0-9]+([.][0-9]+)?[[:space:]]*$/ {
    value = $2
    sub(/\..*/, "", value)
    last = value
}
END { if (last != "") print last }' "$log")
budget_s=$(token_of budget_s "$log")
[ -n "$budget_s" ] || budget_s=$(token_of soak_s "$log")
if [ -z "$budget_s" ]; then
    budget_arg=$(command_flag --soak "$log")
    if [ -n "$budget_arg" ]; then
        budget_s=$(parse_duration "$budget_arg" 2>/dev/null) || budget_s=
    fi
fi

elapsed=
if [ -n "$elapsed_s" ]; then
    is_uint "$elapsed_s" || refuse "invalid elapsed_s value: $elapsed_s"
    elapsed=$elapsed_s
fi
if [ -n "$real_s" ]; then
    is_uint "$real_s" || refuse "invalid time -p real value: $real_s"
    if [ -z "$elapsed" ] || [ "$real_s" -lt "$elapsed" ]; then
        elapsed=$real_s
    fi
fi
if [ -z "$elapsed" ] && [ -n "$budget_s" ]; then
    is_uint "$budget_s" || refuse "invalid budget_s value: $budget_s"
    elapsed=$budget_s
fi
if [ -z "$elapsed" ]; then
    recovery_ms=$(awk '
        /^fault-injection: run / {
            for (i = 1; i <= NF; i++) {
                if (index($i, "recovery_ms=") == 1) sum += substr($i, 13)
            }
        }
        END { printf "%d\n", sum }
    ' "$log")
    if [ "$recovery_ms" -ge $((min_seconds * 1000)) ]; then
        elapsed=$((recovery_ms / 1000))
    fi
fi
[ -n "$elapsed" ] || refuse "missing elapsed evidence (need elapsed_s=, time -p real, a --soak budget or enough recovery_ms)"
[ "$elapsed" -ge "$min_seconds" ] || refuse "elapsed ${elapsed}s is below ${min_seconds}s"

if [ "$fail" -ne 0 ]; then
    [ "$first_failure" != "none" ] || refuse "fail=$fail but first_failure=none (inconsistent log)"
    refuse "fail=$fail first_failure=$first_failure (24 h evidence requires 0 failures)"
fi
[ "$first_failure" = "none" ] || refuse "first_failure=$first_failure but fail=0 (inconsistent log)"

crashes=$(token_of crashes "$log")
if [ -n "$crashes" ]; then
    is_uint "$crashes" || refuse "invalid crashes value: $crashes"
    [ "$crashes" -eq 0 ] || refuse "crashes=$crashes (24 h evidence requires 0 crashes)"
else
    crashes=0
fi
if grep -qE 'result=crash|kernel panic|panic:' "$log"; then
    refuse "crash or panic marker in the soak log"
fi

printf 'soak-24h-check: PASS elapsed_s=%s runs=%s pass=%s fail=%s crashes=%s first_failure=none arch=%s profile=%s commit=%s\n' \
    "$elapsed" "$run_count" "$pass" "$fail" "$crashes" "$expected_arch" "$expected_profile" "$commit"
