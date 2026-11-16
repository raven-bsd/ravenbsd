#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P5 umask evidence: the process-local mask is applied to mkdir and O_CREAT.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

cargo nextest run -p raven-libc-platform \
    -E 'test(platform_umask_masks_mkdir_and_create)'
