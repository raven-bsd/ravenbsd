#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Measure the release login cold start over QEMU serial.
#
#   sh scripts/login-cold-start.sh --accel kvm --smp 2 --samples 5
#   sh scripts/login-cold-start.sh --accel tcg --smp 4 --samples 5 --check
#   sh scripts/login-cold-start.sh --baseline build/login-cold-start/baseline.json
#
# One sample times `login:` -> `Password:` after `root` and `Password:` -> `raven# `
# after `raven`. The table and summary are printed; --check fails above 1000 ms.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"
exec python3 tests/fixtures/login-cold-start/measure.py "$@"
