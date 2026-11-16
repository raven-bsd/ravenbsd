#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Static checks for the 0.1.0 release artifacts. This does not build an image; the release gate
# needs the REL2b/REL3/REL4 evidence first.

set -eu
ROOT=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$ROOT"

sh -n scripts/release.sh
scripts/release.sh --help >/dev/null
grep -q '^# RavenBSD 0.1.0 release notes$' docs/releases/0.1.0.md
grep -q 'QEMU transcript' docs/releases/0.1.0.md
grep -q 'Pi 4 transcript' docs/releases/0.1.0.md
grep -q 'Default password warning' docs/releases/0.1.0.md
grep -q 'make check' docs/releases/0.1.0.md
grep -Eq '^Status: \*\*BLOCKED' docs/releases/0.1.0.md
[ -f PROVENANCE.toml ]

printf 'release checks: ok\n'
