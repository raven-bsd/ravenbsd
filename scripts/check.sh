#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# usage: scripts/check.sh [--fast]
# Compatibility wrapper: the local gate lives in `cargo xtask check`.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT"
exec cargo run -q -p xtask -- check "$@"
