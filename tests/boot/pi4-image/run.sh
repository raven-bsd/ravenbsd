#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Pi 4 0.1.0 SD image build and manifest gate.
#
# Usage: sh tests/boot/pi4-image/run.sh --arch aarch64 --profile 0.1.0-ufs
#        sh tests/boot/pi4-image/run.sh --arch aarch64 --profile 0.1.0-ufs --mutation missing-boot-file
#
# The optional --image PATH verifies an existing image instead of rebuilding it.
# --evidence-dir PATH overrides tickets/evidence/artifacts/S85o in the
# management checkout (RAVEN_MGMT_ROOT overrides its location).

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)
cd "$root"
export PYTHONDONTWRITEBYTECODE=1
exec python3 "$root/tests/boot/pi4-image/run.py" "$@"
