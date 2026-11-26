#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S85j: resolve the release artifact manifest for one architecture, verify every
# staged program and add-on has a licence and a provenance notice, and scan the
# scaffold for C/C++ implementation files.
#
#   sh tests/release/artifact-audit/run.sh --arch x86_64
#   sh tests/release/artifact-audit/run.sh --arch aarch64
#
# Manifest, hashes and failures are written below
# tickets/evidence/artifacts/S85j/ in RAVEN_MGMT_ROOT (default: the
# ravenbsd-prjmgmt checkout beside this product checkout).

set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
exec python3 "$root/tests/release/artifact-audit/audit.py" "$@"
