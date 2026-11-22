#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Fail when a product program reintroduces a write to the frozen docs/ tree or
# the deleted tests/evidence/ tree. Reads are allowed only through the mgmt
# resolver; this check is the write-side guard for AGENTS 31.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

fail=0

if [ -e tests/evidence ]; then
    echo "frozen-writes: tests/evidence exists; product evidence must live under RAVEN_MGMT_ROOT/tickets/evidence" >&2
    fail=1
fi

if git grep -n --untracked -E 'tests/evidence' -- \
    scripts tools tests/release xtask/src \
    ':(exclude)scripts/check-frozen-writes.sh' \
    ':(exclude)*_ut.rs' \
    ':(exclude)tools/*/tests/**' 2>/dev/null; then
    echo "frozen-writes: tests/evidence reference in a product program; write evidence under RAVEN_MGMT_ROOT" >&2
    fail=1
fi

write_pattern='write_text\([^)]*docs/|write_bytes\([^)]*docs/|fs::write\([^)]*docs/|std::fs::write\([^)]*docs/|File::create\([^)]*docs/|create_dir_all\([^)]*docs/|(^|[^#])[[:space:]]*(>|>>)[[:space:]]*[^#[:space:]]*docs/|tee[[:space:]]+[^#]*docs/|(cp|mv|rm|install)[[:space:]]+[^#]*docs/'

if git grep -n --untracked -E "$write_pattern" -- \
    scripts tools tests/release xtask/src \
    ':(exclude)scripts/check-frozen-writes.sh' \
    ':(exclude)*_ut.rs' \
    ':(exclude)tools/*/tests/**' 2>/dev/null; then
    echo "frozen-writes: product program writes under docs/; use RAVEN_MGMT_ROOT" >&2
    fail=1
fi

if [ "$fail" -eq 0 ]; then
    echo "frozen-writes: ok (no tests/evidence tree and no docs/ writes)"
fi
exit "$fail"
