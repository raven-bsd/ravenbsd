#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Capture and check a Pi 4 integrated GENET/PWM soak serial log, or write the UNVERIFIED board
# evidence template. This helper changes no driver and never claims a hardware result: it only
# verifies the markers a board run must produce.
#
# Usage:
#   scripts/pi4-soak.sh <serial.log>
#   scripts/pi4-soak.sh --capture <tty> <serial.log> [seconds]
#   scripts/pi4-soak.sh --template <path>

set -eu

usage() {
    cat >&2 <<'EOF'
usage: scripts/pi4-soak.sh <serial.log>
       scripts/pi4-soak.sh --capture <tty> <serial.log> [seconds]
       scripts/pi4-soak.sh --template <path>

  <serial.log>        existing UART log to check
  --capture           read a UART adapter with stty/cat for [seconds] (default 600)
  --template <path>   write the UNVERIFIED board evidence template and exit

Required log markers: pi4-soak: start, pi4-soak: genet, pi4-soak: recovery,
pi4-soak: audio, pi4-soak: pass.
EOF
    exit "${1:-2}"
}

log=
template=
capture=0
device=
seconds=600

while [ "$#" -gt 0 ]; do
    case "$1" in
        --capture)
            [ "$#" -ge 3 ] || usage
            capture=1
            device=$2
            log=$3
            if [ "$#" -ge 4 ]; then
                seconds=$4
                shift 4
            else
                shift 3
            fi
            ;;
        --template)
            [ "$#" -ge 2 ] || usage
            template=$2
            shift 2
            ;;
        -h|--help)
            usage 0
            ;;
        --*)
            usage
            ;;
        *)
            [ -z "$log" ] || usage
            log=$1
            shift
            ;;
    esac
done

if [ -n "$template" ]; then
    [ -z "$log" ] || usage
    if [ -e "$template" ]; then
        echo "pi4-soak-evidence: template already exists: $template" >&2
        exit 1
    fi
    mkdir -p "$(dirname -- "$template")"
    cat >"$template" <<'EOF'
# Pi 4 integrated soak evidence

Status: UNVERIFIED template. Fill every field from a real board log and attach the log.

- Date (UTC):
- Owner:
- Board/revision:
- Boot image commit:
- Boot image sha256:
- UART log path:
- UART log sha256:
- UART adapter and baud:
- `config.txt` (`dtparam=audio=off`, `dtoverlay=pwm1,pins_40_41,pull_off`):
- GENET link speed/duplex and peer:
- Audio cable/jack and line-in capture device:
- Capture gain and format:

## Required UART markers

```
pi4-soak: start iterations=...
pi4-soak: genet rx=... tx=... owned=... rx_errors=... tx_errors=... link_events=... irq=...
pi4-soak: recovery resets=... faults=...
pi4-soak: audio periods=... frames=... underruns=... writes=... bytes=...
pi4-soak: pass
```

## Observed counters

| Counter | Observed | Expected / note |
|---|---|---|
| RX completions | | equal to iterations |
| TX completions | | iterations minus malformed TX |
| Still-owned checks | | non-zero, each must recover |
| RX errors | | injected CRC completions |
| TX errors | | injected malformed completions |
| Link events | | debounced up/down transitions |
| IRQ decode checks | | combined RX/TX status |
| Recovery resets / faults | | 3 resets then 1 fault in the model |
| Audio PCM periods | | iterations minus injected underruns |
| Audio frames | | periods x 1024 |
| Audio underruns | | injected silence periods |
| DMA writes / bytes | | driver-reported |

## Audio line-in check

- 1 kHz at -6 dBFS, 48 kHz:
- 1 kHz at -6 dBFS, 44.1 kHz:
- Left-only / right-only channel order:
- Clicks, hum, DC offset:

## Verdict

UNVERIFIED until the UART log, the live register dump, the audio recording/spectrogram and the
final `config.txt` are attached to the ticket report.
EOF
    echo "pi4-soak-evidence: template=$template (UNVERIFIED)"
    exit 0
fi

[ -n "$log" ] || usage

if [ "$capture" -eq 1 ]; then
    command -v stty >/dev/null 2>&1 || { echo "stty not found" >&2; exit 2; }
    command -v timeout >/dev/null 2>&1 || { echo "timeout not found" >&2; exit 2; }
    [ -c "$device" ] || { echo "pi4-soak-evidence: not a character device: $device" >&2; exit 1; }
    stty -F "$device" raw 115200 cs8 -cstopb -parenb -crtscts -echo
    timeout "$seconds" cat "$device" >"$log" || true
fi

[ -s "$log" ] || { echo "pi4-soak-evidence: log is empty or missing: $log" >&2; exit 1; }

repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
commit=$(git -C "$repo" rev-parse HEAD 2>/dev/null || printf 'unknown')
if command -v sha256sum >/dev/null 2>&1; then
    hash=$(sha256sum "$log" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
    hash=$(shasum -a 256 "$log" | awk '{print $1}')
else
    echo "pi4-soak-evidence: sha256sum or shasum is required" >&2
    exit 2
fi

echo "pi4-soak-evidence: commit=$commit"
echo "pi4-soak-evidence: log=$log sha256=$hash"

missing=0
for marker in 'pi4-soak: start' 'pi4-soak: genet' 'pi4-soak: recovery' 'pi4-soak: audio' 'pi4-soak: pass'; do
    if ! grep -Fq -- "$marker" "$log"; then
        echo "pi4-soak-evidence: missing marker $marker"
        missing=1
    fi
done
if grep -Fq -- 'pi4-soak: failed' "$log"; then
    echo "pi4-soak-evidence: soak reported a failure"
    missing=1
fi
if grep -Eiq 'panic|EXCEPTION' "$log"; then
    echo "pi4-soak-evidence: panic or exception marker found"
    missing=1
fi

grep -F -- 'pi4-soak:' "$log" || true

if [ "$missing" -ne 0 ]; then
    echo "pi4-soak-evidence: FAIL"
    exit 1
fi
echo "pi4-soak-evidence: PASS"
