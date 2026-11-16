#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Capture a Raspberry Pi 5 Linux baseline on the board. This is an evidence kit: it runs under
# Raspberry Pi OS and writes text/hex dumps under tests/fixtures/pi5/linux-baseline/<stamp>/.
# It never copies the kernel, a GPL source tree or the RP1 firmware blob; it records paths and
# SHA-256 hashes only. Run as root (or with passwordless sudo) so dmesg/lspci/ethtool are complete.

set -eu

PATH="$PATH:/usr/sbin:/sbin"
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)

usage() {
    cat <<'EOF'
Usage: scripts/pi5-baseline.sh [options]

Capture the Pi 5 Linux oracle used by R9/R11: lspci, lsusb, dmesg, ethtool, ip, the device tree,
the RP1 firmware path/hash and bounded PCI/MMIO register dumps.

Options:
  -o, --out <dir>    output directory (default: tests/fixtures/pi5/linux-baseline/<stamp>-<serial>)
      --mmio <a:w>   read one MMIO word with devmem (address:width; width 8, 16, 32 or 64);
                      repeatable; only pass addresses known safe on the board
  -c, --check        list the available capture tools and exit
  -h, --help         show this help

Run it on the Pi 5 under Raspberry Pi OS, ideally as root:
  sudo scripts/pi5-baseline.sh -o tests/fixtures/pi5/linux-baseline/2026-...-<serial>
The output directory must not exist. Every command's output and exit status is stored in a
separate file; SHA256SUMS covers the captured text. The RP1 driver donor is
ravenbsd-resources/linux-rpi/drivers/firmware/rp1-fw.c; it is a mailbox protocol driver, not a
firmware file loader, so the script records modinfo/dmesg and hashes any rp1-named firmware it
finds under the standard firmware directories.
EOF
}

out=
check_only=0
mmio=
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o|--out)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            out=$2
            shift 2
            ;;
        --mmio)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            mmio="${mmio}${2}
"
            shift 2
            ;;
        -c|--check)
            check_only=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "pi5-baseline.sh: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [ "$check_only" -eq 1 ]; then
    for tool in lspci lsusb dmesg ip ethtool dtc devmem vcgencmd rpi-eeprom-config rpi-eeprom-update modinfo mmc sha256sum timeout od find sort awk; do
        if command -v "$tool" >/dev/null 2>&1; then
            echo "ok      $tool"
        else
            echo "missing $tool (optional unless the captured section needs it)"
        fi
    done
    exit 0
fi

if [ -z "$out" ]; then
    stamp=$(date -u +%Y%m%dT%H%M%SZ)
    serial=$(tr -d '\000' < /proc/device-tree/serial-number 2>/dev/null || true)
    serial=$(printf '%s' "${serial:-unknown}" | tr -c '[:alnum:]._-' '_')
    out="tests/fixtures/pi5/linux-baseline/$stamp-$serial"
fi
if [ -e "$out" ]; then
    echo "pi5-baseline.sh: output already exists: $out" >&2
    exit 1
fi
mkdir -p "$out"

priv=
if [ "$(id -u)" -ne 0 ] && command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
    priv="sudo -n"
fi

{
    echo "Pi 5 Linux baseline"
    echo "script: scripts/pi5-baseline.sh"
    echo "started: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "host: $(uname -srm 2>/dev/null || echo unknown)"
    echo "uid: $(id -u 2>/dev/null || echo unknown)"
    echo "privileged: ${priv:-no (run as root for complete dmesg/lspci/ethtool)}"
    echo "output: $out"
    echo "commit: $(git -C "$script_dir/.." rev-parse HEAD 2>/dev/null || echo unknown)"
} > "$out/capture-info.txt"

capture() {
    name=$1
    shift
    file=$out/$name.txt
    {
        printf '$'
        for argument in "$@"; do
            printf ' %s' "$argument"
        done
        printf '\n'
    } > "$file"
    if "$@" >> "$file" 2>&1; then
        printf 'exit=0\n' >> "$file"
    else
        status=$?
        printf 'exit=%s\n' "$status" >> "$file"
    fi
}

capture_priv() {
    name=$1
    shift
    file=$out/$name.txt
    {
        printf '$'
        for argument in "$@"; do
            printf ' %s' "$argument"
        done
        printf '\n'
    } > "$file"
    if [ -n "$priv" ]; then
        if $priv "$@" >> "$file" 2>&1; then
            printf 'exit=0\n' >> "$file"
        else
            status=$?
            printf 'exit=%s\n' "$status" >> "$file"
        fi
    elif "$@" >> "$file" 2>&1; then
        printf 'exit=0\n' >> "$file"
    else
        status=$?
        printf 'exit=%s\n' "$status" >> "$file"
    fi
}

capture uname uname -a
capture os-release cat /etc/os-release
capture cmdline cat /proc/cmdline
capture cpuinfo cat /proc/cpuinfo
capture meminfo cat /proc/meminfo
capture partitions cat /proc/partitions
capture lsblk lsblk -o NAME,SIZE,TYPE,FSTYPE,MOUNTPOINTS
capture df df -h
capture lspci-tree lspci -t
capture lspci-nn lspci -nn
capture_priv lspci-vv lspci -vv
capture_priv lspci-xxx lspci -xxx
capture lsusb lsusb
capture lsusb-tree lsusb -t
capture_priv lsusb-v lsusb -v
capture_priv dmesg dmesg

grep -i -E 'rp1|pcie|brcm|sdhci|xhci|eth|phy|firmware' "$out/dmesg.txt" \
    > "$out/dmesg-filtered.txt" 2>&1 || true

capture ip-addr ip -details addr
capture ip-link ip -details link
capture ip-route ip route
capture ip-neigh ip neigh

if command -v ip >/dev/null 2>&1; then
    for iface in $(ip -o link show 2>/dev/null | awk -F': ' '{print $2}' | cut -d@ -f1); do
        [ "$iface" = lo ] && continue
        safe=$(printf '%s' "$iface" | tr -c '[:alnum:]._-' '_')
        capture_priv "ethtool-$safe" ethtool "$iface"
        capture_priv "ethtool-info-$safe" ethtool -i "$iface"
        capture_priv "ethtool-features-$safe" ethtool -k "$iface"
        capture_priv "ethtool-stats-$safe" ethtool -S "$iface"
    done
fi

if command -v mmc >/dev/null 2>&1 && [ -e /dev/mmcblk0 ]; then
    capture_priv mmc-extcsd mmc extcsd read /dev/mmcblk0
fi

if command -v vcgencmd >/dev/null 2>&1; then
    capture vcgencmd-version vcgencmd version
    capture vcgencmd-bootloader vcgencmd bootloader_version
    capture vcgencmd-config vcgencmd get_config int
    capture vcgencmd-clock-core vcgencmd measure_clock core
    capture vcgencmd-temp vcgencmd measure_temp
fi
if command -v rpi-eeprom-update >/dev/null 2>&1; then
    capture rpi-eeprom-update rpi-eeprom-update
fi
if command -v rpi-eeprom-config >/dev/null 2>&1; then
    capture_priv rpi-eeprom-config rpi-eeprom-config
fi

capture_priv modinfo-rp1-fw modinfo rp1-fw
capture modinfo-rp1-fw-path modinfo -F filename rp1-fw

if command -v dtc >/dev/null 2>&1; then
    capture device-tree dtc -I fs -O dts /proc/device-tree
else
    {
        echo "=== /proc/device-tree files ==="
        find /proc/device-tree -type f -print 2>/dev/null | sort
        for property in model compatible chosen/stdout-path; do
            path="/proc/device-tree/$property"
            if [ -f "$path" ]; then
                echo "=== $path ==="
                od -An -tx1 -v "$path" 2>/dev/null | head -n 16
            fi
        done
    } > "$out/device-tree.txt"
fi

{
    echo "=== per-device PCI config space (first 256 bytes each) ==="
    for device in /sys/bus/pci/devices/*; do
        [ -d "$device" ] || continue
        echo "=== $(basename "$device") ==="
        od -An -tx1 -v "$device/config" 2>/dev/null | head -n 16
    done
} > "$out/pci-config.txt"

: > "$out/.rp1-firmware-files"
for directory in /lib/firmware /usr/lib/firmware /boot/firmware /boot; do
    [ -d "$directory" ] || continue
    find "$directory" -maxdepth 5 -type f -iname '*rp1*' -print \
        >> "$out/.rp1-firmware-files" 2>/dev/null || true
done
sort -u "$out/.rp1-firmware-files" > "$out/rp1-firmware-files.txt"
{
    echo "# SHA-256 of RP1-named firmware files found on the board"
    while IFS= read -r file; do
        [ -f "$file" ] && sha256sum "$file"
    done < "$out/rp1-firmware-files.txt"
} > "$out/rp1-firmware-hashes.txt"
rm -f "$out/.rp1-firmware-files"

if [ -n "$mmio" ]; then
    if command -v devmem >/dev/null 2>&1; then
        printf '%s' "$mmio" | while IFS= read -r entry; do
            [ -n "$entry" ] || continue
            address=${entry%%:*}
            width=${entry#*:}
            [ "$width" = "$entry" ] && width=32
            case "$width" in
                8|16|32|64) ;;
                *) echo "skip invalid width: $entry" >> "$out/mmio.txt"; continue ;;
            esac
            safe=$(printf '%s' "$entry" | tr -c '[:alnum:]._-' '_')
            capture_priv "mmio-$safe" devmem "$address" "$width"
        done
    else
        echo "devmem unavailable; MMIO dumps skipped" > "$out/mmio.txt"
    fi
fi

{
    echo "# Pi 5 Linux baseline"
    echo "Started: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "Files:"
    find "$out" -type f ! -name SHA256SUMS -print | sort
    echo
    echo "# SHA256SUMS"
    (
        cd "$out"
        find . -type f ! -name SHA256SUMS -print | sort | while IFS= read -r file; do
            sha256sum "$file"
        done
    )
} > "$out/SHA256SUMS"

printf 'pi5-baseline: captured %s files under %s\n' "$(find "$out" -type f | wc -l | tr -d ' ')" "$out"
