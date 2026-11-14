#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host tests for tests/libc/target/pg-probe.sh. Each fixture tree exercises one present/absent
# combination of the C73 4.2 probes; no product file is touched.

set -eu

here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
probe=$here/../pg-probe.sh
tmp=${TMPDIR:-/tmp}/rl-pg-probe-test-$$
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

passed=0
failed=0

ok() {
    passed=$((passed + 1))
}

bad() {
    failed=$((failed + 1))
    printf 'pg-probe-test: FAIL %s\n' "$1" >&2
}

assert_contains() {
    if printf '%s\n' "$1" | grep -Fq "$2"; then
        ok
    else
        bad "missing: $2"
    fi
}

assert_not_contains() {
    if printf '%s\n' "$1" | grep -Fq "$2"; then
        bad "unexpected: $2"
    else
        ok
    fi
}

write_file() {
    mkdir -p "$(dirname "$1")"
    printf '%s\n' "$2" > "$1"
}

new_tree() {
    tree=$tmp/$1
    mkdir -p "$tree/tests/libc/target"
    printf 'case\thard\tsoft\tbinaries\tlink\n' > "$tree/tests/libc/target/cases.tsv"
    printf 'alpha\t1\t52\tx\tstatic\n' >> "$tree/tests/libc/target/cases.tsv"
    printf 'beta\t29\t\tx\tstatic\n' >> "$tree/tests/libc/target/cases.tsv"
    printf 'gamma\t49\t\tx\tstatic\n' >> "$tree/tests/libc/target/cases.tsv"
    printf 'delta\t23\t\tx\tstatic\n' >> "$tree/tests/libc/target/cases.tsv"
}

run_probe() {
    RL_PROBE_ROOT="$1" RL_PROBE_TSV="$1/tests/libc/target/cases.tsv" sh "$probe" "$2"
}

new_tree present
write_file "$tree/crates/raven-abi/src/syscall.rs" 'pub const THREAD_SET_TLS: usize = 0x23;'
write_file "$tree/crates/raven-abi/src/errno.rs" 'pub const ENOTSUP: i32 = 1; pub const EDEADLK: i32 = 2;'
write_file "$tree/userland/rt/src/spawn.rs" 'fn spawn() {}'
out=$(run_probe "$tree" alpha)
assert_contains "$out" 'PG-1: present'
assert_contains "$out" 'case alpha: RUNNABLE'
out=$(run_probe "$tree" beta)
assert_contains "$out" 'PG-29: present'
assert_contains "$out" 'case beta: RUNNABLE'
out=$(run_probe "$tree" gamma)
assert_contains "$out" 'PG-49: present'
assert_contains "$out" 'case gamma: RUNNABLE'

new_tree missing
out=$(run_probe "$tree" alpha)
assert_contains "$out" 'PG-1: absent'
assert_contains "$out" 'case alpha: BLOCKED needs PG-1'
out=$(run_probe "$tree" beta)
assert_contains "$out" 'PG-29: absent'
assert_contains "$out" 'case beta: BLOCKED needs PG-29'
out=$(run_probe "$tree" gamma)
assert_contains "$out" 'PG-49: unknown'
assert_contains "$out" 'case gamma: BLOCKED needs PG-49'

new_tree half-errno
write_file "$tree/crates/raven-abi/src/errno.rs" 'pub const ENOTSUP: i32 = 1;'
out=$(run_probe "$tree" beta)
assert_contains "$out" 'PG-29: absent'
assert_contains "$out" 'case beta: BLOCKED needs PG-29'

new_tree interpreter
write_file "$tree/userland/rt/src/spawn.rs" 'return Err(SpawnError::HasInterpreter);'
out=$(run_probe "$tree" gamma)
assert_contains "$out" 'PG-49: absent'
assert_contains "$out" 'case gamma: BLOCKED needs PG-49'

new_tree unknown
out=$(run_probe "$tree" delta)
assert_contains "$out" 'PG-23: unknown'
assert_contains "$out" 'case delta: BLOCKED needs PG-23'

new_tree scheme-seek
printf 'alt\t22\t\tx\tstatic\n' >> "$tree/tests/libc/target/cases.tsv"
write_file "$tree/crates/raven-scheme/src/lib.rs" 'pub enum Call { Seek(u64) }'
out=$(run_probe "$tree" alt)
assert_contains "$out" 'PG-22: present'
assert_contains "$out" 'case alt: RUNNABLE'

new_tree syscall-pread
printf 'alt\t22\t\tx\tstatic\n' >> "$tree/tests/libc/target/cases.tsv"
write_file "$tree/crates/raven-abi/src/syscall.rs" 'pub const PREAD: usize = 0x90;'
out=$(run_probe "$tree" alt)
assert_contains "$out" 'PG-22: present'
assert_contains "$out" 'case alt: RUNNABLE'

new_tree shape-config
printf 'shape\t53\t\tx\tstatic\n' >> "$tree/tests/libc/target/cases.tsv"
write_file "$tree/.cargo/config.toml" 'rustflags = ["-C", "link-arg=-zmax-page-size=4096"]'
out=$(run_probe "$tree" shape)
assert_contains "$out" 'PG-53: present'
assert_contains "$out" 'case shape: RUNNABLE'

new_tree prefixed
write_file "$tree/crates/raven-abi/src/syscall.rs" 'pub const THREAD_SET_TLS: usize = 0x23;'
out=$(run_probe "$tree" libc-alpha)
assert_contains "$out" 'case alpha: RUNNABLE'
assert_not_contains "$out" 'case beta:'

out=$(RL_PROBE_ROOT="$tmp/none" RL_PROBE_TSV="$tmp/none/cases.tsv" sh "$probe" all 2>/dev/null)
if [ -z "$out" ]; then ok; else bad "missing tsv printed: $out"; fi

new_tree allmode
write_file "$tree/crates/raven-abi/src/syscall.rs" 'pub const THREAD_SET_TLS: usize = 0x23;'
out=$(run_probe "$tree" all)
assert_contains "$out" 'PG-1: present'
assert_contains "$out" 'case alpha: RUNNABLE'
assert_contains "$out" 'case beta: BLOCKED needs PG-29'
assert_contains "$out" 'case gamma: BLOCKED needs PG-49'
assert_contains "$out" 'case delta: BLOCKED needs PG-23'

printf 'pg-probe-test: %s passed, %s failed\n' "$passed" "$failed"
if [ "$failed" -ne 0 ]; then
    exit 1
fi
