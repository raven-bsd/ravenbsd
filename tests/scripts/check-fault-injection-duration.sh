#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host checks for scripts/fault-injection.sh that run no QEMU case:
#
#   * shell syntax;
#   * the duration-parser table (`--self-test`);
#   * every malformed `--soak` value is a usage error (exit 2) raised before a case runs;
#   * a zero-run soak is rejected instead of reported as a pass;
#   * a missing option value is a usage error, not an unbound-variable crash.
#
# Run it from the repository root: sh tests/scripts/check-fault-injection-duration.sh

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"

script=scripts/fault-injection.sh
failures=0

pass() {
    printf 'fault-injection-duration-test: ok   %s\n' "$1"
}

fail() {
    printf 'fault-injection-duration-test: FAIL %s\n' "$1" >&2
    failures=$((failures + 1))
}

expect_ok() {
    description=$1
    shift
    if "$@" >/dev/null 2>&1; then
        pass "$description"
    else
        fail "$description (rejected)"
    fi
}

expect_rejected() {
    description=$1
    shift
    if "$@" >/dev/null 2>&1; then
        fail "$description (accepted)"
    else
        pass "$description (rejected)"
    fi
}

expect_ok "shell syntax" sh -n "$script"
expect_ok "duration parser self-test" sh "$script" --self-test
expect_ok "--help" sh "$script" --help
expect_ok "a bare seconds budget parses" sh "$script" --soak 180 --self-test

# Malformed durations must be a usage error before any case runs, so no qtest is started here.
expect_rejected "rejects 24hx" sh "$script" --soak 24hx --case fault-restart
expect_rejected "rejects the old 24h-with-suffix typo" sh "$script" --soak 24hh --case fault-restart
expect_rejected "rejects abc" sh "$script" --soak abc --case fault-restart
expect_rejected "rejects 1.5h" sh "$script" --soak 1.5h --case fault-restart
expect_rejected "rejects a negative budget" sh "$script" --soak -3 --case fault-restart
expect_rejected "rejects a bare suffix" sh "$script" --soak h --case fault-restart
expect_rejected "rejects an overflowing budget" sh "$script" --soak 10000000000 --case fault-restart

# Zero elapsed budget runs no iteration; that is never a pass.
expect_rejected "rejects a 0s soak" sh "$script" --soak 0s --case fault-restart
expect_rejected "rejects a 0m soak" sh "$script" --soak 0m --case fault-restart

# Missing values are usage errors.
expect_rejected "--soak without a value" sh "$script" --soak
expect_rejected "--case without a value" sh "$script" --case

# The rejection reasons are stable enough for an operator to act on.
if ! sh "$script" --soak 24hx --case fault-restart 2>&1 | grep -q 'invalid --soak duration 24hx'; then
    fail "24hx error message names the value"
else
    pass "24hx error message names the value"
fi
if ! sh "$script" --soak 0s --case fault-restart 2>&1 | grep -q 'zero iterations'; then
    fail "0s error message explains the zero-run refusal"
else
    pass "0s error message explains the zero-run refusal"
fi

if [ "$failures" -ne 0 ]; then
    printf 'fault-injection-duration-test: FAIL (%s checks)\n' "$failures" >&2
    exit 1
fi
printf 'fault-injection-duration-test: PASS\n'
