#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# EVID6 host evidence check: no QEMU, no product test. It verifies the durable host artifacts and
# that the release packet carries the assembled host rows. A missing row or a changed artifact
# fails the check.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)
cd "$root"

sha256sum -c tests/evidence/release-host/SHA256SUMS >/dev/null
sha256sum -c tests/evidence/release-smp/SHA256SUMS >/dev/null
sha256sum -c tests/evidence/tcp/soak/SHA256SUMS >/dev/null

packet=docs/releases/0.1.0-evidence.md
grep -Fq '| shell-hygiene |' "$packet"
grep -Fq '| tcp-soak |' "$packet"
grep -Fq '| rack-audit |' "$packet"
grep -Fq '| pf-conformance |' "$packet"
grep -Fq '| pipe-poll |' "$packet"
grep -Fq 'tests/evidence/release-host/cold-start-boot/README.md' "$packet"
grep -Fq 'tests/evidence/release-host/image-pins.txt' "$packet"
grep -Fq 'HARDWARE/UNVERIFIED' "$packet"

readme=$(sha256sum tests/evidence/release-host/README.md | cut -d ' ' -f 1)
grep -Fq "tests/evidence/release-host/README.md\` sha256=\`$readme\`" "$packet"

printf 'release-host: PASS (%s artifacts, host rows present)\n' \
    "$(wc -l < tests/evidence/release-host/SHA256SUMS)"
