#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P5g relative symlink readlink evidence: a Readlink must traverse the parent
# directories and must not follow the final symlink, whether the path is
# relative or absolute.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo nextest run -p fsd-ufs -E 'test(readlink)'
