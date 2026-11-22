#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Static checks for the 0.1.0 release artifacts. This does not build an image; the release gate
# needs the REL2b/REL3/REL4 evidence first.

set -eu
ROOT=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
. "$ROOT/scripts/lib/mgmt.sh"
notes=$(mgmt_require_file architecture/product-data/release-0.1.0.md) || exit 1

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
grep -q '^# RavenBSD 0.1.0 release notes$' "$notes"
grep -q 'QEMU transcript' "$notes"
grep -q 'Pi 4 gate' "$notes"
grep -q 'rsh' "$notes"
grep -q 'P9a' "$notes"
grep -q 'P8/P8b/P10/P12/REL3c' "$notes"
grep -q 'REL7b' "$notes"
grep -q 'self-contained' "$notes"
grep -q 'release-verify' "$notes"
if grep -q 'RAVEN_USER_RELEASE' "$notes"; then
    echo 'release checks: release notes still require RAVEN_USER_RELEASE' >&2
    exit 1
fi
grep -q 'Default password warning' "$notes"
grep -q 'make check' "$notes"
grep -q 'release-evidence' "$notes"
grep -Eq '^Status: \*\*BLOCKED' "$notes"
[ -f PROVENANCE.toml ]

printf 'release checks: ok\n'
