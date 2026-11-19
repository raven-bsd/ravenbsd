#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Convenience wrapper for the host raven-flash TUI. The tool itself is
# tools/raven-flash; this script only forwards arguments from the repo root.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"
exec cargo run -q -p raven-flash -- "$@"
