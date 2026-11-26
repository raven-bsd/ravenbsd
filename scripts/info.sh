#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# `make info`: code statistics for the RavenBSD tree. The Rust summary is printed first; the
# full tokei table follows so the C add-ons, shell and donor material stay visible.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

if ! command -v tokei >/dev/null 2>&1; then
    echo "info: tokei not found; install it with 'cargo install tokei' or the host package manager" >&2
    exit 1
fi

excludes="--exclude target --exclude .git --exclude build --exclude vendor"

echo "RavenBSD code statistics"
echo

if command -v python3 >/dev/null 2>&1; then
    tokei . $excludes --output json | python3 -c '
import json
import sys

data = json.load(sys.stdin)
reports = [row for row in data.get("Rust", {}).get("reports", [])
           if row["name"].endswith(".rs")]
total = sum(row["stats"]["code"] for row in reports)
vendored = sum(row["stats"]["code"] for row in reports
               if row["name"].lstrip("./").startswith(("userland/uutils/", "userland/brush/")))
print("  Rust files:          %d" % len(reports))
print("  Rust code lines:     %d" % total)
print("  vendored uutils/brush: %d" % vendored)
print("  first-party Rust:    %d" % (total - vendored))
'
    echo
fi

tokei . $excludes
