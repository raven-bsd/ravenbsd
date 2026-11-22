#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K492/K546 boot-loop matrix entry point. The Python runner owns the matrix and
# reuses the existing `tests/smp/boot/soak_boot.py` foreground QEMU supervisor.
# Usage: sh tests/soak/run.sh --mode tcg --boots 20
#        sh tests/soak/run.sh --mode tcg --boots 10 --arch aarch64

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
cd "$root"
export PYTHONDONTWRITEBYTECODE=1
for arg in "$@"; do
    case "$arg" in
        --hours|--hours=*|--resume|--resume=*|--prove-hung-boot|--prove-stall)
            exec python3 "$root/tests/soak/soak.py" "$@"
            ;;
    esac
done
exec python3 "$root/tests/soak/run.py" "$@"
