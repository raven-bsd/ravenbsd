#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# POSIX behaviour evidence for `newgrp`: argument parsing, group lookup,
# membership and the shell plan.

set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/../../../../.." && pwd)
cd "$root"

cargo nextest run -p newgrp
printf 'ok: newgrp host tests\n'
