#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K395 workspace test-build audit. Builds every workspace package's test targets with
# `cargo test --no-run` (no test execution) and writes a per-package result TSV plus the
# failure logs under tests/release/workspace-build/<sha>/.
#
# usage: sh tests/release/workspace-build/run.sh
# The caller supplies the overall foreground timeout; each package gets a 120 s cap so a
# single hang cannot consume the whole audit budget.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"
sha=$(git rev-parse HEAD)
out=${WORKSPACE_BUILD_OUT:-"$root/tests/release/workspace-build/$sha"}
mkdir -p "$out/failures"

python3 - "$out" <<'PY'
import json
import os
import subprocess
import sys
import time

out = sys.argv[1]
metadata = subprocess.run(
    ["cargo", "metadata", "--no-deps", "--format-version", "1"],
    capture_output=True,
    text=True,
    check=True,
)
packages = sorted({package["name"] for package in json.loads(metadata.stdout)["packages"]})
results_path = os.path.join(out, "results.tsv")

with open(results_path, "w") as results:
    results.write("package\tstatus\tseconds\tfirst_error\n")
    for index, package in enumerate(packages, 1):
        started = time.time()
        built = subprocess.run(
            ["timeout", "120", "cargo", "test", "--no-run", "-p", package],
            capture_output=True,
            text=True,
        )
        seconds = time.time() - started
        output = built.stdout + "\n" + built.stderr
        if built.returncode == 0:
            status = "pass"
            first = ""
        elif built.returncode == 124:
            status = "timeout"
            first = "timeout 120s"
        else:
            status = "fail"
            first = ""
            for line in output.splitlines():
                if line.startswith("error[") or line.startswith("error:"):
                    first = line.strip()
                    break
            safe = package.replace("/", "_").replace(":", "_")
            with open(os.path.join(out, "failures", safe + ".log"), "w") as log:
                log.write(output)
        results.write(f"{package}\t{status}\t{seconds:.2f}\t{first}\n")
        results.flush()
        if index % 25 == 0:
            print(f"workspace-build: {index}/{len(packages)} packages", file=sys.stderr, flush=True)
PY

printf 'workspace-build: results %s\n' "$out/results.tsv"
