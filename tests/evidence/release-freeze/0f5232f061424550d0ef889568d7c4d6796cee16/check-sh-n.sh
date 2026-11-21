#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K384: `sh -n` the shell scripts cited by the freeze checklist. No command is executed.

set -u
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root" || exit 2

scripts='
scripts/qemu-kvm.sh
scripts/boot-time.sh
scripts/login-cold-start.sh
scripts/qemu-net.sh
scripts/ufs-demo.sh
scripts/pi4-install.sh
scripts/pi4-release.sh
scripts/pi4-update.sh
scripts/pi4-hammer.sh
scripts/board-pi4-check.sh
scripts/fault-injection.sh
tests/scripts/check-soak-24h.sh
scripts/release-evidence.sh
'

fail=0
for path in $scripts; do
    if sh -n "$path" 2>/dev/null; then
        printf 'sh-n ok %s\n' "$path"
    else
        printf 'sh-n FAIL %s\n' "$path"
        fail=$((fail + 1))
    fi
done
printf 'summary checked=13 fail=%s\n' "$fail"
exit "$fail"
