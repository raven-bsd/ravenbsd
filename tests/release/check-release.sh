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
sh -n scripts/release-evidence.sh
sh -n scripts/release-verify.sh
scripts/release.sh --help >/dev/null
scripts/release-evidence.sh --help >/dev/null
scripts/release-verify.sh --help >/dev/null
if ! output=$(scripts/release-evidence.sh --check 2>&1); then
    printf '%s\n' "$output" | grep -q '^release-evidence: blocked: missing' || {
        printf 'release checks: unexpected evidence failure\n%s\n' "$output" >&2
        exit 1
    }
fi
grep -q '^# RavenBSD 0.1.0 release notes$' docs/releases/0.1.0.md
grep -q 'QEMU transcript' docs/releases/0.1.0.md
grep -q 'Pi 4 gate' docs/releases/0.1.0.md
grep -q 'rsh' docs/releases/0.1.0.md
grep -q 'P9a' docs/releases/0.1.0.md
grep -q 'REL3/P8/P10/P12/REL4d' docs/releases/0.1.0.md
grep -q 'release-verify' docs/releases/0.1.0.md
grep -q 'Default password warning' docs/releases/0.1.0.md
grep -q 'make check' docs/releases/0.1.0.md
grep -q 'release-evidence' docs/releases/0.1.0.md
grep -Eq '^Status: \*\*BLOCKED' docs/releases/0.1.0.md
[ -f PROVENANCE.toml ]

printf 'release checks: ok\n'
