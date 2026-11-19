#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Host selftest for scripts/raven-ufs. Builds a Pi 4-shaped GPT image (ESP + UFS2),
# updates the UFS slice in place, and checks the ESP bytes, fsck, metadata and the
# safety refusals. No board access and no whole-image copy.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

work=$(mktemp -d "${TMPDIR:-/tmp}/raven-ufs-selftest.XXXXXX")
cleanup() {
    rm -rf "$work"
}
trap cleanup EXIT INT TERM

fail() {
    echo "raven-ufs-selftest: $*" >&2
    exit 1
}

cargo build -q -p ufs-inject -p ufs-mkfs || fail "cannot build ufs-inject/ufs-mkfs"
inject=${RAVEN_UFS_INJECT:-$root/target/debug/ufs-inject}
mkfs=${RAVEN_UFS_MKFS:-$root/target/debug/ufs-mkfs}
raven=$root/scripts/raven-ufs
[ -x "$inject" ] || fail "missing $inject"
[ -x "$mkfs" ] || fail "missing $mkfs"
[ -x "$raven" ] || fail "missing $raven"

"$mkfs" -O 2 -s 8m -T 1700000000 "$work/root.ufs" || fail "mkfs failed"

python3 - "$work/root.ufs" "$work/pi4.img" <<'PY'
import struct
import sys
import uuid
import zlib

ufs_path, image_path = sys.argv[1], sys.argv[2]
ufs = open(ufs_path, "rb").read()
if len(ufs) % 512:
    raise SystemExit("UFS payload is not sector aligned")
esp_start = 2048
esp_sectors = 2048
ufs_start = esp_start + esp_sectors
ufs_sectors = len(ufs) // 512
total = ufs_start + ufs_sectors + 33
size = total * 512
disk_guid = uuid.UUID("11111111-1111-1111-1111-111111111111").bytes
esp_guid = uuid.UUID("22222222-2222-2222-2222-222222222222").bytes
ufs_guid = uuid.UUID("33333333-3333-3333-3333-333333333333").bytes

def header(current, backup, entries_lba):
    data = bytearray(92)
    data[0:8] = b"EFI PART"
    struct.pack_into("<I", data, 8, 0x00010000)
    struct.pack_into("<I", data, 12, 92)
    struct.pack_into("<I", data, 16, 0)
    struct.pack_into("<QQ", data, 24, current, backup)
    struct.pack_into("<QQ", data, 40, 34, total - 34)
    data[56:72] = disk_guid
    struct.pack_into("<Q", data, 72, entries_lba)
    struct.pack_into("<I", data, 80, 128)
    struct.pack_into("<I", data, 84, 128)
    struct.pack_into("<I", data, 88, zlib.crc32(entries) & 0xFFFFFFFF)
    struct.pack_into("<I", data, 16, zlib.crc32(data) & 0xFFFFFFFF)
    return data

entries = bytearray(128 * 128)
esp_type = bytes.fromhex("28732ac11ff8d211ba4b00a0c93ec93b")
ufs_type = bytes.fromhex("b67c6e51cf6ed6118ff800022d09712b")
struct.pack_into("<16s16sQQQ", entries, 0, esp_type, esp_guid,
                 esp_start, esp_start + esp_sectors - 1, 0)
entries[56:56 + 6] = "ESP".encode("utf-16le")
struct.pack_into("<16s16sQQQ", entries, 128, ufs_type, ufs_guid,
                 ufs_start, ufs_start + ufs_sectors - 1, 0)
entries[128 + 56:128 + 56 + 16] = "ufs-root".encode("utf-16le")

disk = bytearray(size)
disk[510:512] = b"\x55\xaa"
disk[446] = 0
disk[447:450] = b"\x00\x02\x00"
disk[450] = 0xEE
disk[451:454] = b"\xff\xff\xff"
struct.pack_into("<I", disk, 454, 1)
struct.pack_into("<I", disk, 458, min(total - 1, 0xFFFFFFFF))
disk[512:512 + 92] = header(1, total - 1, 2)
disk[1024:1024 + len(entries)] = entries
disk[esp_start * 512:(esp_start + esp_sectors) * 512] = b"\xa5" * (esp_sectors * 512)
disk[ufs_start * 512:(ufs_start + ufs_sectors) * 512] = ufs
disk[(total - 33) * 512:(total - 33) * 512 + len(entries)] = entries
disk[(total - 1) * 512:(total - 1) * 512 + 92] = header(total - 1, 1, total - 33)
open(image_path, "wb").write(disk)
PY

esp_before=$work/esp.before
esp_after=$work/esp.after
dd if="$work/pi4.img" bs=512 skip=2048 count=2048 of="$esp_before" status=none ||
    fail "cannot capture the ESP bytes"
sha_before=$(sha256sum "$esp_before" | awk '{print $1}')
inode_before=$(stat -c '%i' "$work/pi4.img")
size_before=$(stat -c '%s' "$work/pi4.img")

"$raven" check "$work/pi4.img" || fail "initial check failed"
"$raven" ls "$work/pi4.img" / | grep -qx 'lost+found' ||
    fail "fresh root does not list lost+found"

mkdir -p "$work/tree/etc" "$work/tree/opt/app/bin"
printf 'selftest motd\n' > "$work/tree/etc/motd"
printf 'binary payload\n' > "$work/tree/opt/app/bin/tool"
chmod 0640 "$work/tree/etc/motd"
chmod 0750 "$work/tree/opt/app/bin/tool"
ln -s /opt/update/etc/motd "$work/tree/opt/app/bin/motd"
touch -d @1700000123 "$work/tree/etc/motd" "$work/tree/opt/app/bin/tool"
touch -h -d @1700000123 "$work/tree/opt/app/bin/motd"

"$raven" sync "$work/tree" /opt/update "$work/pi4.img" || fail "sync failed"
"$raven" ls "$work/pi4.img" /opt/update/etc | grep -qx motd ||
    fail "sync did not create /opt/update/etc/motd"
"$raven" ls "$work/pi4.img" /opt/update/opt/app/bin | grep -qx tool ||
    fail "sync did not create the tool"
"$raven" ls "$work/pi4.img" /opt/update/opt/app/bin | grep -qx motd ||
    fail "sync did not create the symlink"

ufs_off=$((4096 * 512))
ufs_len=$(stat -c '%s' "$work/root.ufs")
host_uid=$(stat -c '%u' "$work/tree/etc/motd")
host_gid=$(stat -c '%g' "$work/tree/etc/motd")
"$inject" --offset "$ufs_off" --size "$ufs_len" "$work/pi4.img" cat \
    /opt/update/etc/motd | grep -qx 'selftest motd' ||
    fail "synced file is not readable"
"$inject" --offset "$ufs_off" --size "$ufs_len" "$work/pi4.img" cat \
    /opt/update/opt/app/bin/motd | grep -qx 'selftest motd' ||
    fail "synced symlink does not resolve"
"$inject" --offset "$ufs_off" --size "$ufs_len" "$work/pi4.img" stat \
    /opt/update/etc/motd | grep -q "mode=0o640 uid=$host_uid gid=$host_gid" ||
    fail "sync did not preserve mode/uid/gid"
"$inject" --offset "$ufs_off" --size "$ufs_len" "$work/pi4.img" stat \
    /opt/update/etc/motd | grep -q 'mtime=1700000123' ||
    fail "sync did not preserve mtime"
"$inject" --offset "$ufs_off" --size "$ufs_len" "$work/pi4.img" stat \
    /opt/update/opt/app/bin/tool | grep -q 'mode=0o750' ||
    fail "sync did not preserve the executable mode"
"$inject" --offset "$ufs_off" --size "$ufs_len" "$work/pi4.img" stat \
    --no-follow /opt/update/opt/app/bin/motd | grep -q '^symlink ' ||
    fail "sync did not preserve the symlink"

printf 'put payload\n' > "$work/put.txt"
"$raven" put "$work/put.txt" /opt/update/put.txt "$work/pi4.img" ||
    fail "put failed"
"$raven" ls "$work/pi4.img" /opt/update | grep -qx put.txt ||
    fail "put did not create the file"
"$raven" rm /opt/update/put.txt "$work/pi4.img" || fail "rm failed"
if "$raven" ls "$work/pi4.img" /opt/update | grep -qx put.txt; then
    fail "rm left the file behind"
fi
"$raven" mkdir /opt/update/empty "$work/pi4.img" || fail "mkdir failed"
"$raven" ls "$work/pi4.img" /opt/update | grep -qx empty || fail "mkdir missing"

rm "$work/tree/opt/app/bin/tool"
"$raven" --delete sync "$work/tree" /opt/update "$work/pi4.img" ||
    fail "sync --delete failed"
if "$raven" ls "$work/pi4.img" /opt/update/opt/app/bin | grep -qx tool; then
    fail "sync --delete left the removed file"
fi
if "$raven" ls "$work/pi4.img" /opt/update | grep -qx empty; then
    fail "sync --delete left an unexpected directory"
fi

"$raven" check "$work/pi4.img" || fail "final check failed"

dd if="$work/pi4.img" bs=512 skip=2048 count=2048 of="$esp_after" status=none ||
    fail "cannot capture the ESP bytes after the update"
cmp -s "$esp_before" "$esp_after" || fail "ESP/FAT bytes changed"
sha_after=$(sha256sum "$esp_after" | awk '{print $1}')
[ "$sha_before" = "$sha_after" ] || fail "ESP hash changed"
inode_after=$(stat -c '%i' "$work/pi4.img")
size_after=$(stat -c '%s' "$work/pi4.img")
[ "$inode_before" = "$inode_after" ] || fail "image was replaced, not updated in place"
[ "$size_before" = "$size_after" ] || fail "image size changed"

cp "$work/pi4.img" "$work/bad.img"
python3 - "$work/bad.img" <<'PY'
import sys
path = sys.argv[1]
with open(path, "r+b") as image:
    image.seek(512 + 16)
    image.write(b"\0\0\0\0")
PY
if "$raven" check "$work/bad.img" >"$work/bad.log" 2>&1; then
    fail "corrupt GPT was accepted"
fi
echo "raven-ufs-selftest: corrupt-GPT refusal PASS"

loop=$(losetup -f 2>/dev/null || true)
if [ -n "$loop" ] && [ -b "$loop" ]; then
    if "$raven" put "$work/put.txt" /probe "$loop" >"$work/unconfirmed.log" 2>&1; then
        fail "unconfirmed block device was accepted"
    fi
    grep -q 'requires --yes' "$work/unconfirmed.log" ||
        fail "unconfirmed block device failed for the wrong reason"
    echo "raven-ufs-selftest: unconfirmed block-device refusal PASS"
fi

mounted=$(lsblk -rno NAME,MOUNTPOINT 2>/dev/null |
    awk '$2 != "" && $1 ~ /^loop/ {print "/dev/" $1; exit}')
if [ -n "$mounted" ]; then
    if "$raven" --yes put "$work/put.txt" /probe "$mounted" >"$work/mounted.log" 2>&1; then
        fail "mounted block device was accepted"
    fi
    grep -Eq 'mounted|running root' "$work/mounted.log" ||
        fail "mounted block device failed for the wrong reason"
    echo "raven-ufs-selftest: mounted block-device refusal PASS"
fi

echo "raven-ufs-selftest: ESP sha256 $sha_before (unchanged)"
echo "raven-ufs-selftest: inode $inode_before size $size_before (unchanged)"
echo "raven-ufs-selftest: PASS"
