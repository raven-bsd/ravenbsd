#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Pi 4 SD image check and owner-only installer.
#
# The image is a GPT disk with a FAT boot partition (firmware, config.txt,
# cmdline.txt, flat kernel8.img, DTB) followed by a populated UFS2 root. The
# host checks prove GPT/FAT/UFS and the root tree; no board result is claimed.
# --flash writes an existing image to a block device after owner, mount, root
# disk, size and confirmation checks.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

image=build/aarch64/ravenbsd-pi4-sd.img
manifest=tests/fixtures/pi4-sd/manifest.txt
root_tree=tests/fixtures/pi4-sd/root-tree.txt
staging=${RAVEN_PI4_STAGING:-build/aarch64/rootfs-staging}
flash_device=
assume_yes=0
check_only=0

usage() {
    cat <<'EOF'
usage:
  scripts/pi4-install.sh --check [-i IMAGE]
  scripts/pi4-install.sh --flash DEVICE [--yes] [-i IMAGE]

  -i, --image PATH   SD image (default: build/aarch64/ravenbsd-pi4-sd.img)
  -c, --check        verify GPT, FAT manifest, UFS fsck and the UFS root tree
  -f, --flash DEV    owner-only write to a block device; refuses mounted/root disks
  -y, --yes          skip the interactive confirmation in --flash
  -h, --help         show this help

Build the image first:
  RAVEN_RPI_FIRMWARE=... SOURCE_DATE_EPOCH=1700000000 \
    cargo xtask image --arch aarch64 --platform pi4 --profile pi4-rootfs
EOF
}

use_image() {
    [ "$#" -ge 2 ] || { usage >&2; exit 2; }
    image=$2
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        -i|--image) use_image "$@"; shift 2 ;;
        -c|--check) check_only=1; shift ;;
        -f|--flash) [ "$#" -ge 2 ] || { usage >&2; exit 2; }; flash_device=$2; shift 2 ;;
        -y|--yes) assume_yes=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "pi4-install: unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

fail() {
    printf 'pi4-install: FAIL %s\n' "$*" >&2
    exit 1
}

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        fail "sha256sum or shasum is required"
    fi
}

gpt_layout() {
    python3 - "$image" <<'PY'
import struct
import sys
import zlib

path = sys.argv[1]
data = open(path, "rb").read()
if len(data) < 4096 or len(data) % 512 != 0:
    raise SystemExit("image is not a whole number of 512-byte sectors")
if data[510:512] != b"\x55\xaa":
    raise SystemExit("protective MBR signature is not 55aa")
pmbr = data[446:462]
if pmbr[4] != 0xEE:
    raise SystemExit("protective MBR does not declare a 0xEE GPT partition")

header = data[512:512 + 92]
if header[:8] != b"EFI PART":
    raise SystemExit("primary GPT header signature missing")
header_size = struct.unpack_from("<I", header, 12)[0]
if header_size < 92 or header_size > 512:
    raise SystemExit("invalid GPT header size")
stored_crc = struct.unpack_from("<I", header, 16)[0]
check = bytearray(header[:header_size])
check[16:20] = b"\0\0\0\0"
if zlib.crc32(check) & 0xffffffff != stored_crc:
    raise SystemExit("primary GPT header CRC mismatch")
current, backup = struct.unpack_from("<QQ", header, 24)
first_usable, last_usable = struct.unpack_from("<QQ", header, 40)
entries_lba = struct.unpack_from("<Q", header, 72)[0]
count, entry_size = struct.unpack_from("<II", header, 80)
if count == 0 or entry_size < 128:
    raise SystemExit("invalid GPT entry table")
if current != 1:
    raise SystemExit("primary GPT header current LBA is not 1")
if backup + 1 != len(data) // 512:
    raise SystemExit("GPT backup LBA does not match the image size")
entries_bytes = count * entry_size
entries = data[entries_lba * 512:entries_lba * 512 + entries_bytes]
if len(entries) != entries_bytes:
    raise SystemExit("GPT entry table is truncated")
if zlib.crc32(entries) & 0xffffffff != struct.unpack_from("<I", header, 88)[0]:
    raise SystemExit("GPT entry table CRC mismatch")

backup_header = data[backup * 512:backup * 512 + 92]
if backup_header[:8] != b"EFI PART":
    raise SystemExit("backup GPT header signature missing")
if struct.unpack_from("<Q", backup_header, 24)[0] != backup:
    raise SystemExit("backup GPT header current LBA mismatch")
if struct.unpack_from("<Q", backup_header, 32)[0] != current:
    raise SystemExit("backup GPT header backup LBA mismatch")

parts = []
for index in range(count):
    entry = entries[index * entry_size:(index + 1) * entry_size]
    if entry[:16] == b"\0" * 16:
        continue
    begin, end = struct.unpack_from("<QQ", entry, 32)
    name = entry[56:128].decode("utf-16le", "replace").rstrip("\0")
    parts.append((entry[:16].hex(), begin, end, name))
if len(parts) != 2:
    raise SystemExit(f"expected 2 GPT partitions, found {len(parts)}")
fat_type = "28732ac11ff8d211ba4b00a0c93ec93b"
ufs_type = "b67c6e51cf6ed6118ff800022d09712b"
if parts[0][0] != fat_type or parts[1][0] != ufs_type:
    raise SystemExit("GPT partition types are not FAT-boot + UFS-root")
if parts[0][1] < first_usable or parts[1][2] > last_usable:
    raise SystemExit("a partition is outside the GPT usable range")
if parts[0][2] >= parts[1][1]:
    raise SystemExit("FAT and UFS partitions overlap")
print(f"fat_start={parts[0][1]}")
print(f"fat_sectors={parts[0][2] - parts[0][1] + 1}")
print(f"ufs_start={parts[1][1]}")
print(f"ufs_sectors={parts[1][2] - parts[1][1] + 1}")
print(f"total_sectors={len(data) // 512}")
PY
}

fat_check() {
    command -v mtype >/dev/null 2>&1 || fail "mtools (mtype) is required"
    [ -f "$manifest" ] || fail "FAT manifest missing: $manifest"
    while IFS=' ' read -r path min check; do
        case "$path" in ''|'#'*) continue ;; esac
        out=$work/fat-entry
        mtype -i "$image@@$((fat_start * 512))" "::/$path" > "$out" 2>/dev/null ||
            fail "FAT file missing: $path"
        size=$(wc -c < "$out" | tr -d ' ')
        [ "$size" -ge "$min" ] || fail "FAT file $path is $size bytes, need >= $min"
        case "$check" in
            config)
                for line in arm_64bit=1 kernel=kernel8.img enable_uart=1 uart_2ndstage=1; do
                    grep -Fqx "$line" "$out" || fail "config.txt missing '$line'"
                done
                ;;
            cmdline)
                grep -Fq 'console=ttyAMA0' "$out" || fail "cmdline.txt lacks console=ttyAMA0"
                ;;
            kernel)
                nonzero=$(od -An -tx1 -N16 "$out" | tr -d ' \n')
                [ "$nonzero" != "00000000000000000000000000000000" ] ||
                    fail "kernel8.img starts with zeros"
                ;;
            fdt)
                magic=$(od -An -tx1 -N4 "$out" | tr -d ' \n')
                [ "$magic" = "d00dfeed" ] || fail "$path is not an FDT blob"
                ;;
            none) ;;
            *) fail "unknown FAT check '$check' for $path" ;;
        esac
        echo "pi4-install: FAT $path $size bytes $check ok"
    done < "$manifest"
}

ufs_check() {
    ufs=$work/root.ufs
    dd if="$image" of="$ufs" bs=512 skip="$ufs_start" count="$ufs_sectors" status=none ||
        fail "cannot extract the UFS partition"
    if ! cargo run -q -p ufs-fsck -- -n "$ufs" > "$work/ufs-fsck.log" 2>&1; then
        tail -10 "$work/ufs-fsck.log" >&2
        fail "UFS fsck failed"
    fi
    tail -1 "$work/ufs-fsck.log"
    [ -d "$staging" ] || fail "rootfs staging tree missing: $staging"
    while IFS= read -r path; do
        case "$path" in ''|'#'*) continue ;; esac
        [ -e "$staging/${path#/}" ] || fail "UFS root tree missing: $path"
        echo "pi4-install: UFS path $path ok"
    done < "$root_tree"
}

check_image() {
    [ -f "$image" ] || fail "image not found: $image (build it first)"
    work=$(mktemp -d "${TMPDIR:-/tmp}/pi4-install.XXXXXX")
    trap 'rm -rf "$work"' EXIT HUP INT TERM
    layout=$(gpt_layout) || fail "GPT validation failed"
    eval "$layout"
    echo "pi4-install: GPT ok fat=$fat_start+$fat_sectors ufs=$ufs_start+$ufs_sectors total=$total_sectors"
    fat_check
    ufs_check
    echo "pi4-install: image check PASS sha256=$(sha256_file "$image")"
    echo "pi4-install: board evidence remains UNVERIFIED (SD3)"
}

flash_image() {
    [ "$(id -u)" -eq 0 ] || fail "--flash is owner-only (run as root)"
    [ -b "$flash_device" ] || fail "--flash target is not a block device: $flash_device"
    check_image
    dev=$(readlink -f "$flash_device")
    [ -b "$dev" ] || fail "resolved --flash target is not a block device: $dev"
    command -v lsblk >/dev/null 2>&1 || fail "lsblk is required for the safety checks"
    command -v blockdev >/dev/null 2>&1 || fail "blockdev is required for the size check"

    root_src=$(findmnt -n -o SOURCE / 2>/dev/null || true)
    if [ -n "$root_src" ]; then
        case "$root_src" in
            "$dev"|"$dev"p*|"$dev"[0-9]*)
                fail "refusing to write the root disk/partition: $dev"
                ;;
        esac
        root_parent=$(lsblk -no PKNAME "$root_src" 2>/dev/null | head -n 1 || true)
        dev_name=$(basename "$dev")
        if [ -n "$root_parent" ] && [ "$dev_name" = "$root_parent" ]; then
            fail "refusing to write the disk containing /: $dev"
        fi
    fi

    while IFS= read -r name; do
        [ -n "$name" ] || continue
        if awk -v node="/dev/$name" '$1 == node { found = 1 } END { exit !found }' /proc/mounts; then
            fail "refusing to write a mounted device: /dev/$name"
        fi
    done <<EOF
$(lsblk -ln -o NAME "$dev")
EOF

    image_size=$(wc -c < "$image" | tr -d ' ')
    dev_size=$(blockdev --getsize64 "$dev")
    [ "$dev_size" -ge "$image_size" ] ||
        fail "target is too small: $dev_size < $image_size bytes"
    echo "pi4-install: target=$dev size=$dev_size image=$image_size"
    if [ "$assume_yes" -ne 1 ]; then
        printf 'This will overwrite %s with %s.\nType YES to continue: ' "$dev" "$image"
        read -r answer || answer=
        [ "$answer" = "YES" ] || fail "confirmation declined"
    fi
    echo "pi4-install: writing $image to $dev"
    dd if="$image" of="$dev" bs=4M conv=fsync
    sync
    echo "pi4-install: flash complete (no board claim)"
}

if [ -n "$flash_device" ]; then
    flash_image
    exit 0
fi
if [ "$check_only" -eq 1 ]; then
    check_image
    exit 0
fi
usage >&2
exit 2
