#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P5 sparse-file evidence: growing a UFS file leaves holes and st_blocks stays small.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

cargo nextest run -p fsd-ufs \
    -E 'test(sparse_extension_keeps_block_accounting_small)'
