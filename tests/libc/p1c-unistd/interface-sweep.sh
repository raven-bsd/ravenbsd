#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Committed interface sweep for both gate architectures. Runs the full present-interface
# declaration sweep from the posix-conform tool; documented extension rows are reported as
# EXTENSION, any other failure is a required-declaration regression. Pass --strict to treat the
# extension list as failures too (for P1f7 to flip once the waiver list is gone).

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

cargo run -q -p posix-conform -- sweep --arch x86_64 "$@"
AARCH64_CC=${AARCH64_CC:-aarch64-linux-gnu-gcc} \
    cargo run -q -p posix-conform -- sweep --arch aarch64 "$@"
