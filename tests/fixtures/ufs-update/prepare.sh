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

fail() {
    echo "ufs-update: $*" >&2
    exit 1
}

if [ ! -f "$baseline_disk" ] || [ ! -f "$baseline_esp" ]; then
    cargo xtask image --arch x86_64 --profile qemu-ufs --output "$baseline_esp" ||
        fail "cannot build the baseline qemu-ufs image"
fi
[ -f "$baseline_disk" ] || fail "missing baseline root disk $baseline_disk"
[ -f "$baseline_esp" ] || fail "missing baseline ESP $baseline_esp"

cp "$baseline_disk" "$target" || fail "cannot copy the baseline root disk"
printf 'k185-first\n' > "$payload" || fail "cannot create the first payload"

esp_before=$(sha256sum "$baseline_esp" | awk '{print $1}')
scripts/raven-ufs check "$target" || fail "baseline UFS check failed"
scripts/raven-ufs put "$payload" /etc/ufs-update.txt "$target" ||
    fail "first raven-ufs put failed"
scripts/raven-ufs check "$target" || fail "post-update UFS check failed"
esp_after=$(sha256sum "$baseline_esp" | awk '{print $1}')

[ "$esp_before" = "$esp_after" ] || fail "baseline ESP changed"
echo "ufs-update: first put /etc/ufs-update.txt -> $target"
echo "ufs-update: ESP sha256 $esp_before (unchanged)"
