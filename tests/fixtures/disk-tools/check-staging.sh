#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host check for S212: the 0.1.0-ufs fragment stages the disk tools from the
# arch-parameterised fixture output, exactly like the release profile. The
# legacy arch-less directory is the same defect K434 removed from the release
# profile and must not reappear here.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

fragment=profiles/0.1.0-ufs.d/30-disk-tools.toml
tools='lsblk newfs gpart'

for tool in $tools; do
    expected="src = \"build/fixtures/disk-tools/{arch}/tools/$tool\""
    if ! grep -Fq "$expected" "$fragment"; then
        echo "disk-tools-staging: $fragment does not stage $tool from build/fixtures/disk-tools/{arch}/tools/$tool" >&2
        exit 1
    fi
done

if grep -Eq '^src = "build/fixtures/disk-tools/tools/[^"]+"$' "$fragment"; then
    echo "disk-tools-staging: $fragment stages a disk tool without the {arch} segment" >&2
    exit 1
fi

echo "disk-tools-staging: PASS ($fragment)"
