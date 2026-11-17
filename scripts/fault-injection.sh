#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S71 deterministic server-death fault-injection driver.
#
# With no selector it runs the cross-architecture fault cases:
#
#   fault-restart    after-reply, during-restart and idle STOP/START on the `frt` worker
#   fault-reconnect  in-flight request death: EIO/EPIPE, generation change, bounded reopen
#
# `--profile release` selects the x86_64 release-image matrix (netd and vfsd through svcctl).
# `--case <name>` runs one case. With `--soak <duration>` the selected set repeats until the
# budget is exhausted and the driver reports the pass/fail counts, the per-run wall time and the
# first failure.
#
# A duration is a positive integer with an optional `s`, `m` or `h` suffix (`90`, `90s`, `2m`,
# `24h`) or a combination of at most two units (`1h30m`). A bare number is seconds. Anything else
# is a usage error (exit 2), and a budget that would run zero iterations is a usage error too: a
# zero-run soak is never reported as a pass.
#
# `--self-test` exercises the duration parser and exits; it opens no QEMU case.

set -eu

profile=
arch=x86_64
soak=0
soak_given=0
soak_arg=
selected_case=

usage() {
    cat <<'EOF'
usage: sh scripts/fault-injection.sh [--profile release] [--arch ARCH]
                                     [--case NAME] [--soak DURATION]
       sh scripts/fault-injection.sh --self-test

  --soak DURATION  repeat the selected set until DURATION elapses.
                   Accepts <n>, <n>s, <n>m, <n>h or <a>h<b>m (bare <n> is seconds).
  --self-test      run the duration-parser checks and exit (no QEMU case).
EOF
}

# Parse a duration into whole seconds.
#
# Accepts `<n>`, `<n>s`, `<n>m`, `<n>h`, or `<a>h<b>m`. Prints the seconds on stdout and returns 0,
# or returns 1 without output for anything else (empty, non-numeric, extra suffix, overflow).
parse_duration() {
    value=$1
    case $value in
        '') return 1 ;;
        *h*m)
            hours=${value%h*m}
            minutes=${value#*h}
            minutes=${minutes%m}
            # only digits on both sides of the `h`
            case $hours in ''|*[!0-9]*) return 1 ;; esac
            case $minutes in ''|*[!0-9]*) return 1 ;; esac
            # two units only; cap at 10 years so the shell arithmetic cannot overflow
            [ "${#hours}" -le 5 ] || return 1
            [ "${#minutes}" -le 5 ] || return 1
            printf '%s\n' $(( hours * 3600 + minutes * 60 ))
            ;;
        *h)
            number=${value%h}
            case $number in ''|*[!0-9]*) return 1 ;; esac
            [ "${#number}" -le 5 ] || return 1
            printf '%s\n' $(( number * 3600 ))
            ;;
        *m)
            number=${value%m}
            case $number in ''|*[!0-9]*) return 1 ;; esac
            [ "${#number}" -le 9 ] || return 1
            printf '%s\n' $(( number * 60 ))
            ;;
        *s)
            number=${value%s}
            case $number in ''|*[!0-9]*) return 1 ;; esac
            [ "${#number}" -le 9 ] || return 1
            printf '%s\n' "$number"
            ;;
        *)
            case $value in ''|*[!0-9]*) return 1 ;; esac
            [ "${#value}" -le 9 ] || return 1
            printf '%s\n' "$value"
            ;;
    esac
}

# Duration-parser table: valid forms must parse, invalid forms must fail.
self_test() {
    failures=0
    for spec in 0 30 90s 2m 24h 1h30m 23h59m 0s; do
        if ! parse_duration "$spec" >/dev/null; then
            printf 'fault-injection self-test: rejected valid duration %s\n' "$spec" >&2
            failures=$((failures + 1))
        fi
    done
    for spec in '' abc 24hx -3 1.5h h 5m5m '24 h' 10000000000; do
        if parse_duration "$spec" >/dev/null 2>&1; then
            printf 'fault-injection self-test: accepted invalid duration %s\n' "$spec" >&2
            failures=$((failures + 1))
        fi
    done
    for spec in 90s=90 2m=120 24h=86400 1h30m=5400; do
        want=${spec#*=}
        got=$(parse_duration "${spec%%=*}" 2>/dev/null) || got=invalid
        if [ "$got" != "$want" ]; then
            printf 'fault-injection self-test: %s parsed as %s, want %s\n' "${spec%%=*}" "$got" "$want" >&2
            failures=$((failures + 1))
        fi
    done
    if [ "$failures" -ne 0 ]; then
        printf 'fault-injection self-test: FAIL (%s checks)\n' "$failures" >&2
        return 1
    fi
    printf 'fault-injection self-test: PASS\n'
}

# Each option that takes a value needs one.
needs_value() {
    [ "$#" -ge 2 ] || {
        printf 'fault-injection: %s needs a value\n' "$1" >&2
        exit 2
    }
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --profile)
            needs_value "$@"
            profile=$2
            shift 2
            ;;
        --arch)
            needs_value "$@"
            arch=$2
            shift 2
            ;;
        --soak)
            needs_value "$@"
            soak_given=1
            soak_arg=$2
            shift 2
            ;;
        --case)
            needs_value "$@"
            selected_case=$2
            shift 2
            ;;
        --self-test)
            self_test
            exit $?
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            printf 'fault-injection: unknown option %s\n' "$1" >&2
            exit 2
            ;;
    esac
done

if [ "$soak_given" -eq 1 ]; then
    soak=$(parse_duration "$soak_arg") || {
        printf 'fault-injection: invalid --soak duration %s (use <n>, <n>s, <n>m, <n>h or <a>h<b>m)\n' \
            "$soak_arg" >&2
        exit 2
    }
    if [ "$soak" -le 0 ]; then
        printf 'fault-injection: --soak %s would run zero iterations; refusing to report a pass\n' \
            "$soak_arg" >&2
        exit 2
    fi
fi

if [ -n "$selected_case" ]; then
    cases=$selected_case
elif [ "$profile" = "release" ]; then
    cases=fault-restart-release
elif [ -z "$profile" ]; then
    cases="fault-restart fault-reconnect"
else
    printf 'fault-injection: only --profile release or the default cross-arch set is supported\n' >&2
    exit 2
fi

run_once() {
    for case_name in $cases; do
        cargo xtask qtest --case "$case_name" --arch "$arch" || return $?
    done
}

if [ "$soak" -le 0 ]; then
    run_once
    exit $?
fi

start=$(date +%s)
pass=0
fail=0
first_failure=
while [ $(( $(date +%s) - start )) -lt "$soak" ]; do
    run_start=$(date +%s%3N)
    if run_once >/dev/null 2>&1; then
        pass=$((pass + 1))
    else
        fail=$((fail + 1))
        [ -n "$first_failure" ] || first_failure=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    fi
    run_end=$(date +%s%3N)
    recovery_ms=$((run_end - run_start))
    printf 'fault-injection: run pass=%s fail=%s recovery_ms=%s\n' "$pass" "$fail" "$recovery_ms"
done
printf 'fault-injection: soak done pass=%s fail=%s first_failure=%s\n' "$pass" "$fail" "${first_failure:-none}"
runs=$((pass + fail))
if [ "$runs" -eq 0 ]; then
    printf 'fault-injection: soak ran 0 iterations in %ss; refusing to report a pass\n' "$soak" >&2
    exit 2
fi
[ "$fail" -eq 0 ]
