#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Thin launcher for the C74ci release-evidence collector.
#
#   sh scripts/release-evidence.sh                 # collect and write the packet
#   sh scripts/release-evidence.sh --check         # validate the packet; no commands
#   sh scripts/release-evidence.sh --self-test     # fixture refusal tests
#
# The collector lives in tests/release/evidence/collect.py. It never tags and never turns a
# missing, stale or virt/synthetic artifact into a PASS.

set -eu
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
exec python3 "$ROOT/tests/release/evidence/collect.py" "$@"
