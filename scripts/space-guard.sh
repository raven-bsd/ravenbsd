#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Free-space preflight for the image build. Failing here is cheaper than a half-written image.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
minimum_gib=${1:-10}

available_kib=$(df -Pk "$root" | awk 'NR == 2 { print $4 }')
[ -n "$available_kib" ] || { echo "space: cannot read free space for $root" >&2; exit 2; }

minimum_kib=$((minimum_gib * 1024 * 1024))
if [ "$available_kib" -lt "$minimum_kib" ]; then
    available_gib=$((available_kib / 1024 / 1024))
    echo "space: only ${available_gib} GiB free; need at least ${minimum_gib} GiB" >&2
    echo "space: run 'make prune PRUNE_APPLY=1' (or free space another way) and retry" >&2
    exit 1
fi
