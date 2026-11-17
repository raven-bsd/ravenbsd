#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# P5c record-lock evidence: raw RECORD_LOCK conflict, F_GETLK, blocking,
# close-release and exit-release.
#
# Named nextest exception: the lock behaviour is exercised by the raw-syscall
# qtest `posix-fs-record-lock` on the target, not by a host test.

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../../.." && pwd)
cd "$root"

cargo xtask qtest --case posix-fs-record-lock --arch x86_64
