#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K185 first update: copy the assembled qemu-ufs root disk, apply a bounded
# /etc update with raven-ufs, and prove the baseline ESP hash is unchanged.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
cd "$root"

out=build/qtest
baseline_esp=$out/ufs-update-baseline.img
baseline_disk=build/x86_64/qemu-ufs-root.img
target=$out/ufs-update-root.img
payload=$out/ufs-update-first.txt
log=$out/ufs-update-prepare.log

fail() {
    echo "ufs-update: $*" >&2
    exit 1
}

note() {
    echo "$*"
    echo "$*" >> "$log"
}

: > "$log"

if [ ! -f "$baseline_disk" ] || [ ! -f "$baseline_esp" ]; then
    cargo xtask image --arch x86_64 --profile qemu-ufs --output "$baseline_esp" ||
        fail "cannot build the baseline qemu-ufs image"
fi
[ -f "$baseline_disk" ] || fail "missing baseline root disk $baseline_disk"
[ -f "$baseline_esp" ] || fail "missing baseline ESP $baseline_esp"

# A prior guest boot in the same acceptance run can leave the rw root dirty.
scripts/raven-ufs repair "$baseline_disk" >> "$log" 2>&1 ||
    fail "cannot preen the baseline root disk"
cp "$baseline_disk" "$target" || fail "cannot copy the baseline root disk"
printf 'k185-first\n' > "$payload" || fail "cannot create the first payload"

esp_before=$(sha256sum "$baseline_esp" | awk '{print $1}')
scripts/raven-ufs check "$target" >> "$log" 2>&1 || fail "baseline UFS check failed"
scripts/raven-ufs put "$payload" /etc/ufs-update.txt "$target" >> "$log" 2>&1 ||
    fail "first raven-ufs put failed"
scripts/raven-ufs check "$target" >> "$log" 2>&1 || fail "post-update UFS check failed"
esp_after=$(sha256sum "$baseline_esp" | awk '{print $1}')

[ "$esp_before" = "$esp_after" ] || fail "baseline ESP changed"
note "ufs-update: first put /etc/ufs-update.txt -> $target"
note "ufs-update: ESP sha256 $esp_before (unchanged)"
