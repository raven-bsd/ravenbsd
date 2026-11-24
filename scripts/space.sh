#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# `make space`: show where the build disk is going. Target and build trees are reported
# separately; the filesystem line is last so the free-space number is always visible.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

echo "RavenBSD disk use"
echo

for tree in target build; do
    if [ -d "$tree" ]; then
        du -sh "$tree" 2>/dev/null || true
    fi
done

echo
echo "target (top entries)"
if [ -d target ]; then
    du -sh target/* 2>/dev/null | sort -h | tail -10 || true
fi

echo
echo "build (top entries)"
if [ -d build ]; then
    du -sh build/* 2>/dev/null | sort -h | tail -10 || true
fi

echo
df -h "$root"
