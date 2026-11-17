#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# POSIX xargs behaviour runner: runs the self-contained host case script.

set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec sh "$here/xargs/test.sh"
