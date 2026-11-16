#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Evidence entry point: run the complete bounded-m4 behaviour suite.

set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec sh "$here/../run-m4.sh" "$@"
