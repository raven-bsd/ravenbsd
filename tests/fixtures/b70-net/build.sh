#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Build/preflight step for the B70 cases. The profile's packages are real workspace packages
# owned by B12b/B12c/B14/B15; this only checks the dependency seams and prepares the host
# artifact directory.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

sh tests/fixtures/b70-net/preflight.sh
mkdir -p build/fixtures/b70-net
