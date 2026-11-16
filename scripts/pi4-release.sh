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
do_check=1
markers=tests/fixtures/pi4-release/gate-markers.txt
initfs=build/aarch64/initfs.img

usage() {
    cat <<'EOF'
usage:
  scripts/pi4-release.sh --check [-i IMAGE]
  scripts/pi4-release.sh --log LOG
  scripts/pi4-release.sh --device TTY [--log LOG] [-i IMAGE]
  scripts/pi4-release.sh --flash DEVICE [--device TTY] [--log LOG]

  -i, --image PATH   release image (default: build/aarch64/ravenbsd-pi4.img)
  -d, --device TTY   UART adapter for capture (default: /dev/ttyUSB0)
  -f, --flash DEV    write the image to an SD-card block device first (owner only)
  -l, --log PATH     existing log to check, or capture destination
  -c, --check        host-side image/manifest/initfs check only
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
    for path in /etc/passwd /etc/group /etc/shadow /etc/profile /etc/motd /etc/drivers.toml; do
        if grep -Fq -- "$path" "$work/initfs.strings"; then
            echo "pi4-release: initfs path $path ok"
        else
            echo "pi4-release: initfs path $path MISSING" >&2
            missing=1
        fi
    done
    [ "$missing" -eq 0 ]
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
    initfs_paths
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
