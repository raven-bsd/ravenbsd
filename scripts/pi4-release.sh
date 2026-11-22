#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Pi 4 release image check and serial gate.
#
# The host side builds and checks the release image: firmware/DTB/FAT manifest,
# flat kernel8, and the release initfs paths. A board capture can be supplied
# with --log (or captured with --device); the release markers are then checked.
# Without a captured log no board result is claimed.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

image=build/aarch64/ravenbsd-pi4.img
device=/dev/ttyUSB0
have_device=0
flash_device=
log=
check_only=0
repro=0
do_check=1
markers=tests/fixtures/pi4-release/gate-markers.txt
initfs=build/aarch64/initfs.img
dtb=build/fixtures/pi4-release/pi4.dtb

usage() {
    cat <<'EOF'
usage:
  scripts/pi4-release.sh --check [-i IMAGE]
  scripts/pi4-release.sh --repro
  scripts/pi4-release.sh --log LOG
  scripts/pi4-release.sh --device TTY [--log LOG] [-i IMAGE]
  scripts/pi4-release.sh --flash DEVICE [--device TTY] [--log LOG]

  -i, --image PATH   release image (default: build/aarch64/ravenbsd-pi4.img)
  -d, --device TTY   UART adapter for capture (default: /dev/ttyUSB0)
  -f, --flash DEV    write the image to an SD-card block device first (owner only)
  -l, --log PATH     existing log to check, or capture destination
  -c, --check        host-side image/manifest/initfs check only
      --repro        clean build/fixtures, build the image twice, compare hashes,
                     and run --check on both; writes only under build/
      --no-check     skip the host-side check before capture
  -h, --help         show this help

Capture uses 115200 8N1 and console=ttyAMA0, matching scripts/pi4-serial.sh.
The release gates are kernel banner, login root/raven, `raven#`, `ls /bin`,
`ping <gateway>` and a clean exit. A board result is claimed only from a
captured log; `--check` never opens the serial port.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        -i|--image)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            image=$2
            shift 2
            ;;
        -d|--device)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            device=$2
            have_device=1
            shift 2
            ;;
        -f|--flash)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            flash_device=$2
            shift 2
            ;;
        -l|--log)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            log=$2
            shift 2
            ;;
        -c|--check)
            check_only=1
            shift
            ;;
        --repro)
            repro=1
            shift
            ;;
        --no-check)
            do_check=0
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "pi4-release: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

work=$(mktemp -d "${TMPDIR:-/tmp}/pi4-release.XXXXXX")
trap 'rm -rf "$work"' EXIT

find_firmware() {
    if [ -n "${RAVEN_RPI_FIRMWARE:-}" ] && [ -d "$RAVEN_RPI_FIRMWARE" ]; then
        return 0
    fi
    for candidate in \
        "$root/../ravenbsd-resources/rpi-firmware/boot" \
        "$root/../../ravenbsd-resources/rpi-firmware/boot" \
        "$root/../../../ravenbsd-resources/rpi-firmware/boot" \
        "${HOME:-/nonexistent}/work/ravenos-workspace/ravenbsd-resources/rpi-firmware/boot" \
        "${HOME:-/nonexistent}/ravenbsd-resources/rpi-firmware/boot"
    do
        if [ -d "$candidate" ]; then
            RAVEN_RPI_FIRMWARE=$candidate
            export RAVEN_RPI_FIRMWARE
            return 0
        fi
    done
    echo "pi4-release: Raspberry Pi firmware not found; set RAVEN_RPI_FIRMWARE" >&2
    return 1
}

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        echo "pi4-release: sha256sum or shasum is required" >&2
        return 1
    fi
}

fdt_check() {
    [ -f "$dtb" ] || {
        echo "pi4-release: DTB fixture missing: $dtb" >&2
        return 1
    }
    python3 - "$dtb" > "$work/fdt.txt" <<'PY'
import struct
import sys
import zlib
from pathlib import Path

path = Path(sys.argv[1])
data = path.read_bytes()
if len(data) < 40 or data[:4] != b"\xd0\x0d\xfe\xed":
    raise SystemExit(f"pi4-release: {path} is not an FDT blob")
total = struct.unpack_from(">I", data, 4)[0]
if total > len(data) or total < 40:
    raise SystemExit(f"pi4-release: {path} totalsize {total} exceeds {len(data)}")
print(len(data), f"{zlib.crc32(data) & 0xffffffff:08x}")
PY
    read -r dtb_size dtb_crc < "$work/fdt.txt"
    CARGO_TARGET_DIR="$root/build/cargo/pi4-release-check" \
        cargo run -q -p raven-initfs --features std --example lsinitfs -- "$initfs" > "$work/initfs.list"
    entry=$(awk '$1 == "/etc/pi4.dtb" { print $2, $4 }' "$work/initfs.list")
    [ -n "$entry" ] || {
        echo "pi4-release: /etc/pi4.dtb is not in the initfs" >&2
        return 1
    }
    set -- $entry
    if [ "$1" != "$dtb_size" ] || [ "$2" != "$dtb_crc" ]; then
        echo "pi4-release: /etc/pi4.dtb does not match the validated fixture" >&2
        return 1
    fi
    echo "pi4-release: /etc/pi4.dtb FDT ok totalsize=$dtb_size crc32=$dtb_crc"
}

repro_check() {
    find_firmware || return 1
    out=build/pi4-release/repro
    rm -rf "$out"
    mkdir -p "$out"
    for side in a b; do
        rm -rf build/fixtures
        sh tests/fixtures/pi4-release/build.sh > "$out/fixture-$side.log" 2>&1 || {
            echo "pi4-release: fixture build failed for $side" >&2
            return 1
        }
        if ! CARGO_TARGET_DIR="$root/build/cargo/pi4-release-repro" \
            cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release \
            --output "$out/$side.img" > "$out/build-$side.log" 2>&1; then
            echo "pi4-release: image build failed for $side" >&2
            tail -20 "$out/build-$side.log" >&2
            return 1
        fi
        cp build/aarch64/initfs.img "$out/$side.initfs.img"
    done
    hash_a=$(sha256_file "$out/a.img") || return 1
    hash_b=$(sha256_file "$out/b.img") || return 1
    if [ "$hash_a" != "$hash_b" ]; then
        echo "pi4-release: reproducibility FAIL ($hash_a != $hash_b)" >&2
        return 1
    fi
    image="$out/a.img"
    initfs="$out/a.initfs.img"
    image_check || return 1
    image="$out/b.img"
    initfs="$out/b.initfs.img"
    image_check || return 1
    cp "$out/a.img" build/aarch64/ravenbsd-pi4.img
    cp "$out/a.initfs.img" build/aarch64/initfs.img
    echo "pi4-release: reproducibility PASS"
    echo "  build a sha256 $hash_a"
    echo "  build b sha256 $hash_b"
}

manifest_check() {
    [ -f "$image" ] || {
        echo "pi4-release: image not found: $image" >&2
        return 1
    }
    sh scripts/pi4-serial.sh --check -i "$image"
}

initfs_paths() {
    [ -f "$initfs" ] || {
        echo "pi4-release: initfs not found: $initfs (build the release image first)" >&2
        return 1
    }
    strings "$initfs" > "$work/initfs.strings"
    missing=0
    for path in /sbin/vfsd /sbin/netd /sbin/platd /sbin/genet /sbin/pcid /sbin/devd \
        /bin/rsh /bin/login /bin/getty /bin/brush /bin/ls /bin/ping; do
        if grep -Fq -- "$path" "$work/initfs.strings"; then
            echo "pi4-release: initfs path $path ok"
        else
            echo "pi4-release: initfs path $path MISSING" >&2
            missing=1
        fi
    done
    for path in /etc/passwd /etc/group /etc/shadow /etc/profile /etc/motd /etc/drivers.toml /etc/pi4.dtb; do
        if grep -Fq -- "$path" "$work/initfs.strings"; then
            echo "pi4-release: initfs path $path ok"
        else
            echo "pi4-release: initfs path $path MISSING" >&2
            missing=1
        fi
    done
    [ "$missing" -eq 0 ]
}

manifest_check_release() {
    failed=0
    devd=profiles/pi4-release.d/devd.manifest
    genet=profiles/pi4-release.d/genet.manifest
    netd=profiles/pi4-release.d/netd.manifest
    platd=profiles/pi4-release.d/platd.manifest
    drivers=profiles/pi4-release.d/drivers.conf

    if ! grep -Fq 'args = ["devd", "dtb", "launch"]' "$devd"; then
        echo "pi4-release: devd.manifest does not use dtb launch" >&2
        failed=1
    fi
    for manifest in "$devd" "$netd"; do
        if grep -Eq '^enable = false$' "$manifest"; then
            echo "pi4-release: $manifest is disabled" >&2
            failed=1
        fi
    done
    for need in logd vfsd initfsd; do
        if ! grep -Fq -- "\"$need\"" "$devd"; then
            echo "pi4-release: devd.manifest lacks $need" >&2
            failed=1
        fi
    done
    if grep -Fq 'scheme:pci' "$devd"; then
        echo "pi4-release: devd.manifest still needs scheme:pci" >&2
        failed=1
    fi
    if ! grep -Fq 'grants = ["platform-root"]' "$devd"; then
        echo "pi4-release: devd.manifest lacks the platform-root grant" >&2
        failed=1
    fi
    if ! grep -Fq 'provides = ["scheme:plat", "scheme:dev", "scheme:nic"]' "$devd"; then
        echo "pi4-release: devd.manifest does not provide plat/dev/nic" >&2
        failed=1
    fi
    if ! grep -Eq '^enable = false$' "$platd"; then
        echo "pi4-release: platd.manifest must stay disabled; devd launches it" >&2
        failed=1
    fi
    if ! grep -Eq '^enable = false$' "$genet"; then
        echo "pi4-release: genet.manifest must stay disabled; devd launches it" >&2
        failed=1
    fi
    for need in logd 'scheme:plat'; do
        if ! grep -Fq -- "$need" "$genet"; then
            echo "pi4-release: genet.manifest lacks $need" >&2
            failed=1
        fi
    done
    if ! grep -Fq 'provides = ["scheme:nic"]' "$genet"; then
        echo "pi4-release: genet.manifest does not provide scheme:nic" >&2
        failed=1
    fi
    for need in logd 'scheme:nic'; do
        if ! grep -Fq -- "$need" "$netd"; then
            echo "pi4-release: netd.manifest lacks $need" >&2
            failed=1
        fi
    done
    if ! grep -Fq 'provides = ["scheme:net"]' "$netd"; then
        echo "pi4-release: netd.manifest does not provide scheme:net" >&2
        failed=1
    fi
    if ! grep -Fq 'dhcp-first' "$netd"; then
        echo "pi4-release: netd.manifest is not DHCP-first" >&2
        failed=1
    fi
    if ! grep -A5 '^name = "genet"' "$drivers" | grep -Fq 'slots=platform'; then
        echo "pi4-release: drivers.toml genet entry lacks slots=platform" >&2
        failed=1
    fi
    if [ "$(grep -c '^name = "genet"' "$drivers")" -ne 1 ]; then
        echo "pi4-release: duplicate genet driver entry" >&2
        failed=1
    fi
    if ! grep -A6 '^name = "platd"' "$drivers" | grep -Fq 'mailbox=3:64'; then
        echo "pi4-release: drivers.toml platd entry lacks the multi-window plan" >&2
        failed=1
    fi
    platd_args=$(grep -A6 '^name = "platd"' "$drivers" | grep '^args =')
    for token in 'bar=0' 'bar=1' 'bar=2' 'dma_size=256' 'mailbox=3:64' 'gpio=4:180' 'rng=5:40' 'dma=6:256'; do
        case "$platd_args" in
            *"$token"*) ;;
            *)
                echo "pi4-release: platd args lack $token" >&2
                failed=1
                ;;
        esac
    done
    if [ "$(grep -c '^name = "platd"' "$drivers")" -ne 1 ]; then
        echo "pi4-release: duplicate platd driver entry" >&2
        failed=1
    fi
    if [ "$(grep -l '^name = "platd"' profiles/pi4-release.d/*.manifest | wc -l | tr -d ' ')" -ne 1 ]; then
        echo "pi4-release: duplicate platd service launcher" >&2
        failed=1
    fi
    if [ "$(grep -h '^package = "raven-platd"' profiles/pi4-release.d/*.toml | wc -l | tr -d ' ')" -ne 1 ]; then
        echo "pi4-release: duplicate platd program entry" >&2
        failed=1
    fi
    if [ "$(grep -l '^name = "genet"' profiles/pi4-release.d/*.manifest | wc -l | tr -d ' ')" -ne 1 ]; then
        echo "pi4-release: duplicate genet service launcher" >&2
        failed=1
    fi
    if [ "$(grep -h '^package = "genet"' profiles/pi4-release.d/*.toml | wc -l | tr -d ' ')" -ne 1 ]; then
        echo "pi4-release: duplicate genet program entry" >&2
        failed=1
    fi
    [ "$failed" -eq 0 ]
}

image_check() {
    manifest_check || return 1
    mtype -i "$image@@1M" "::/kernel8.img" > "$work/kernel8" 2>/dev/null || {
        echo "pi4-release: cannot read kernel8.img from the FAT partition" >&2
        return 1
    }
    size=$(wc -c < "$work/kernel8" | tr -d ' ')
    [ "$size" -ge 65536 ] || {
        echo "pi4-release: kernel8.img is too small: $size bytes" >&2
        return 1
    }
    strings "$work/kernel8" > "$work/kernel.strings"
    grep -Fq 'RavenBSD kernel' "$work/kernel.strings" || {
        echo "pi4-release: kernel banner marker missing from kernel8.img" >&2
        return 1
    }
    echo "pi4-release: kernel8.img $size bytes, banner marker ok"
    initfs_paths || return 1
    fdt_check || return 1
    manifest_check_release || return 1
    has_ufs=$(python3 - "$image" <<'PY'
import struct
import sys

data = open(sys.argv[1], "rb").read()
if len(data) < 4096 or data[512:520] != b"EFI PART":
    print("no")
    raise SystemExit(0)
entries_lba = struct.unpack_from("<Q", data, 512 + 72)[0]
count, size = struct.unpack_from("<II", data, 512 + 80)
ufs = bytes.fromhex("b67c6e51cf6ed6118ff800022d09712b")
for index in range(count):
    entry = data[entries_lba * 512 + index * size:entries_lba * 512 + (index + 1) * size]
    if entry[:16] == ufs:
        print("yes")
        raise SystemExit(0)
print("no")
PY
)
    if [ "$has_ufs" = yes ]; then
        sh scripts/pi4-install.sh --check -i "$image" || return 1
        echo "pi4-release: SD FAT+UFS layout PASS"
    fi
    echo "pi4-release: image check PASS (board gates remain UNVERIFIED)"
}

evaluate_log() {
    [ -s "$log" ] || {
        echo "pi4-release: log is empty or missing: $log" >&2
        return 1
    }
    [ -f "$markers" ] || {
        echo "pi4-release: marker file is missing: $markers" >&2
        return 1
    }
    echo "pi4-release: log=$log"
    failed=0
    while IFS=' ' read -r gate marker; do
        case "$gate" in
            ''|'#'*) continue ;;
        esac
        if grep -Fq -- "$marker" "$log"; then
            echo "pi4-release: gate $gate PASS"
        else
            echo "pi4-release: gate $gate MISSING"
            failed=1
        fi
    done < "$markers"
    if grep -Eiq 'panic|EXCEPTION|login: .*incorrect|login: .*failed|ping: .*timeout' "$log"; then
        echo "pi4-release: forbidden failure marker in the transcript"
        failed=1
    fi
    if [ "$failed" -ne 0 ]; then
        echo "pi4-release: serial gate FAIL"
        return 1
    fi
    echo "pi4-release: serial gate PASS"
}

if [ "$repro" -eq 1 ]; then
    repro_check
    exit $?
fi

if [ "$check_only" -eq 1 ]; then
    image_check
    exit $?
fi

if [ -n "$flash_device" ]; then
    image_check || exit 1
    [ -b "$flash_device" ] || {
        echo "pi4-release: --flash target is not a block device: $flash_device" >&2
        exit 1
    }
    command -v dd >/dev/null 2>&1 || {
        echo "pi4-release: dd is required for --flash" >&2
        exit 1
    }
    echo "pi4-release: writing $image to $flash_device"
    dd if="$image" of="$flash_device" bs=4M conv=fsync
fi

if [ "$have_device" -eq 1 ] && [ -z "$log" ]; then
    log=build/pi4-release/serial.log
fi

if [ -n "$log" ] && [ "$have_device" -eq 1 ]; then
    if [ "$do_check" -eq 1 ]; then
        image_check || exit 1
    fi
    sh scripts/pi4-serial.sh -d "$device" -b 115200 -i "$image" -l "$log"
fi

if [ -n "$log" ]; then
    evaluate_log
    exit $?
fi

echo "pi4-release: no --device or --log supplied; the board gate is UNVERIFIED" >&2
echo "  build: cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release" >&2
echo "  host:  scripts/pi4-release.sh --check" >&2
echo "  board: scripts/pi4-release.sh --device /dev/ttyUSB0" >&2
exit 2
