#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Capture a Raspberry Pi 4 UART log and check the FAT boot manifest of the image first.
# The capture is timestamped and written under build/pi4/<image-sha256>.log by default.
# No hardware is touched by --check: it only reads the image with mtools.

set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
root=$(CDPATH= cd -- "$script_dir/.." && pwd -P)
cd "$root"

device=/dev/ttyUSB0
image=build/aarch64/ravenbsd-pi4.img
baud=115200
log=
check_only=0
do_check=1

usage() {
    cat <<'EOF'
Usage: scripts/pi4-serial.sh [options]

Capture a timestamped Pi 4 serial log and, unless disabled, check the image's FAT boot files.

Options:
  -d, --device <tty>   UART adapter (default: /dev/ttyUSB0)
  -i, --image <path>   built Pi 4 image (default: build/aarch64/ravenbsd-pi4.img)
  -b, --baud <rate>    capture baud rate (default: 115200)
  -l, --log <path>     log path (default: build/pi4/<image-sha256>.log)
  -c, --check          run the manifest check only and exit; no serial device needed
      --no-check       skip the image manifest check before capture
  -h, --help           show this help

The manifest is tests/fixtures/pi4-image/manifest.txt; it needs mtools (mtype). The capture needs
stty and either picocom or cat. Run `sh -n scripts/pi4-serial.sh` for a syntax check.
EOF
}

manifest_check() {
    image=$1
    manifest=$2
    failed=0

    if [ ! -f "$image" ]; then
        echo "pi4-manifest: image not found: $image" >&2
        return 1
    fi
    if ! command -v mtype >/dev/null 2>&1; then
        echo "pi4-manifest: mtools (mtype) is required for the manifest check" >&2
        return 1
    fi

    echo "pi4 image manifest: $image"
    while IFS=' ' read -r path min_bytes check; do
        case "$path" in
            ''|'#'*) continue ;;
        esac
        case "$min_bytes" in
            ''|*[!0-9]*)
                echo "FAIL manifest: bad minimum for $path" >&2
                failed=1
                continue
                ;;
        esac

        tmp="$work/$(printf '%s' "$path" | tr '/' '_')"
        if ! mtype -i "$image@@1M" "::/$path" > "$tmp" 2>/dev/null; then
            echo "FAIL $path: missing from the FAT boot partition" >&2
            failed=1
            continue
        fi
        size=$(wc -c < "$tmp" | tr -d ' ')
        if [ "$size" -lt "$min_bytes" ]; then
            echo "FAIL $path: $size bytes, minimum $min_bytes" >&2
            failed=1
            continue
        fi
        echo "  $path $size"

        case "$check" in
            none) ;;
            config)
                for line in arm_64bit=1 kernel=kernel8.img enable_uart=1 uart_2ndstage=1; do
                    if ! grep -qxF "$line" "$tmp"; then
                        echo "FAIL config.txt: missing '$line'" >&2
                        failed=1
                    fi
                done
                ;;
            kernel)
                first=$(od -An -tx1 -N16 "$tmp" | tr -d ' \n')
                if [ "$first" = "00000000000000000000000000000000" ]; then
                    echo "FAIL kernel8.img: first 16 bytes are zero" >&2
                    failed=1
                fi
                ;;
            fdt)
                magic=$(od -An -tx1 -N4 "$tmp" | tr -d ' \n')
                if [ "$magic" != "d00dfeed" ]; then
                    echo "FAIL $path: not an FDT (magic $magic)" >&2
                    failed=1
                fi
                ;;
            *)
                echo "FAIL manifest: unknown check '$check' for $path" >&2
                failed=1
                ;;
        esac
    done < "$manifest"

    if [ "$failed" -ne 0 ]; then
        return 1
    fi
    echo "pi4-manifest: PASSED"
    return 0
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        -d|--device)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            device=$2
            shift 2
            ;;
        -i|--image)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            image=$2
            shift 2
            ;;
        -b|--baud)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            baud=$2
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
            echo "pi4-serial.sh: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

work=$(mktemp -d "${TMPDIR:-/tmp}/pi4-evidence.XXXXXX")
trap 'rm -rf "$work"' EXIT

image_sha=missing
if [ -f "$image" ]; then
    if command -v sha256sum >/dev/null 2>&1; then
        image_sha=$(sha256sum "$image" | awk '{print $1}')
    else
        echo "pi4-serial.sh: sha256sum is required for the image hash" >&2
        exit 1
    fi
fi

manifest_out="$work/manifest.log"
if [ "$do_check" -eq 1 ]; then
    if ! manifest_check "$image" "tests/fixtures/pi4-image/manifest.txt" > "$manifest_out" 2>&1; then
        cat "$manifest_out" >&2
        exit 1
    fi
fi

if [ "$check_only" -eq 1 ]; then
    cat "$manifest_out"
    exit 0
fi

if [ -z "$log" ]; then
    if [ "$image_sha" = missing ]; then
        short=$(git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)
        log="build/pi4/$short.log"
    else
        log="build/pi4/$image_sha.log"
    fi
fi
case "$log" in
    /*) ;;
    *) log="$root/$log" ;;
esac
mkdir -p "$(dirname -- "$log")"

{
    echo "# Pi 4 serial capture"
    echo "# started: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "# host: $(uname -srm 2>/dev/null || echo unknown)"
    echo "# commit: $(git rev-parse HEAD 2>/dev/null || echo unknown)"
    echo "# image: $image"
    echo "# image sha256: $image_sha"
    echo "# device: $device at $baud 8N1"
    echo "# manifest:"
    sed 's/^/#   /' "$manifest_out"
} >> "$log"

if [ ! -c "$device" ]; then
    echo "pi4-serial.sh: not a character device: $device" >&2
    echo "pi4-serial.sh: connect the UART adapter or pass --check to test only the image" >&2
    exit 1
fi

stty -F "$device" raw "$baud" cs8 -cstopb -parenb -crtscts -echo

if command -v picocom >/dev/null 2>&1; then
    run_capture() {
        picocom -q -b "$baud" -d 8 -p n -s 1 --imap lfcrlf "$device"
    }
else
    echo "pi4-serial.sh: picocom not found; falling back to cat (install picocom for clean control)" >&2
    run_capture() {
        cat "$device"
    }
fi

timestamp() {
    awk '{
        cmd = "date -u +%Y-%m-%dT%H:%M:%SZ"
        cmd | getline stamp
        close(cmd)
        print stamp, $0
        fflush()
    }'
}

stop_capture() {
    printf '# stopped: %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$log"
    exit 0
}
trap stop_capture INT TERM

echo "pi4-serial.sh: logging to $log (Ctrl-C to stop)" >&2
run_capture | timestamp | tee -a "$log"
printf '# stopped: %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$log"
