#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Pi 4 first-power-on preflight. `--check` is read-only: it reports the release
# image hash when the image exists, the SD target, the UART adapter, the current
# 115200 8N1 settings and the wiring reminder. `--flash` is owner-run: it refuses
# a mounted or root disk, asks for the device path, writes the image and verifies
# the read-back hash. Capture and transcript grading stay with REL3's scripts.

set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
root=$(CDPATH= cd -- "$script_dir/.." && pwd -P)
cd "$root"

image=build/aarch64/ravenbsd-pi4.img
expected_sha=
sd=
device=
assume_yes=0
mode=
markers=tests/fixtures/pi4-bringup/expected-sequence.txt
image_sha=

usage() {
    cat <<'EOF'
usage:
  scripts/pi4-bringup.sh --check [-i IMAGE] [--sha256 HASH] [--sd DEV] [--device TTY]
  scripts/pi4-bringup.sh --flash SD [-i IMAGE] [--device TTY] [--yes]
  scripts/pi4-bringup.sh --markers
  scripts/pi4-bringup.sh --help

  -c, --check        read-only host preflight; never writes the image or a device
  -f, --flash DEV    owner only: write IMAGE to the whole SD block device DEV
  -i, --image PATH   release image (default: build/aarch64/ravenbsd-pi4.img)
      --sha256 HASH  expected image SHA-256; a mismatch is a hard failure
      --sd DEV       SD block device to check (auto-detects one removable disk)
  -d, --device TTY   USB-UART adapter (auto-detects one /dev/ttyUSB* or ttyACM*)
      --yes          skip the interactive device-path confirmation for --flash
      --markers      print the ordered expected bring-up markers
  -h, --help         show this help

--check exits 0 when the host checklist is consistent; a missing release image
or absent board hardware is reported as an owner step, not a fake pass. --flash
requires the image and a confirmed block device. No board result is claimed.
EOF
}

note() {
    printf 'pi4-bringup: %s\n' "$*"
}

fail() {
    printf 'pi4-bringup: FAIL: %s\n' "$*" >&2
    exit 1
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        -c|--check)
            [ -z "$mode" ] || fail "--check, --flash and --markers are mutually exclusive"
            mode=check
            shift
            ;;
        -f|--flash)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            [ -z "$mode" ] || fail "--check, --flash and --markers are mutually exclusive"
            mode=flash
            sd=$2
            shift 2
            ;;
        -i|--image)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            image=$2
            shift 2
            ;;
        --sha256)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            expected_sha=$2
            shift 2
            ;;
        --sd)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            sd=$2
            shift 2
            ;;
        -d|--device)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            device=$2
            shift 2
            ;;
        --yes)
            assume_yes=1
            shift
            ;;
        --markers)
            [ -z "$mode" ] || fail "--check, --flash and --markers are mutually exclusive"
            mode=markers
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            printf 'pi4-bringup: unknown argument: %s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

[ -n "$mode" ] || { usage >&2; exit 2; }
if [ "$assume_yes" -eq 1 ] && [ "$mode" != flash ]; then
    fail "--yes is only valid with --flash"
fi

print_wiring() {
    cat <<'EOF'
pi4-bringup: UART wiring (3.3 V TTL, never connect adapter VCC/5V to the Pi):
  Pi pin 6  GND              -> adapter GND
  Pi pin 8  GPIO14 / TXD     -> adapter RXD
  Pi pin 10 GPIO15 / RXD     -> adapter TXD
pi4-bringup: serial settings: 115200 8N1, no flow control, no echo; console=ttyAMA0.
pi4-bringup: power: 5V/3A USB-C supply; a phone charger is not sufficient.
EOF
}

print_markers() {
    [ -f "$markers" ] || fail "marker file is missing: $markers"
    printf '%-12s %-38s %s\n' stage marker meaning
    while IFS='|' read -r stage marker meaning; do
        case "$stage" in
            ''|'#'*) continue ;;
        esac
        printf '%-12s %-38s %s\n' "$stage" "$marker" "$meaning"
    done < "$markers"
}

detect_sd() {
    command -v lsblk >/dev/null 2>&1 || return 1
    candidates=$(lsblk -dpno NAME,RM,TYPE 2>/dev/null | awk '$2 == "1" && $3 == "disk" {print $1}')
    count=$(printf '%s\n' "$candidates" | sed '/^[[:space:]]*$/d' | wc -l | tr -d ' ')
    if [ "$count" -eq 1 ]; then
        sd=$candidates
        return 0
    fi
    if [ "$count" -gt 1 ]; then
        note "sd: multiple removable block devices detected; pass --sd /dev/sdX"
    fi
    return 1
}

detect_serial() {
    candidates=$(ls /dev/ttyUSB* /dev/ttyACM* 2>/dev/null || true)
    count=$(printf '%s\n' "$candidates" | sed '/^[[:space:]]*$/d' | wc -l | tr -d ' ')
    if [ "$count" -eq 1 ]; then
        device=$candidates
        return 0
    fi
    if [ "$count" -gt 1 ]; then
        note "serial: multiple USB-UART adapters detected; pass --device /dev/ttyUSB0"
    fi
    return 1
}

check_image() {
    if [ ! -e "$image" ]; then
        note "image: MISSING $image"
        note "  build: RAVEN_RPI_FIRMWARE=/path/to/rpi-firmware/boot cargo xtask image --arch aarch64 --platform pi4 --profile pi4-release"
        if [ -n "$expected_sha" ]; then
            return 1
        fi
        return 2
    fi
    [ -f "$image" ] || { note "image: not a regular file: $image"; return 1; }
    [ -s "$image" ] || { note "image: empty: $image"; return 1; }
    command -v sha256sum >/dev/null 2>&1 || { note "image: sha256sum is required"; return 1; }
    image_sha=$(sha256sum "$image" | awk '{print $1}')
    note "image: $image"
    note "image: sha256 $image_sha"
    if [ -n "$expected_sha" ]; then
        if [ "$image_sha" != "$expected_sha" ]; then
            note "image: expected sha256 $expected_sha"
            return 1
        fi
        note "image: sha256 matches --sha256"
    else
        note "image: record this hash in the evidence block"
    fi
    return 0
}

check_sd() {
    if [ -z "$sd" ]; then
        if detect_sd; then
            note "sd: detected removable block device $sd"
        else
            note "sd: no removable block device detected; pass --sd /dev/sdX (owner step)"
            return 2
        fi
    fi
    [ -b "$sd" ] || { note "sd: not a block device: $sd"; return 1; }
    command -v lsblk >/dev/null 2>&1 || { note "sd: lsblk is required"; return 1; }
    sd_type=$(lsblk -dno TYPE "$sd" 2>/dev/null | tr -d ' ')
    if [ "$sd_type" != disk ]; then
        note "sd: not a whole disk (type=${sd_type:-unknown}): $sd"
        note "sd: pass the whole device (/dev/sdX or /dev/mmcblk0), never a partition"
        return 1
    fi
    command -v findmnt >/dev/null 2>&1 || { note "sd: findmnt is required"; return 1; }
    command -v blockdev >/dev/null 2>&1 || { note "sd: blockdev is required"; return 1; }
    mounts=$(lsblk -nlo MOUNTPOINT "$sd" 2>/dev/null | sed '/^[[:space:]]*$/d')
    if [ -n "$mounts" ]; then
        note "sd: mounted: $mounts"
        note "sd: unmount every partition before --flash"
        return 1
    fi
    root_src=$(findmnt -n -o SOURCE / 2>/dev/null || true)
    if [ -n "$root_src" ]; then
        root_disk=$(lsblk -no PKNAME "$root_src" 2>/dev/null | head -n 1 | tr -d ' ')
        if [ -n "$root_disk" ] && [ "$(basename "$(readlink -f "$sd")")" = "$root_disk" ]; then
            note "sd: refusing the root disk: $sd"
            return 1
        fi
    fi
    size=$(blockdev --getsize64 "$sd" 2>/dev/null || echo 0)
    [ "$size" -gt 0 ] || { note "sd: cannot read the size of $sd"; return 1; }
    model=$(lsblk -dno MODEL "$sd" 2>/dev/null | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
    note "sd: $sd size=$size bytes model=${model:-unknown}"
    if [ -f "$image" ]; then
        image_size=$(wc -c < "$image" | tr -d ' ')
        if [ "$size" -lt "$image_size" ]; then
            note "sd: too small for $image ($size < $image_size)"
            return 1
        fi
    fi
    return 0
}

check_serial() {
    if [ -z "$device" ]; then
        if detect_serial; then
            note "serial: detected $device"
        else
            note "serial: no USB-UART adapter detected; pass --device /dev/ttyUSB0 (owner step)"
            return 2
        fi
    fi
    [ -c "$device" ] || { note "serial: not a character device: $device"; return 1; }
    if [ ! -r "$device" ] || [ ! -w "$device" ]; then
        note "serial: no read/write permission: $device"
        note "serial: add the owner to the dialout group (or fix the udev rule) and re-login"
        return 1
    fi
    note "serial: $device is readable and writable"
    if command -v stty >/dev/null 2>&1; then
        speed=$(stty -F "$device" speed 2>/dev/null || echo unknown)
        note "serial: current speed=$speed; the capture script sets 115200 8N1"
        settings=$(stty -F "$device" -a 2>/dev/null || true)
        if printf '%s\n' "$settings" | grep -q 'cs8' &&
            printf '%s\n' "$settings" | grep -q -- '-cstopb' &&
            printf '%s\n' "$settings" | grep -q -- '-parenb' &&
            printf '%s\n' "$settings" | grep -q -- '-crtscts'; then
            note "serial: current settings are 8N1/no flow control"
        else
            note "serial: current settings are not 8N1/no flow control; scripts/pi4-serial.sh sets them"
        fi
    else
        note "serial: stty is required for capture"
        return 1
    fi
    return 0
}

run_check() {
    label=$1
    shift
    rc=0
    "$@" || rc=$?
    case "$rc" in
        0) note "$label: ok" ;;
        2) note "$label: pending owner step"; pending=$((pending + 1)) ;;
        *) note "$label: FAILED"; failed=1 ;;
    esac
}

case "$mode" in
    markers)
        print_markers
        ;;
    check)
        print_wiring
        pending=0
        failed=0
        run_check image check_image
        run_check sd check_sd
        run_check serial check_serial
        printf '\n'
        print_markers
        if [ "$failed" -ne 0 ]; then
            note "host check FAIL"
            exit 1
        fi
        if [ "$pending" -ne 0 ]; then
            note "host check PASS; $pending owner step(s) pending; board result remains UNVERIFIED"
        else
            note "host check PASS; board result remains UNVERIFIED"
        fi
        ;;
    flash)
        [ -n "$sd" ] || fail "--flash needs the SD block device"
        check_image || fail "image check failed"
        check_sd || fail "SD check failed"
        [ -n "$image_sha" ] || image_sha=$(sha256sum "$image" | awk '{print $1}')
        image_size=$(wc -c < "$image" | tr -d ' ')
        [ "$image_size" -ge 1048576 ] ||
            fail "refusing to flash a small/non-release image: $image ($image_size bytes)"
        note "flash target: $sd"
        note "flash image:  $image"
        note "image sha256: $image_sha"
        if [ "$assume_yes" -ne 1 ]; then
            printf 'Type the device path (%s) to confirm: ' "$sd"
            answer=
            read -r answer || true
            [ "$answer" = "$sd" ] || fail "aborted; device not confirmed"
        fi
        command -v dd >/dev/null 2>&1 || fail "dd is required"
        note "writing; this overwrites the whole device $sd"
        dd if="$image" of="$sd" bs=4M conv=fsync
        sync
        read_sha=$(dd if="$sd" bs=1M count=$(( (image_size + 1048575) / 1048576 )) 2>/dev/null |
            head -c "$image_size" | sha256sum | awk '{print $1}')
        [ "$read_sha" = "$image_sha" ] || fail "read-back hash mismatch: wrote $image_sha, read $read_sha"
        note "flash verified: read-back sha256 $read_sha"
        if [ -n "$device" ]; then
            note "capture: scripts/pi4-release.sh --device $device -i $image -l build/pi4/$image_sha.log"
        else
            note "capture: scripts/pi4-release.sh --device /dev/ttyUSB0 -i $image -l build/pi4/$image_sha.log"
        fi
        note "the board result stays UNVERIFIED until the captured log passes the REL3 gate"
        ;;
esac
