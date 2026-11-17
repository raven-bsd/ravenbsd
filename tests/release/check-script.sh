#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host checks for scripts/release-verify.sh that do not clone or build.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

sh -n scripts/release-verify.sh

if ! sh scripts/release-verify.sh --help >/dev/null 2>&1; then
    echo "release-verify-test: --help failed" >&2
    exit 1
fi

if sh scripts/release-verify.sh "$root" >/dev/null 2>&1; then
    echo "release-verify-test: accepted a non-/tmp scratch directory" >&2
    exit 1
fi

if sh scripts/release-verify.sh /tmp/opencode/release-verify-test-invalid-extra \
    extra >/dev/null 2>&1; then
    echo "release-verify-test: accepted an extra argument" >&2
    exit 1
fi

echo "release-verify-test: PASS"
