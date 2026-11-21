#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K384: cheap path presence check for the freeze checklist. It does not run any gate.

set -u
root=$(CDPATH= cd -- "$(dirname "$0")/../../../.." && pwd)
cd "$root" || exit 2

paths='
scripts/qemu-kvm.sh
scripts/boot-time.sh
scripts/login-cold-start.sh
scripts/qemu-net.sh
scripts/ufs-demo.sh
scripts/pi4-install.sh
scripts/pi4-release.sh
scripts/pi4-eink-evidence.sh
scripts/pi4-update.sh
scripts/pi4-hammer.sh
scripts/board-pi4-check.sh
scripts/fault-injection.sh
tests/scripts/check-soak-24h.sh
scripts/release-evidence.sh
docs/releases/0.1.0-evidence.md
docs/kernel-gate.md
docs/posix/CONFORMANCE.md
tests/libc/CONFORMANCE.md
docs/release-images.md
docs/releases/0.1.0-notes.md
docs/releases/0.1.0-crosscheck.md
tests/libc-oracle/g1.toml
tests/qtest/cases/brush-login.toml
tests/qtest/cases/login-latency.toml
tests/qtest/cases/disk-tools.toml
tests/qtest/cases/login-retry.toml
tests/qtest/cases/pi4-usb.toml
tests/qtest/cases/hdmi-console.toml
tests/qtest/cases/release-login.toml
tests/qtest/cases/posix-shell.toml
tests/qtest/cases/libc-spawn.toml
tests/qtest/cases/dynamic-userland.toml
tests/qtest/cases/dynamic-exec-aarch64.toml
'

present=0
missing=0
for path in $paths; do
    if [ -e "$path" ]; then
        printf 'present %s\n' "$path"
        present=$((present + 1))
    else
        printf 'missing %s\n' "$path"
        missing=$((missing + 1))
    fi
done
printf 'summary present=%s missing=%s\n' "$present" "$missing"
