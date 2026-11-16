#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../../.." && pwd)
cd "$root"
cargo nextest run -p raven-strings
printf 'ok: raven-strings host tests\n'
