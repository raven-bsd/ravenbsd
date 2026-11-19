#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K185 second update: reuse the same root disk, replace the first file, keep the
# boot ESP for the --no-build persist boot and prove the ESP hash is unchanged.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd -P)
cd "$root"

out=build/qtest
target=$out/ufs-update-root.img
esp=$out/ufs-update.img
persist_esp=$out/ufs-update-persist.img
payload=$out/ufs-update-second.txt

fail() {
    echo "ufs-update: $*" >&2
    exit 1
}

[ -f "$target" ] || fail "missing root disk $target; run ufs-update first"
[ -f "$esp" ] || fail "missing ESP $esp; run ufs-update first"
cp "$esp" "$persist_esp" || fail "cannot stage the persist ESP"
printf 'k185-second\n' > "$payload" || fail "cannot create the second payload"

esp_before=$(sha256sum "$esp" | awk '{print $1}')
scripts/raven-ufs check "$target" || fail "pre-second UFS check failed"
scripts/raven-ufs put "$payload" /etc/ufs-update.txt "$target" ||
    fail "second raven-ufs put failed"
scripts/raven-ufs check "$target" || fail "post-second UFS check failed"
esp_after=$(sha256sum "$esp" | awk '{print $1}')

[ "$esp_before" = "$esp_after" ] || fail "boot ESP changed"
echo "ufs-update: second put /etc/ufs-update.txt -> $target"
echo "ufs-update: ESP sha256 $esp_before (unchanged)"
