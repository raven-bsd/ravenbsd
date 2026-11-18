#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Bounded Miri runner for libc gate 1.6 (16 3.B.2.1).
#
# Miri is the only nightly tool in the project; normal builds stay on the pinned stable toolchain
# in rust-toolchain.toml. The run is bounded in three ways:
#   * only the gate crate set in tests/libc/miri/crates.txt;
#   * the five crate library suites (`--lib`), plus the one integration binary with a cfg(miri)
#     path (raven-libc-str/tests/alignment.rs); the other integration binaries are enumerated in
#     tests/libc/miri/integration-exclusions.txt with their reasons;
#   * the deterministic long vectors in tests/libc/miri/skip.txt and a per-crate timeout.
#
# Exit: 0 all crates green, 1 a crate failed or timed out, 77 cargo-miri is not installed.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

toolchain=${MIRI_TOOLCHAIN:-nightly}
timeout_secs=${MIRI_TIMEOUT:-1800}
crates=tests/libc/miri/crates.txt
skips=tests/libc/miri/skip.txt
out=build/libc-miri

if ! command -v timeout >/dev/null 2>&1; then
    printf 'check-miri: timeout(1) is required for the bounded run\n' >&2
    exit 1
fi
if ! rustup run "$toolchain" cargo miri --version >/dev/null 2>&1; then
    printf 'check-miri: cargo-miri is not installed for %s\n' "$toolchain" >&2
    printf 'check-miri: install with: rustup component add --toolchain %s miri\n' "$toolchain" >&2
    exit 77
fi

mkdir -p "$out"
{
    printf 'toolchain: %s\n' "$toolchain"
    rustup run "$toolchain" rustc --version
    rustup run "$toolchain" cargo miri --version
} | tee "$out/toolchain.txt"

pass=0
fail=0
failed_names=
for crate in $(sed -e 's/#.*//' -e '/^[[:space:]]*$/d' "$crates"); do
    skip_args=
    for name in $(awk -v c="$crate" '$1 == c { print $2 }' "$skips"); do
        skip_args="$skip_args --skip $name"
    done
    log=$out/$crate.log
    # shellcheck disable=SC2086
    set +e
    timeout "$timeout_secs" \
        rustup run "$toolchain" cargo miri test -p "$crate" --lib -- $skip_args >"$log" 2>&1
    code=$?
    set -e
    if [ "$code" -eq 0 ]; then
        result=$(grep -E '^test result:' "$log" | tail -n 1)
        printf '[PASS] %s: %s\n' "$crate" "${result:-test result missing}"
        pass=$((pass + 1))
    else
        if [ "$code" -eq 124 ]; then
            printf '[TIMEOUT] %s: %s s (see %s)\n' "$crate" "$timeout_secs" "$log"
        else
            printf '[FAIL] %s: exit %s (see %s)\n' "$crate" "$code" "$log"
        fi
        grep -E 'error:|test result:|unsupported operation' "$log" | tail -n 5 | sed 's/^/  /' || true
        fail=$((fail + 1))
        failed_names="$failed_names $crate"
    fi
done

# The only integration binary in the gate set with a cfg(miri) path. It prints the exact
# capability exclusion for the inline-assembly/mmap part and still runs as a real integration test.
log=$out/raven-libc-str-alignment.log
set +e
timeout "$timeout_secs" \
    rustup run "$toolchain" cargo miri test -p raven-libc-str --test alignment >"$log" 2>&1
code=$?
set -e
if [ "$code" -eq 0 ]; then
    result=$(grep -E '^test result:' "$log" | tail -n 1)
    printf '[PASS] raven-libc-str alignment: %s\n' "${result:-test result missing}"
    pass=$((pass + 1))
else
    if [ "$code" -eq 124 ]; then
        printf '[TIMEOUT] raven-libc-str alignment: %s s (see %s)\n' "$timeout_secs" "$log"
    else
        printf '[FAIL] raven-libc-str alignment: exit %s (see %s)\n' "$code" "$log"
    fi
    grep -E 'error:|test result:|unsupported operation' "$log" | tail -n 5 | sed 's/^/  /' || true
    fail=$((fail + 1))
    failed_names="$failed_names raven-libc-str-alignment"
fi

printf 'check-miri: %s pass, %s fail (toolchain %s)\n' "$pass" "$fail" "$toolchain"
if [ "$fail" -ne 0 ]; then
    printf 'check-miri: failed crates:%s\n' "$failed_names" >&2
    exit 1
fi
