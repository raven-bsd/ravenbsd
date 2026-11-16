#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P5b mmap-file evidence: the posix-fs qtest drives shared/private file mappings,
# msync, munmap and the past-EOF rejection through the VFS pager.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root"

cargo xtask qtest --case posix-fs --arch x86_64
