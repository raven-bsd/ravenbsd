#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S85h 0.1.0 SMP soak entry point.
# Usage: sh tests/soak/smp-0.1.0/run.sh --arch x86_64 --accel kvm --smp 4 --cycles 100
#        sh tests/soak/smp-0.1.0/run.sh --arch aarch64 --accel tcg --smp 4 --cycles 100

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"
export PYTHONDONTWRITEBYTECODE=1
exec python3 "$root/tests/soak/smp-0.1.0/run.py" "$@"
