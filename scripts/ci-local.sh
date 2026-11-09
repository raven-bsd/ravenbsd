#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# usage: scripts/ci-local.sh
# Runs `make check` and the QEMU integration tests for every architecture whose emulator is
# installed, then writes build/ci/summary.json. Exits non-zero if any step failed.
set -u
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT"

mkdir -p build/ci
summary=build/ci/summary.json
printf '[' > "$summary"
first=1
failed=0

record() {
	name=$1
	code=$2
	seconds=$3
	if [ "$first" -eq 1 ]; then
		first=0
	else
		printf ',' >> "$summary"
	fi
	printf '{"step":"%s","exit":%s,"seconds":%s}' "$name" "$code" "$seconds" >> "$summary"
}

run_step() {
	name=$1
	shift
	printf '==> %s\n' "$name"
	start=$(date +%s)
	"$@"
	code=$?
	end=$(date +%s)
	record "$name" "$code" "$((end - start))"
	if [ "$code" -ne 0 ]; then
		failed=1
	fi
}

run_step "make check" make check

if command -v qemu-system-x86_64 >/dev/null 2>&1; then
	run_step "qtest x86_64" cargo xtask qtest --all --arch x86_64
else
	printf '==> qtest x86_64: skipped (no qemu-system-x86_64)\n'
fi

if command -v qemu-system-aarch64 >/dev/null 2>&1; then
	run_step "qtest aarch64" cargo xtask qtest --all --arch aarch64
else
	printf '==> qtest aarch64: skipped (no qemu-system-aarch64)\n'
fi

printf ']\n' >> "$summary"
printf 'summary: %s\n' "$summary"
exit "$failed"
