#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K400 pre-tag workspace test-build gate. Compiles every host workspace test target with
# `cargo test --no-run`, without running a test and without network. The fast `check-fast` gate
# only builds changed packages; this gate closes that blind spot.
#
# The gate is package-by-package so one failing crate cannot hide another package's test-build
# error. Each package log is kept under build/release/check-build/. A failing package is printed
# with its first compiler error and source location; the group summary is pass/fail per top-level
# workspace area. Exit 0 only when every package test target compiles.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
cd "$root"

logdir=build/release/check-build
rm -rf "$logdir"
mkdir -p "$logdir"

command -v cargo >/dev/null 2>&1 || { echo "check-build: cargo not found" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "check-build: python3 not found" >&2; exit 1; }

cargo metadata --no-deps --format-version 1 >"$logdir/metadata.json"
python3 - "$logdir/metadata.json" >"$logdir/packages.tsv" <<'PY'
import json
import sys

import os
import re

metadata = json.load(open(sys.argv[1], encoding="utf-8"))
root = os.path.abspath(os.path.join(os.path.dirname(sys.argv[1]), "../../.."))
direct_fixture = re.compile(r"tests/fixtures/[^/]+/Cargo\.toml$")
for package in metadata.get("packages", []):
    raven = (package.get("metadata") or {}).get("raven") or {}
    if raven.get("target"):
        continue
    manifest = package["manifest_path"]
    relative = os.path.relpath(manifest, root)
    # The kernel and the direct test fixtures are bare/uefi targets; their host test harness
    # is not the release workspace test target. Nested fixture tools stay in.
    if relative == "kernel/Cargo.toml" or direct_fixture.search(relative):
        continue
    print(package["name"] + "\t" + manifest)
PY

per_package=${CHECK_BUILD_PACKAGE_TIMEOUT:-180}
total=0
failed=0
: >"$logdir/failures.txt"
: >"$logdir/results.tsv"

while IFS="$(printf '\t')" read -r name manifest; do
    [ -n "$name" ] || continue
    total=$((total + 1))
    safe=$(printf '%s' "$name" | tr '/ ' '__')
    log="$logdir/$safe.log"
    relative=${manifest#"$root"/}
    case "$relative" in
        crates/*) group=crates ;;
        libc/*) group=libc ;;
        userland/*) group=userland ;;
        tests/*) group=tests ;;
        kernel/*) group=kernel ;;
        boot/*) group=boot ;;
        tools/*) group=tools ;;
        xtask/*) group=xtask ;;
        *) group=other ;;
    esac
    if timeout "$per_package" cargo test --no-run --offline --locked -p "$name" >"$log" 2>&1; then
        printf '%s\t%s\tpass\n' "$group" "$name" >>"$logdir/results.tsv"
        continue
    fi
    code=$?
    failed=$((failed + 1))
    printf '%s\t%s\tfail\n' "$group" "$name" >>"$logdir/results.tsv"
    if [ "$code" -eq 124 ]; then
        printf 'check-build: FAIL %s: timed out after %s s\n' "$name" "$per_package" |
            tee -a "$logdir/failures.txt"
        continue
    fi
    first=$(grep -m1 '^error' "$log" || true)
    location=$(awk '/^error/{found=1} found && /--> /{print; exit}' "$log" |
        sed 's/^[[:space:]]*--> //')
    if [ -n "$location" ]; then
        printf 'check-build: FAIL %s: %s (%s)\n' "$name" "$first" "$location" |
            tee -a "$logdir/failures.txt"
    else
        printf 'check-build: FAIL %s: %s\n' "$name" "$first" |
            tee -a "$logdir/failures.txt"
    fi
done <"$logdir/packages.tsv"

printf 'check-build: log %s\n' "$logdir"
awk -F '\t' '
    { total[$1]++; if ($3 == "pass") pass[$1]++ }
    END {
        for (group in total) {
            printf "check-build: group %-10s %d/%d pass\n", group, pass[group] + 0, total[group]
        }
    }
' "$logdir/results.tsv" | sort

if [ "$failed" -eq 0 ]; then
    printf 'check-build: PASS (%d packages)\n' "$total"
    exit 0
fi
printf 'check-build: FAILED (%d/%d packages)\n' "$failed" "$total"
exit 1
