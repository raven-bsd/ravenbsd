#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# POSIX behaviour evidence for first-party `uucp`.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../../.." && pwd)
cd "$root"
cargo nextest run -p uucp
printf 'ok: uucp local copy, queue, config and job codec host tests\n'
