#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Guard for scripts/smp-aarch64.sh: every run_case <case> <arch> must name a qtest case whose
# toml declares that same arch. The matrix used to force the x86_64-only libc-threads case onto
# aarch64, which fails while the image is built (foreign e_machine) instead of exercising the
# cross-arch libc row. This check is static and does not run QEMU.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
script=$root/scripts/smp-aarch64.sh
cases=$root/tests/qtest/cases
failed=0
seen=0

if [ ! -f "$script" ]; then
    echo "smp-aarch64-case-arch: missing $script" >&2
    exit 2
fi

while read -r case_name arch; do
    [ -n "$case_name" ] || continue
    seen=$((seen + 1))
    toml=$cases/$case_name.toml
    if [ ! -f "$toml" ]; then
        echo "smp-aarch64-case-arch: $case_name: missing $toml" >&2
        failed=1
        continue
    fi
    declared=$(sed -n 's/^arch = "\([^"]*\)"$/\1/p' "$toml" | head -n 1)
    if [ -z "$declared" ]; then
        echo "smp-aarch64-case-arch: $case_name: no arch in $toml" >&2
        failed=1
        continue
    fi
    if [ "$declared" != "$arch" ]; then
        echo "smp-aarch64-case-arch: $case_name: matrix requests $arch, case declares $declared" >&2
        failed=1
    fi
done <<EOF
$(sed -n 's/^[[:space:]]*run_case[[:space:]]\{1,\}\([^[:space:]]\{1,\}\)[[:space:]]\{1,\}\([^[:space:]]\{1,\}\).*/\1 \2/p' "$script")
EOF

if [ "$seen" -eq 0 ]; then
    echo "smp-aarch64-case-arch: no run_case entries found in $script" >&2
    exit 2
fi
if [ "$failed" -ne 0 ]; then
    exit 1
fi
echo "smp-aarch64-case-arch: ok ($seen cases)"
