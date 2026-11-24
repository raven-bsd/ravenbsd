#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S173 acceptance: run the self-contained polling conformance case.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"
exec cargo xtask qtest --case tests/conformance/polling/polling.toml --arch x86_64
