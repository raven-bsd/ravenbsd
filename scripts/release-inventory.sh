#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Machine-readable release image inventory.
#
# Usage: scripts/release-inventory.sh <image>
#        scripts/release-inventory.sh --initfs <initfs> --kind <uefi|pi4>
#
# The image form detects the FAT boot partition from the GPT, extracts the initfs
# (the UEFI ESP file `raven/initfs.img` or the initfs appended to the Pi 4
# `kernel8.img`), lists every path/size/mode/CRC, and checks the release content.
# The --initfs form is the host-test aid: it runs the same content checks on a
# standalone initfs image without needing a disk image.

set -eu

LC_ALL=C
export LC_ALL

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

usage() {
    cat <<'EOF'
usage: scripts/release-inventory.sh <image>
       scripts/release-inventory.sh --initfs <initfs> --kind <uefi|pi4>

  <image>        release disk image (build/<arch>/ravenbsd-*.img)
  --initfs FILE  standalone initfs image (host tests; no disk extraction)
  --kind KIND    image kind for --initfs: uefi (default) or pi4
  -h, --help     show this help

Output is key=value lines. `path=` lines are the inventory; `flag=` lines are
fatal findings (exit 1); `warn=` lines are release-hygiene warnings (exit 0).
EOF
}

die() {
    echo "release-inventory: $*" >&2
    exit 2
}

have() {
    command -v "$1" >/dev/null 2>&1
}

image=
initfs_arg=
kind=

while [ "$#" -gt 0 ]; do
    case "$1" in
        --initfs)
            [ "$#" -ge 2 ] || die "--initfs needs a file"
            initfs_arg=$2
            shift 2
            ;;
        --kind)
            [ "$#" -ge 2 ] || die "--kind needs uefi or pi4"
            kind=$2
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        --*)
            usage >&2
            die "unknown option: $1"
            ;;
        *)
            [ -z "$image" ] || die "exactly one image is required"
            image=$1
            shift
            ;;
    esac
done

if [ -n "$initfs_arg" ]; then
    [ -z "$image" ] || die "use either an image or --initfs, not both"
    [ -f "$initfs_arg" ] || die "initfs not found: $initfs_arg"
    kind=${kind:-uefi}
    case "$kind" in
        uefi|pi4) ;;
        *) die "unknown kind: $kind (expected uefi or pi4)" ;;
    esac
else
    [ -n "$image" ] || {
        usage >&2
        exit 2
    }
    [ -f "$image" ] || die "image not found: $image"
    [ -z "$kind" ] || die "--kind is only valid with --initfs"
fi

for cmd in awk cargo file grep python3; do
    have "$cmd" || die "required command not found: $cmd"
done

work=$(mktemp -d "${TMPDIR:-/tmp}/release-inventory.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM

: > "$work/flags"
: > "$work/warnings"

flag() {
    printf 'release-inventory: flag=%s %s\n' "$1" "$2"
    printf '%s %s\n' "$1" "$2" >> "$work/flags"
}

warn() {
    printf 'release-inventory: warn=%s %s\n' "$1" "$2"
    printf '%s %s\n' "$1" "$2" >> "$work/warnings"
}

# Pi 4 kernel8.img is the flat kernel with the initfs appended. Find a complete
# RAVENFS1 archive at the end and write it to the requested output file.
extract_pi4() {
    input=$1
    output=$2
    python3 - "$input" "$output" <<'PY'
import struct
import sys
from pathlib import Path

MAGIC = b"RAVENFS1"
HEADER = 64
ENTRY = 48
data = Path(sys.argv[1]).read_bytes()
out = Path(sys.argv[2])

candidates = []
pos = 0
while True:
    pos = data.find(MAGIC, pos)
    if pos < 0:
        break
    if pos + HEADER <= len(data):
        version, header_size, count, entry_size = struct.unpack_from("<IIII", data, pos + 8)
        names_off, names_len, total = struct.unpack_from("<QQQ", data, pos + 24)
        table_end = HEADER + count * ENTRY
        if (
            version == 1
            and header_size == HEADER
            and entry_size == ENTRY
            and table_end <= len(data)
            and names_off >= table_end
            and names_off + names_len <= total
            and total > 0
            and pos + total == len(data)
        ):
            candidates.append(pos)
    pos += 1

if len(candidates) != 1:
    raise SystemExit(
        f"expected one complete initfs at the end of {sys.argv[1]}, found {len(candidates)}"
    )
out.write_bytes(data[candidates[0] :])
PY
}

# Extract every initfs entry to a staging tree. The reader validates the archive
# before the shell checks its contents.
extract_initfs() {
    input=$1
    output=$2
    python3 - "$input" "$output" <<'PY'
import struct
import sys
from pathlib import Path

MAGIC = b"RAVENFS1"
HEADER = 64
ENTRY = 48
data = Path(sys.argv[1]).read_bytes()
out = Path(sys.argv[2])
root = out.resolve()

if data[:8] != MAGIC:
    raise SystemExit("not a RAVENFS1 initfs")
version, header_size, count, entry_size = struct.unpack_from("<IIII", data, 8)
names_off, names_len, total = struct.unpack_from("<QQQ", data, 24)
if version != 1 or header_size != HEADER or entry_size != ENTRY:
    raise SystemExit("unsupported initfs header")
if total != len(data):
    raise SystemExit("initfs total size does not match the file")
if HEADER + count * ENTRY > len(data) or names_off + names_len > len(data):
    raise SystemExit("initfs table or names out of bounds")

names = data[names_off : names_off + names_len]
out.mkdir(parents=True, exist_ok=True)
for index in range(count):
    base = HEADER + index * ENTRY
    name_off, name_len, data_off, data_len, mode, _crc = struct.unpack_from(
        "<IIQQII", data, base
    )
    if name_off + name_len > len(names):
        raise SystemExit(f"entry {index}: name out of bounds")
    name = names[name_off : name_off + name_len].decode("utf-8")
    if not name.startswith("/"):
        raise SystemExit(f"entry {index} is not absolute: {name}")
    parts = name.split("/")[1:]
    if not parts or any(part in ("", ".", "..") for part in parts):
        raise SystemExit(f"entry {index} has an invalid path: {name}")
    if data_off + data_len > len(data):
        raise SystemExit(f"entry {index} data out of bounds: {name}")
    target = out.joinpath(*parts)
    resolved = target.resolve()
    if resolved != root and root not in resolved.parents:
        raise SystemExit(f"entry {index} escapes the staging tree: {name}")
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data[data_off : data_off + data_len])
    target.chmod(mode & 0o777)
PY
}

if [ -n "$initfs_arg" ]; then
    initfs=$initfs_arg
else
    have sgdisk || die "required command not found: sgdisk"
    have mdir || die "required command not found: mdir"
    have mcopy || die "required command not found: mcopy"

    offset=$(sgdisk -p "$image" | awk '$1 == 1 { print $2 * 512; exit }')
    [ -n "$offset" ] || die "no partition 1 in $image"

    if mdir -i "$image@@$offset" ::/kernel8.img >/dev/null 2>&1; then
        kind=pi4
        mcopy -o -i "$image@@$offset" ::/kernel8.img "$work/kernel8.img"
        extract_pi4 "$work/kernel8.img" "$work/initfs.img" ||
            die "cannot extract the Pi 4 initfs"
    elif mdir -i "$image@@$offset" ::/raven/initfs.img >/dev/null 2>&1; then
        kind=uefi
        mcopy -o -i "$image@@$offset" ::/raven/initfs.img "$work/initfs.img"
    else
        die "no raven/initfs.img or kernel8.img in partition 1 of $image"
    fi
    initfs=$work/initfs.img
fi

cargo run -q -p raven-initfs --features std --example lsinitfs -- "$initfs" \
    > "$work/list" 2> "$work/lsinitfs.err" || {
    cat "$work/lsinitfs.err" >&2
    die "lsinitfs failed for $initfs"
}
entries=$(wc -l < "$work/list")

printf 'release-inventory: image=%s\n' "${image:-$initfs}"
printf 'release-inventory: kind=%s\n' "$kind"
printf 'release-inventory: initfs=%s entries=%s\n' "$initfs" "$entries"
while IFS= read -r line; do
    set -- $line
    [ "$#" -ge 4 ] || continue
    printf 'release-inventory: path=%s size=%s mode=%s crc=%s\n' "$1" "$2" "$3" "$4"
done < "$work/list"

extract_initfs "$initfs" "$work/tree" || die "cannot extract $initfs"

repo_root=$(git -C "$root" rev-parse --show-toplevel 2>/dev/null || true)

present() {
    awk -v path="$1" '$1 == path { found = 1 } END { exit !found }' "$work/list"
}

check_path() {
    if ! present "$1"; then
        flag missing-critical "path=$1"
    fi
}

for path in \
    /bin/rsh /bin/login /bin/getty /bin/ls /bin/ping /bin/brush \
    /etc/passwd /etc/shadow
do
    check_path "$path"
done

if [ "$kind" = "pi4" ]; then
    for path in \
        /sbin/genet /sbin/devd /sbin/platd /sbin/netd \
        /etc/services.d/genet.toml /etc/services.d/devd.toml \
        /etc/services.d/netd.toml /etc/pi4.dtb /etc/drivers.toml
    do
        check_path "$path"
    done
else
    for path in \
        /sbin/virtio-net /sbin/netd /sbin/resolvd \
        /etc/services.d/netd.toml /etc/services.d/resolvd.toml /etc/drivers.toml
    do
        check_path "$path"
    done
fi

check_manifest() {
    file=$1
    text=$2
    label=$3
    if [ ! -f "$work/tree$file" ]; then
        return 0
    fi
    if ! grep -Fq -- "$text" "$work/tree$file"; then
        flag nic-contract "path=$file missing=$label"
    fi
}

if [ "$kind" = "pi4" ]; then
    check_manifest /etc/services.d/genet.toml 'provides = ["scheme:nic"]' scheme:nic
    check_manifest /etc/services.d/genet.toml 'enable = false' "genet disabled"
    check_manifest /etc/services.d/devd.toml 'args = ["devd", "dtb", "launch"]' "dtb launch"
    check_manifest /etc/services.d/devd.toml 'grants = ["platform-root"]' platform-root
    check_manifest /etc/services.d/netd.toml 'scheme:nic' scheme:nic
    check_manifest /etc/drivers.toml 'brcm,bcm2711-genet-v5' genet-compatible
    check_manifest /etc/drivers.toml '/sbin/genet' genet-exec
else
    check_manifest /etc/services.d/netd.toml 'scheme:nic' scheme:nic
    check_manifest /etc/services.d/netd.toml 'scheme:net' scheme:net
    check_manifest /etc/services.d/resolvd.toml 'scheme:net' scheme:net
    check_manifest /etc/drivers.toml 'virtio-net' virtio-net
fi

# Executable files under /bin or /sbin must carry at least one execute bit.
awk '$1 ~ /^\/(bin|sbin)\// { perm = substr($3, length($3) - 2); if (perm !~ /[1357]/) print $1, $3 }' \
    "$work/list" > "$work/nonexec"
while IFS= read -r line; do
    set -- $line
    flag non-executable "path=$1 mode=$2"
done < "$work/nonexec"

# Duplicate launcher names across /bin and /sbin are a release smell: one
# driver should have one canonical launcher path.
awk '$1 ~ /^\/(bin|sbin)\// {
        perm = substr($3, length($3) - 2)
        if (perm ~ /[1357]/) {
            name = $1
            sub(/^.*\//, "", name)
            paths[name] = paths[name] " " $1
            count[name]++
        }
     }
     END {
        for (name in count)
            if (count[name] > 1)
                print name, count[name], paths[name]
     }' "$work/list" > "$work/duplicates"
while IFS= read -r line; do
    set -- $line
    name=$1
    count=$2
    shift 2
    flag duplicate-launcher "name=$name count=$count paths=$*"
done < "$work/duplicates"

# ELF and embedded-path checks.
find "$work/tree" -type f > "$work/files"
while IFS= read -r file; do
    rel=${file#"$work/tree"}
    magic=$(dd if="$file" bs=1 count=4 2>/dev/null | od -An -tx1 | tr -d ' \n')
    is_elf=0
    [ "$magic" = "7f454c46" ] && is_elf=1

    if [ "$is_elf" -eq 1 ]; then
        description=$(file -b "$file")
        case "$description" in
            *stripped*) ;;
            *) flag not-stripped "path=$rel type=$description" ;;
        esac
    fi

    if grep -aFq -- "/target/" "$file"; then
        if [ "$is_elf" -eq 1 ]; then
            flag build-path "path=$rel match=/target/"
        else
            warn text-build-path "path=$rel match=/target/"
        fi
    fi
    if [ -n "$repo_root" ] && grep -aFq -- "$repo_root" "$file"; then
        if [ "$is_elf" -eq 1 ]; then
            flag repo-path "path=$rel match=$repo_root"
        else
            warn text-repo-path "path=$rel match=$repo_root"
        fi
    fi
    if grep -aEq -- '/(home|media|Users)/' "$file"; then
        warn host-path "path=$rel"
    fi
done < "$work/files"

flags=$(wc -l < "$work/flags")
warnings=$(wc -l < "$work/warnings")
printf 'release-inventory: summary entries=%s flags=%s warnings=%s\n' \
    "$entries" "$flags" "$warnings"

[ "$flags" -eq 0 ]
