#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Isolate the 4 GiB console-input NUL bytes on the S157 smp-4g image.
#
#   sh scripts/console-input-4g.sh --size 512M
#   sh scripts/console-input-4g.sh --size 4G
#   sh scripts/console-input-4g.sh --size 4G --shell rsh
#
# The script boots the existing image read-only (`-snapshot`), logs in, sends
# `echo hi`, `pwd` and `uname -a` with the selected spacing, and keeps the raw
# serial bytes plus a hexdump of each response.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$root"
exec python3 tests/fixtures/console-input-4g/measure.py "$@"
