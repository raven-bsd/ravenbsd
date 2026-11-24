#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K400 pre-tag workspace test-build gate. K395's `run.sh` is the authoritative per-package sweep;
# this wrapper consumes its results and the K395 audit classification:
#   - 71 K401 host-build exclusions (target-none/bare fixtures and kernel)
#   - 4 owned failures: K394 fsd-ufs, K402 blockd/ramfsd, K403 pi4-platd
# Any other failing package is unowned and fails the gate. No failure is silently skipped.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
cd "$root"

out=${WORKSPACE_BUILD_OUT:-build/release/workspace-build}
rm -rf "$out"
mkdir -p "$out"

CARGO_NET_OFFLINE=true WORKSPACE_BUILD_OUT="$out" sh tests/release/workspace-build/run.sh

python3 - "$out/results.tsv" "$out/failures" tests/release/workspace-build/exclusions.tsv <<'PY'
import os
import sys

results_path, failures_dir, exclusions_path = sys.argv[1:4]

excluded = {}
with open(exclusions_path, encoding="utf-8") as handle:
    for line in handle:
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        package, owner = line.split("\t")
        excluded[package] = owner


def first_location(package):
    path = os.path.join(failures_dir, package.replace("/", "_").replace(":", "_") + ".log")
    if not os.path.isfile(path):
        return ""
    seen = False
    with open(path, encoding="utf-8", errors="replace") as handle:
        for line in handle:
            if line.startswith("error"):
                seen = True
            if seen and "--> " in line:
                return line.split("--> ", 1)[1].strip()
    return ""


total = 0
passed = 0
host_excluded = 0
owned = []
unowned = []
with open(results_path, encoding="utf-8") as handle:
    header = handle.readline()
    if not header.startswith("package\t"):
        print("check-build: results.tsv has no header", file=sys.stderr)
        sys.exit(1)
    for line in handle:
        fields = line.rstrip("\n").split("\t")
        if len(fields) < 2:
            continue
        package, status = fields[0], fields[1]
        total += 1
        if status == "pass":
            passed += 1
            continue
        owner = excluded.get(package)
        if owner == "K401":
            host_excluded += 1
        elif owner in {"K394", "K402", "K403"}:
            owned.append((package, owner, fields[3] if len(fields) > 3 else "", first_location(package)))
        else:
            unowned.append((package, fields[3] if len(fields) > 3 else "", first_location(package)))

print(f"check-build: log {os.path.dirname(results_path)}")
print(
    f"check-build: {total} packages: {passed} pass, {host_excluded} host-build exclusions (K401), "
    f"{len(owned)} owned (K394/K402/K403), {len(unowned)} unowned"
)
for package, owner, first, location in owned:
    suffix = f" ({location})" if location else ""
    print(f"check-build: OWNED {package} ({owner}): {first}{suffix}")
for package, first, location in unowned:
    suffix = f" ({location})" if location else ""
    print(f"check-build: FAIL {package}: {first}{suffix}")

if unowned:
    print(f"check-build: FAILED ({len(unowned)} unowned compile failure(s))")
    sys.exit(1)
print("check-build: PASS (only K395-classified exclusions/owned failures)")
PY
