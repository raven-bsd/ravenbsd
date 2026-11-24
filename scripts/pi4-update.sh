#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Fast in-place Pi 4 SD update: FAT boot files through mtools, UFS2 root through
# ufs-inject. The script updates an existing image or SD card; it never reflashes.
# --check and --dry-run are read-only. --backup saves a whole file image; a block
# device needs --backup-dir for the FAT/UFS slices.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

image=build/aarch64/ravenbsd-pi4-sd.img
image_set=0
device=
check_only=0
dry_run=0
backup=0
backup_dir=
kernel=
dtb=
staging=
owner_uid=0
owner_gid=0
assume_yes=0
work=
inject=
restore_image=0
backup_image_path=
target=
fat_start=
fat_sectors=
ufs_start=
ufs_sectors=
ufs_off=
ufs_len=

usage() {
    cat <<'EOF'
usage:
  scripts/pi4-update.sh --check [-i IMAGE]
  scripts/pi4-update.sh [-i IMAGE | -d DEVICE] [options]

  -i, --image PATH       whole GPT SD image (default: build/aarch64/ravenbsd-pi4-sd.img)
  -d, --device DEV       whole SD block device (requires root for writes)
      --check            verify GPT, FAT, UFS; verify update sources; no writes
      --dry-run          verify and print the update commands; no writes
      --program PKG[:DEST] build an aarch64 Rust user program and inject it
      --file SRC:DEST    inject one host file into the UFS root
      --staging DIR      inject a rootfs staging tree (dirs/files/symlinks)
      --fat SRC:DEST     copy one file into the FAT boot partition
      --kernel PATH      copy PATH to FAT kernel8.img
      --dtb PATH         copy PATH to FAT /<basename>
      --uid N            UFS owner uid for injected objects (default 0)
      --gid N            UFS owner gid for injected objects (default 0)
      --backup           copy the whole image before writing (file image)
      --backup-dir DIR   save FAT/UFS partition slices before writing (device or image)
  -y, --yes              skip the block-device confirmation
  -h, --help             show this help

Examples:
  sh scripts/pi4-update.sh --check
  sh scripts/pi4-update.sh --program einkd:/sbin/einkd --file etc/motd:/etc/motd
  sh scripts/pi4-update.sh --device /dev/sdX --kernel build/aarch64/kernel8.img --backup-dir build/pi4-backup
EOF
}

new_list() {
    : > "$1"
}

add_list() {
    printf '%s\n' "$2" >> "$1"
}

need_value() {
    [ "$#" -ge 2 ] || { usage >&2; exit 2; }
}

work=$(mktemp -d "${TMPDIR:-/tmp}/pi4-update.XXXXXX")
programs=$work/programs
files=$work/files
fats=$work/fats
new_list "$programs"
new_list "$files"
new_list "$fats"

while [ "$#" -gt 0 ]; do
    case "$1" in
        -i|--image)
            need_value "$@"
            image=$2
            image_set=1
            shift 2
            ;;
        -d|--device)
            need_value "$@"
            device=$2
            shift 2
            ;;
        --check) check_only=1; shift ;;
        --dry-run) dry_run=1; shift ;;
        --backup) backup=1; shift ;;
        --backup-dir)
            need_value "$@"
            backup_dir=$2
            shift 2
            ;;
        --program)
            need_value "$@"
            add_list "$programs" "$2"
            shift 2
            ;;
        --file)
            need_value "$@"
            add_list "$files" "$2"
            shift 2
            ;;
        --fat)
            need_value "$@"
            add_list "$fats" "$2"
            shift 2
            ;;
        --staging)
            need_value "$@"
            staging=$2
            shift 2
            ;;
        --kernel)
            need_value "$@"
            kernel=$2
            shift 2
            ;;
        --dtb)
            need_value "$@"
            dtb=$2
            shift 2
            ;;
        --uid)
            need_value "$@"
            owner_uid=$2
            shift 2
            ;;
        --gid)
            need_value "$@"
            owner_gid=$2
            shift 2
            ;;
        -y|--yes) assume_yes=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "pi4-update: unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

if [ "$image_set" -eq 1 ] && [ -n "$device" ]; then
    echo "pi4-update: --image and --device are mutually exclusive" >&2
    exit 2
fi

cleanup() {
    status=$?
    trap - EXIT HUP INT TERM
    if [ "$status" -ne 0 ] && [ "$restore_image" -eq 1 ] && [ -n "$backup_image_path" ] && [ -n "$target" ]; then
        if cp -p "$backup_image_path" "$target" 2>/dev/null; then
            echo "pi4-update: restored $target from $backup_image_path" >&2
        else
            echo "pi4-update: WARNING: restore failed; backup is $backup_image_path" >&2
        fi
    fi
    [ -n "${work:-}" ] && rm -rf "$work"
    exit "$status"
}

fail() {
    printf 'pi4-update: FAIL %s\n' "$*" >&2
    exit 1
}

trap 'cleanup' EXIT HUP INT TERM

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        fail "sha256sum or shasum is required"
    fi
}

target_size() {
    if [ -b "$1" ]; then
        blockdev --getsize64 "$1"
    else
        wc -c < "$1" | tr -d ' '
    fi
}

gpt_layout() {
    python3 - "$target" "$(target_size "$target")" <<'PY'
import os
import struct
import sys
import zlib

path = sys.argv[1]
size = int(sys.argv[2])
if size < 4096 or size % 512 != 0:
    raise SystemExit("target is not a whole number of 512-byte sectors")
fd = os.open(path, os.O_RDONLY)
def read(offset, count):
    return os.pread(fd, count, offset)

try:
    mbr = read(0, 512)
    if len(mbr) != 512 or mbr[510:512] != b"\x55\xaa":
        raise SystemExit("protective MBR signature is not 55aa")
    if mbr[446 + 4] != 0xEE:
        raise SystemExit("protective MBR does not declare a 0xEE GPT partition")
    header = read(512, 92)
    if len(header) != 92 or header[:8] != b"EFI PART":
        raise SystemExit("primary GPT header signature missing")
    header_size = struct.unpack_from("<I", header, 12)[0]
    if header_size < 92 or header_size > 512:
        raise SystemExit("invalid GPT header size")
    stored_crc = struct.unpack_from("<I", header, 16)[0]
    check = bytearray(read(512, header_size))
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
    if backup + 1 != size // 512:
        raise SystemExit("GPT backup LBA does not match the target size")
    entries_bytes = count * entry_size
    entries = read(entries_lba * 512, entries_bytes)
    if len(entries) != entries_bytes:
        raise SystemExit("GPT entry table is truncated")
    if zlib.crc32(entries) & 0xffffffff != struct.unpack_from("<I", header, 88)[0]:
        raise SystemExit("GPT entry table CRC mismatch")
    backup_header = read(backup * 512, 92)
    if len(backup_header) != 92 or backup_header[:8] != b"EFI PART":
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
    print(f"total_sectors={size // 512}")
finally:
    os.close(fd)
PY
}

fat_check() {
    command -v mtype >/dev/null 2>&1 || fail "mtools (mtype) is required"
    manifest=tests/fixtures/pi4-sd/manifest.txt
    [ -f "$manifest" ] || fail "FAT manifest missing: $manifest"
    while IFS=' ' read -r path min check; do
        case "$path" in ''|'#'*) continue ;; esac
        out=$work/fat-entry
        mtype -i "$target@@$((fat_start * 512))" "::/$path" > "$out" 2>/dev/null ||
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
    done < "$manifest"
}

build_inject() {
    cargo build -q -p ufs-inject > "$work/inject-build.log" 2>&1 || {
        tail -20 "$work/inject-build.log" >&2
        fail "ufs-inject host build failed"
    }
    target_dir=${CARGO_TARGET_DIR:-$root/target}
    case "$target_dir" in
        /*) ;;
        *) target_dir="$root/$target_dir" ;;
    esac
    inject=$target_dir/debug/ufs-inject
    [ -x "$inject" ] || fail "ufs-inject binary missing: $inject"
}

ufs_check() {
    "$inject" --offset "$ufs_off" --size "$ufs_len" "$target" check > "$work/ufs-check.log" 2>&1 || {
        tail -12 "$work/ufs-check.log" >&2
        fail "UFS check failed"
    }
    tail -1 "$work/ufs-check.log"
}

refuse_unsafe() {
    if [ -b "$target" ]; then
        command -v lsblk >/dev/null 2>&1 || fail "lsblk is required for block-device safety"
        command -v blockdev >/dev/null 2>&1 || fail "blockdev is required for block-device safety"
        root_src=$(findmnt -n -o SOURCE / 2>/dev/null || true)
        dev=$(readlink -f "$target")
        [ -b "$dev" ] || fail "resolved target is not a block device: $dev"
        if [ -n "$root_src" ]; then
            case "$root_src" in
                "$dev"|"$dev"p*|"$dev"[0-9]*)
                    fail "refusing to write the root disk/partition: $dev"
                    ;;
            esac
            root_parent=$(lsblk -no PKNAME "$root_src" 2>/dev/null | head -n 1 || true)
            if [ -n "$root_parent" ] && [ "$(basename "$dev")" = "$root_parent" ]; then
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
    else
        [ -f "$target" ] || fail "image not found: $target"
        if command -v losetup >/dev/null 2>&1; then
            attached=$(losetup -j "$target" 2>/dev/null || true)
            [ -z "$attached" ] || fail "image is attached to a loop device: $attached"
        fi
    fi
}

confirm_device() {
    [ -b "$target" ] || return 0
    [ "$(id -u)" -eq 0 ] || fail "writing a block device is owner-only (run as root)"
    [ "$assume_yes" -eq 1 ] && return 0
    printf 'This will update %s in place.\nType YES to continue: ' "$target"
    read -r answer || answer=
    [ "$answer" = "YES" ] || fail "confirmation declined"
}

backup_target() {
    if [ "$backup" -eq 0 ] && [ -z "$backup_dir" ]; then
        return 0
    fi
    if [ -z "$backup_dir" ]; then
        [ -b "$target" ] && fail "--backup on a block device needs --backup-dir"
        backup_image_path=$target.upd1-backup
        cp -p "$target" "$backup_image_path" || fail "cannot create $backup_image_path"
        restore_image=1
        echo "pi4-update: backup $backup_image_path"
        return 0
    fi
    mkdir -p "$backup_dir" || fail "cannot create $backup_dir"
    dd if="$target" of="$backup_dir/fat.img" bs=512 skip="$fat_start" count="$fat_sectors" status=none ||
        fail "FAT backup failed"
    dd if="$target" of="$backup_dir/ufs.img" bs=512 skip="$ufs_start" count="$ufs_sectors" status=none ||
        fail "UFS backup failed"
    echo "pi4-update: backup $backup_dir/fat.img $backup_dir/ufs.img"
}

inject_file() {
    _isrc=$1
    _idest=$2
    [ -f "$_isrc" ] || fail "source file missing: $_isrc"
    if [ "$dry_run" -eq 1 ]; then
        echo "pi4-update: would inject $_isrc -> $_idest"
        return 0
    fi
    "$inject" --offset "$ufs_off" --size "$ufs_len" "$target" put "$_isrc" "$_idest" \
        --uid "$owner_uid" --gid "$owner_gid" --parents ||
        fail "inject $_isrc -> $_idest failed"
    echo "pi4-update: injected $_isrc -> $_idest"
}

inject_dir() {
    _irel=$1
    if [ "$dry_run" -eq 1 ]; then
        echo "pi4-update: would mkdir $_irel"
        return 0
    fi
    "$inject" --offset "$ufs_off" --size "$ufs_len" "$target" mkdir "$_irel" \
        --uid "$owner_uid" --gid "$owner_gid" --parents ||
        fail "mkdir $_irel failed"
}

inject_symlink() {
    _irel=$1
    _itarget=$2
    if [ "$dry_run" -eq 1 ]; then
        echo "pi4-update: would symlink $_irel -> $_itarget"
        return 0
    fi
    "$inject" --offset "$ufs_off" --size "$ufs_len" "$target" symlink "$_itarget" "$_irel" \
        --uid "$owner_uid" --gid "$owner_gid" --parents ||
        fail "symlink $_irel failed"
}

fat_copy() {
    _fsrc=$1
    _fdest=$2
    _fdest=${_fdest#/}
    [ -f "$_fsrc" ] || fail "FAT source file missing: $_fsrc"
    if [ "$dry_run" -eq 1 ]; then
        echo "pi4-update: would copy $_fsrc -> FAT /$_fdest"
        return 0
    fi
    mcopy -i "$target@@$((fat_start * 512))" -o "$_fsrc" "::/$_fdest" ||
        fail "FAT copy $_fsrc -> /$_fdest failed"
    echo "pi4-update: FAT $_fsrc -> /$_fdest"
}

build_program() {
    _spec=$1
    _package=${_spec%%:*}
    _dest=
    case "$_spec" in
        *:*) _dest=${_spec#*:} ;;
    esac
    [ -n "$_package" ] || fail "empty --program package"
    if [ "$dry_run" -eq 1 ]; then
        echo "pi4-update: would build cargo xtask build user --arch aarch64 --release -p $_package"
        echo "pi4-update: would inject the artifact -> ${_dest:-/sbin/<program>}"
        return 0
    fi
    _log=$work/build-$_package.log
    cargo xtask build user --arch aarch64 --release -p "$_package" > "$_log" 2>&1 || {
        tail -20 "$_log" >&2
        fail "build $_package failed"
    }
    _artifact=$(tail -n 1 "$_log")
    if [ ! -f "$_artifact" ]; then
        _candidate=build/aarch64/user/$(basename "$_package")
        if [ -f "$_candidate" ]; then
            _artifact=$_candidate
        else
            fail "cannot find the built artifact for $_package (last line: $_artifact)"
        fi
    fi
    if [ -z "$_dest" ]; then
        _dest=/sbin/$(basename "$_artifact")
    fi
    inject_file "$_artifact" "$_dest"
}

inject_staging() {
    _root=$1
    [ -d "$_root" ] || fail "staging directory missing: $_root"
    find "$_root" -type d | sort > "$work/staging-dirs"
    find "$_root" -type f | sort > "$work/staging-files"
    find "$_root" -type l | sort > "$work/staging-links"
    while IFS= read -r _path; do
        [ "$_path" = "$_root" ] && continue
        _rel=${_path#"$_root"}
        [ -n "$_rel" ] || continue
        inject_dir "$_rel"
    done < "$work/staging-dirs"
    while IFS= read -r _path; do
        _rel=${_path#"$_root"}
        [ -n "$_rel" ] || continue
        inject_file "$_path" "$_rel"
    done < "$work/staging-files"
    while IFS= read -r _path; do
        _rel=${_path#"$_root"}
        [ -n "$_rel" ] || continue
        _link=$(readlink "$_path") || fail "readlink $_path failed"
        inject_symlink "$_rel" "$_link"
    done < "$work/staging-links"
}

check_sources() {
    while IFS= read -r _spec; do
        [ -n "$_spec" ] || continue
        _src=${_spec%%:*}
        case "$_spec" in
            *:*) ;;
            *) fail "--file needs SRC:DEST: $_spec" ;;
        esac
        [ -f "$_src" ] || fail "source file missing: $_src"
    done < "$files"
    while IFS= read -r _spec; do
        [ -n "$_spec" ] || continue
        _src=${_spec%%:*}
        case "$_spec" in
            *:*) ;;
            *) fail "--fat needs SRC:DEST: $_spec" ;;
        esac
        [ -f "$_src" ] || fail "FAT source file missing: $_src"
    done < "$fats"
    if [ -n "$staging" ]; then
        [ -d "$staging" ] || fail "staging directory missing: $staging"
    fi
    if [ -n "$kernel" ]; then
        [ -f "$kernel" ] || fail "kernel missing: $kernel"
    fi
    if [ -n "$dtb" ]; then
        [ -f "$dtb" ] || fail "DTB missing: $dtb"
    fi
}

if [ -n "$device" ]; then
    target=$device
    [ -b "$target" ] || fail "--device target is not a block device: $target"
else
    target=$image
fi

[ -e "$target" ] || fail "target not found: $target (build the image first)"
refuse_unsafe
layout=$(gpt_layout) || fail "GPT validation failed"
eval "$layout"
echo "pi4-update: GPT ok fat=$fat_start+$fat_sectors ufs=$ufs_start+$ufs_sectors total=$total_sectors"
ufs_off=$((ufs_start * 512))
ufs_len=$((ufs_sectors * 512))

fat_check
build_inject
ufs_check
check_sources

if [ "$check_only" -eq 1 ]; then
    if [ -s "$files" ] || [ -s "$fats" ] || [ -s "$programs" ] || [ -n "$staging" ] || [ -n "$kernel" ] || [ -n "$dtb" ]; then
        echo "pi4-update: update sources ok (no writes; use --dry-run for the plan)"
    fi
    if [ -f "$target" ]; then
        echo "pi4-update: check PASS sha256=$(sha256_file "$target")"
    else
        echo "pi4-update: check PASS (block device)"
    fi
    exit 0
fi

if [ "$dry_run" -eq 1 ]; then
    echo "pi4-update: dry run; no writes"
fi

confirm_device
backup_target

while IFS= read -r spec; do
    [ -n "$spec" ] || continue
    build_program "$spec"
done < "$programs"

while IFS= read -r spec; do
    [ -n "$spec" ] || continue
    src=${spec%%:*}
    dest=${spec#*:}
    case "$dest" in
        /*) ;;
        *) dest="/$dest" ;;
    esac
    inject_file "$src" "$dest"
done < "$files"

if [ -n "$staging" ]; then
    inject_staging "$staging"
fi

while IFS= read -r spec; do
    [ -n "$spec" ] || continue
    src=${spec%%:*}
    dest=${spec#*:}
    fat_copy "$src" "$dest"
done < "$fats"

if [ -n "$kernel" ]; then
    fat_copy "$kernel" kernel8.img
fi
if [ -n "$dtb" ]; then
    fat_copy "$dtb" "$(basename "$dtb")"
fi

ufs_check
restore_image=0
if [ "$dry_run" -eq 1 ]; then
    echo "pi4-update: DRY RUN PASS (no writes)"
elif [ -f "$target" ]; then
    echo "pi4-update: PASS sha256=$(sha256_file "$target")"
else
    echo "pi4-update: PASS $target"
fi
