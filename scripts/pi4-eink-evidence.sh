#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K385: capture Pi 4 E-Ink board evidence. Host checks never open a serial port and
# never claim board evidence. A board run captures the serial transcript and records
# the photo path/hash, panel SKU, fetch URL, NTP time and refresh interval. A log or
# photo that carries a QEMU/virt/synthetic/fixture marker is refused with exit 2.
#
# Usage:
#   scripts/pi4-eink-evidence.sh --check
#   scripts/pi4-eink-evidence.sh --dry-run
#   scripts/pi4-eink-evidence.sh --serial /dev/ttyUSB0 --log build/pi4/eink.serial.log \
#       --photo board-eink.jpg --panel <SKU> --url https://... --ntp 2026-11-21T16:00:00Z \
#       --refresh 60 --image build/aarch64/ravenbsd-pi4.img
#   scripts/pi4-eink-evidence.sh --log existing.serial.log --photo ... --panel ... \
#       --url ... --ntp ... --refresh ... --image ...
#
# Exit codes: 0 complete host check or complete evidence file; 1 capture incomplete or
# evidence fields missing; 2 usage, refused synthetic evidence, or a missing artifact.
# Exit 0 writes evidence for the owner; it does not issue a board PASS.

set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
root=$(CDPATH= cd -- "$script_dir/.." && pwd -P)
cd "$root"
. "$root/scripts/lib/mgmt.sh"

check_only=0
dry_run=0
serial=
log=
duration=120
image=build/aarch64/ravenbsd-pi4.img
photo=
panel=
url=
ntp=
refresh=
out=

usage() {
    cat <<'EOF'
Usage: scripts/pi4-eink-evidence.sh [options]

Host checks (no serial port is opened, no board evidence is claimed):
  --check           validate the procedure and expected artifact paths
  --dry-run         print the exact board command and required fields

Board capture / evidence:
  --serial <dev>    UART adapter to capture (default duration 120 s)
  --log <path>      serial transcript path (required with --serial)
  --duration <sec>  bounded capture duration (default 120)
  --image <path>    Pi 4 image (default build/aarch64/ravenbsd-pi4.img)
  --photo <path>    photographed E-Ink panel image
  --panel <sku>     panel SKU/model
  --url <url>       fetched content URL
  --ntp <iso8601>   NTP-set UTC time, e.g. 2026-11-21T16:00:00Z
  --refresh <sec>   refresh interval in seconds (positive integer)
  --out <path>      evidence Markdown (default RAVEN_MGMT_ROOT/tickets/evidence/pi4-eink/evidence.md)
  -h, --help        show this help

Every required field must be present for exit 0. A missing field is an error, not a
default. QEMU/virt/synthetic/fixture logs and images are refused. No board PASS is
claimed by this script; the owner must attest the photo and transcript.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --check)
            check_only=1
            shift
            ;;
        --dry-run)
            dry_run=1
            shift
            ;;
        --serial)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            serial=$2
            shift 2
            ;;
        --log)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            log=$2
            shift 2
            ;;
        --duration)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            duration=$2
            shift 2
            ;;
        --image)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            image=$2
            shift 2
            ;;
        --photo)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            photo=$2
            shift 2
            ;;
        --panel)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            panel=$2
            shift 2
            ;;
        --url)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            url=$2
            shift 2
            ;;
        --ntp)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            ntp=$2
            shift 2
            ;;
        --refresh)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            refresh=$2
            shift 2
            ;;
        --out)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            out=$2
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "pi4-eink-evidence: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

if [ -z "$out" ]; then
    out=$(mgmt_path tickets/evidence/pi4-eink/evidence.md) || exit 2
fi

require_file() {
    if [ ! -f "$1" ]; then
        echo "pi4-eink-evidence: missing required file: $1" >&2
        exit 2
    fi
}

host_checks() {
    mgmt_eink=$(mgmt_require_file architecture/product-data/0.1.0-eink-evidence.md) || exit 2
    require_file "$mgmt_eink"
    mgmt_readme=$(mgmt_require_file tickets/evidence/pi4-eink/README.md) || exit 2
    require_file "$mgmt_readme"
    require_file scripts/board-pi4-check.sh
    require_file scripts/pi4-serial.sh
    require_file scripts/pi4-eink-evidence.sh
    if [ -e "$out" ]; then
        echo "pi4-eink-evidence: evidence file present: $out"
        echo "pi4-eink-evidence: host check does not authenticate it; no board PASS claimed"
    else
        echo "pi4-eink-evidence: no board evidence file yet: $out (expected before the owner run)"
    fi
    echo "pi4-eink-evidence: required board fields: serial log, photo + SHA-256, panel SKU,"
    echo "pi4-eink-evidence: fetch URL, NTP ISO8601 time, refresh seconds, image + SHA-256"
    echo "pi4-eink-evidence: host check only; no board evidence claimed"
}

if [ "$check_only" -eq 1 ]; then
    host_checks
    exit 0
fi

if [ "$dry_run" -eq 1 ]; then
    host_checks
    cat <<EOF
pi4-eink-evidence: dry run; the board command is:
  sh scripts/pi4-eink-evidence.sh --serial /dev/ttyUSB0 --log build/pi4/eink.serial.log \\
      --duration $duration --image $image --photo board-eink.jpg --panel '<SKU>' \\
      --url 'https://<host>/<content>' --ntp '<YYYY-MM-DDTHH:MM:SSZ>' --refresh 60
pi4-eink-evidence: no serial port opened; no evidence file written
EOF
    exit 0
fi

refuse() {
    echo "pi4-eink-evidence: REFUSED: $1" >&2
    echo "pi4-eink-evidence: no board evidence claimed" >&2
    exit 2
}

case "$duration" in
    ''|*[!0-9]*)
        echo "pi4-eink-evidence: --duration must be a positive integer" >&2
        exit 2
        ;;
esac
if [ "$duration" -le 0 ]; then
    echo "pi4-eink-evidence: --duration must be positive" >&2
    exit 2
fi

if [ -n "$serial" ]; then
    if [ -z "$log" ]; then
        echo "pi4-eink-evidence: --serial requires --log" >&2
        exit 2
    fi
    if [ ! -c "$serial" ]; then
        echo "pi4-eink-evidence: not a character device: $serial" >&2
        exit 2
    fi
    mkdir -p "$(dirname -- "$log")"
    image_sha=missing
    if [ -f "$image" ]; then
        image_sha=$(sha256sum "$image" | awk '{print $1}')
    fi
    {
        echo "# Pi 4 E-Ink serial capture"
        echo "# started: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
        echo "# host: $(uname -srm 2>/dev/null || echo unknown)"
        echo "# commit: $(git rev-parse HEAD 2>/dev/null || echo unknown)"
        echo "# image: $image"
        echo "# image sha256: $image_sha"
        echo "# device: $serial at 115200 8N1"
    } > "$log"
    if command -v stty >/dev/null 2>&1; then
        stty -F "$serial" 115200 raw -echo 2>/dev/null || true
    fi
    timeout "$duration" cat "$serial" >> "$log" 2>/dev/null || true
    echo "# ended: $(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$log"
fi

if [ -z "$log" ] || [ ! -f "$log" ]; then
    echo "pi4-eink-evidence: a serial log is required (--serial/--log or --log)" >&2
    exit 2
fi
if [ ! -s "$log" ]; then
    refuse "serial log is empty; no Pi 4 board marker"
fi

# The same refusal rule as scripts/board-pi4-check.sh: a QEMU/virt or explicitly
# synthetic/fixture capture is not a board result.
if grep -Eiq 'qemu|tcg|emulat' "$log"; then
    refuse "serial log contains a QEMU/tcg/emulator marker"
fi
if grep -Eiq '^#.*(virt|virtual|synthetic|fixture|sample)' "$log"; then
    refuse "serial header marks the log as virt/virtual/synthetic/fixture/sample"
fi
if grep -Eiq '(machine model|platform)[^a-z]*(virt|qemu)' "$log"; then
    refuse "serial log identifies the machine as virt/QEMU"
fi
if ! grep -Eiq 'bcm2711|raspi4b|raspberry pi 4' "$log"; then
    refuse "no Pi 4 board marker (bcm2711/raspi4b/raspberry pi 4)"
fi

# Every evidence field is mandatory once any of them is supplied.
if [ -z "$photo" ] && [ -z "$panel" ] && [ -z "$url" ] && [ -z "$ntp" ] && [ -z "$refresh" ]; then
    echo "pi4-eink-evidence: serial log captured and board-marked, but evidence is incomplete" >&2
    echo "pi4-eink-evidence: supply --photo --panel --url --ntp --refresh to write $out" >&2
    exit 1
fi
if [ -z "$photo" ] || [ -z "$panel" ] || [ -z "$url" ] || [ -z "$ntp" ] || [ -z "$refresh" ]; then
    echo "pi4-eink-evidence: incomplete evidence fields: photo, panel, url, ntp and refresh are all required" >&2
    exit 2
fi
if [ ! -f "$photo" ]; then
    echo "pi4-eink-evidence: photo not found: $photo" >&2
    exit 2
fi
if printf '%s' "$photo" | grep -Eiq 'qemu|tcg|emulat|virt|synthetic|fixture|sample'; then
    refuse "photo path carries a QEMU/virt/synthetic/fixture marker"
fi
if grep -aEiq 'qemu|tcg|emulat|synthetic|fixture' "$photo" 2>/dev/null; then
    refuse "photo contains a QEMU/tcg/emulator/synthetic/fixture marker"
fi
case "$url" in
    http://*|https://*) ;;
    *)
        echo "pi4-eink-evidence: --url must be http(s)://..." >&2
        exit 2
        ;;
esac
if ! printf '%s\n' "$ntp" | grep -Eq '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(Z|[+-][0-9]{2}:[0-9]{2})$'; then
    echo "pi4-eink-evidence: --ntp must be an ISO8601 UTC timestamp" >&2
    exit 2
fi
case "$refresh" in
    ''|*[!0-9]*)
        echo "pi4-eink-evidence: --refresh must be a positive integer" >&2
        exit 2
        ;;
esac
if [ "$refresh" -le 0 ]; then
    echo "pi4-eink-evidence: --refresh must be positive" >&2
    exit 2
fi
if [ ! -f "$image" ]; then
    echo "pi4-eink-evidence: image not found: $image" >&2
    exit 2
fi

image_sha=$(sha256sum "$image" | awk '{print $1}')
photo_sha=$(sha256sum "$photo" | awk '{print $1}')
log_sha=$(sha256sum "$log" | awk '{print $1}')
commit=$(git rev-parse HEAD 2>/dev/null || echo unknown)
mkdir -p "$(dirname -- "$out")"
tmp="$out.tmp.$$"
trap 'rm -f "$tmp"' EXIT HUP INT TERM
{
    echo "# Pi 4 E-Ink board evidence"
    echo
    echo "- Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "- Commit: \`$commit\`"
    echo "- Image: \`$image\`"
    echo "- Image SHA-256: \`$image_sha\`"
    echo "- Serial log: \`$log\`"
    echo "- Serial SHA-256: \`$log_sha\`"
    echo "- Photo: \`$photo\`"
    echo "- Photo SHA-256: \`$photo_sha\`"
    echo "- Panel: \`$panel\`"
    echo "- Fetch URL: \`$url\`"
    echo "- NTP time: \`$ntp\`"
    echo "- Refresh seconds: \`$refresh\`"
    echo "- Owner attestation: **PENDING** — the owner must confirm the photo and serial are from the"
    echo "  physical Pi 4 and panel; this file is not a board PASS by itself."
} > "$tmp"
mv "$tmp" "$out"
trap - EXIT HUP INT TERM
cat <<EOF
pi4-eink-evidence: wrote $out
pi4-eink-evidence: photo sha256=$photo_sha
pi4-eink-evidence: serial sha256=$log_sha
pi4-eink-evidence: image sha256=$image_sha
pi4-eink-evidence: owner attestation remains PENDING; no board PASS claimed
EOF
exit 0
