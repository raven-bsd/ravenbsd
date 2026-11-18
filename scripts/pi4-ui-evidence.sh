#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# UI1 Pi 4 HDMI + USB keyboard board-evidence helper.
#
# --check   host image/manifest checks plus the current provider/hardware blockers; no board
# --plan    print the exact owner board sequence
# --capture capture the UART log (requires the owner's adapter and a board)
# --verify  check a real captured log and photo; never creates or fakes either
#
# The host path is safe to run without a Pi 4. A board PASS is only possible from --verify with
# the owner's real log and photo.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$root"

image=build/aarch64/ravenbsd-pi4-sd.img
device=/dev/ttyUSB0
log=
photo=
check=0
strict=0
plan=0
capture=0
verify=0

usage() {
    cat <<'EOF'
usage:
  scripts/pi4-ui-evidence.sh --check [-i IMAGE] [--strict]
  scripts/pi4-ui-evidence.sh --plan
  scripts/pi4-ui-evidence.sh --capture [-d TTY] [-l LOG] [-i IMAGE]
  scripts/pi4-ui-evidence.sh --verify --log LOG --photo PHOTO [-i IMAGE]

  -i, --image PATH   Pi 4 SD image (default: build/aarch64/ravenbsd-pi4-sd.img)
  -d, --device TTY   UART adapter (default: /dev/ttyUSB0)
  -l, --log PATH     serial log path (default: build/pi4/ui1-serial.log)
  -p, --photo PATH   owner photo/screenshot of the HDMI monitor
      --check        host image/manifest/provider checks; no board
      --strict       make --check exit 2 when a provider blocker exists (board absence alone is
                     reported but does not fail strict mode)
      --plan         print the board sequence
      --capture      capture the UART log; requires the owner's board and adapter
      --verify       verify a real serial log and photo; no fake generated
  -h, --help         show this help

Host check:
  sh scripts/pi4-ui-evidence.sh --check
  sh scripts/pi4-ui-evidence.sh --check --strict   # provider check; board absence is separate

Board sequence:
  sh scripts/pi4-ui-evidence.sh --plan

Verify the owner capture:
  sh scripts/pi4-ui-evidence.sh --verify --log build/pi4/ui1-serial.log \
      --photo /path/to/monitor.jpg
EOF
}

fail() {
    printf 'pi4-ui-evidence: FAIL %s\n' "$*" >&2
    exit 1
}

use_value() {
    [ "$#" -ge 2 ] || { usage >&2; exit 2; }
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        -i|--image) use_value "$@"; image=$2; shift 2 ;;
        -d|--device) use_value "$@"; device=$2; shift 2 ;;
        -l|--log) use_value "$@"; log=$2; shift 2 ;;
        -p|--photo) use_value "$@"; photo=$2; shift 2 ;;
        --check) check=1; shift ;;
        --strict) strict=1; shift ;;
        --plan) plan=1; shift ;;
        --capture) capture=1; shift ;;
        --verify) verify=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "pi4-ui-evidence: unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        fail "sha256sum or shasum is required"
    fi
}

print_plan() {
    cat <<'EOF'
UI1 board sequence
==================
Owner hardware: Pi 4, HDMI monitor + cable, USB keyboard, SD card, 3.3 V USB-UART adapter.
No VCC on the UART header. Record monitor model/native resolution, keyboard model, SD device,
adapter, image SHA and the exact log path.

1. Host: build the current SD image and run the host checks:
     RAVEN_RPI_FIRMWARE=/path/to/ravenbsd-resources/rpi-firmware/boot \
       SOURCE_DATE_EPOCH=1700000000 \
       cargo xtask image --arch aarch64 --platform pi4 --profile pi4-rootfs
     sh scripts/pi4-ui-evidence.sh --check
     make check-fast

2. Owner: flash the SD card and insert it (never reflash over a mounted/root disk):
     sudo sh scripts/pi4-install.sh --flash /dev/sdX --yes \
       -i build/aarch64/ravenbsd-pi4-sd.img
   Connect HDMI, the USB keyboard and the UART adapter (115200 8N1), then power on.

3. Capture the UART log (foreground; stop with Ctrl-C and keep the footer):
     sh scripts/pi4-ui-evidence.sh --capture -d /dev/ttyUSB0 \
       -l build/pi4/ui1-serial.log

4. HDMI/keyboard session (record a photo when the monitor shows the login/prompt):
     - kernel banner text on the monitor
     - wrong password once, then the correct root/raven login:
         login: root
         Password: wrong
         Login incorrect
         login: root
         Password: raven
     - raven# echo UI1-HDMI-OK
     - raven# echo "UI1-TERM=$TERM"
     - raven# echo "UI1-PS1=$PS1"
     - raven# ls --color=always /bin
     - raven# ping 8.8.8.8
     - take a photo of the monitor with the prompt and output visible

5. UART fallback (same serial log): log in over UART and prove it still works:
     login: root
     Password: raven
     raven# echo UI1-UART-OK
     raven# echo "UI1-TERM=$TERM"

6. Verify the owner capture:
     sh scripts/pi4-ui-evidence.sh --verify --log build/pi4/ui1-serial.log \
       --photo /path/to/monitor.jpg

Current host-only blockers (see --check for the exact lines):
  * no HDMI/login userspace fb:console provider: HDMI shows kernel text, getty/login stays on UART;
  * no board/UART adapter attached on this host.
The kbd:console provider is aligned: usb-hid serves the `console` instance and consd opens it;
UART remains the recovery input.
Hand the first failure to HDMI1 (fb provider) or the owner hardware.
No fixture, generated log or borrowed photo substitutes for the owner capture.
EOF
}

host_check() {
    [ -f "$image" ] || fail "image missing: $image (build it first; see --plan)"
    command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1 ||
        fail "sha256sum or shasum is required"

    echo "pi4-ui-evidence: image=$image"
    echo "pi4-ui-evidence: commit=$(git rev-parse HEAD 2>/dev/null || echo unknown)"
    echo "pi4-ui-evidence: image_sha256=$(sha256_file "$image")"

    sh scripts/pi4-release.sh --check -i "$image" > "$root/build/ui1-release-check.log" 2>&1 ||
        { tail -20 "$root/build/ui1-release-check.log" >&2; fail "pi4-release --check failed"; }
    tail -6 "$root/build/ui1-release-check.log"
    sh scripts/pi4-serial.sh --check -i "$image" > "$root/build/ui1-serial-check.log" 2>&1 ||
        { tail -20 "$root/build/ui1-serial-check.log" >&2; fail "pi4-serial --check failed"; }
    tail -4 "$root/build/ui1-serial-check.log"

    board_blocked=0
    provider_blocked=0
    if ls /dev/ttyUSB* /dev/ttyACM* >/dev/null 2>&1; then
        echo "pi4-ui-evidence: uart=$(ls /dev/ttyUSB* /dev/ttyACM* 2>/dev/null | tr '\n' ' ')"
    else
        echo "pi4-ui-evidence: board BLOCKED: no /dev/ttyUSB* or /dev/ttyACM* adapter"
        board_blocked=1
    fi
    echo "pi4-ui-evidence: block devices (owner chooses the SD card; do not guess):"
    lsblk -o NAME,SIZE,TYPE,MOUNTPOINTS 2>/dev/null | sed -n '1,20p' || true

    consd=userland/servers/consd/src/main.rs
    usbhid=userland/drivers/usb-hid/src/main.rs
    usbhid_lib=userland/drivers/usb-hid/src/lib.rs
    if grep -Fq 'fb:console' "$consd"; then
        if grep -R -Fq 'register("fb' userland 2>/dev/null; then
            echo "pi4-ui-evidence: provider fb:console present (userspace)"
        elif grep -Fq 'super::fb::write_text' kernel/src/arch/aarch64/serial.rs 2>/dev/null &&
             grep -Fq 'pub fn write_text' kernel/src/arch/aarch64/fb.rs 2>/dev/null; then
            echo "pi4-ui-evidence: provider fb:console present (kernel DEBUG_WRITE mirror)"
        else
            echo "pi4-ui-evidence: provider BLOCKED: no fb:console provider and no DEBUG_WRITE mirror; login stays on UART"
            provider_blocked=1
        fi
    fi
    if grep -Fq 'kbd:console' "$consd"; then
        if grep -Fq 'KBD_CONSOLE_PATH' "$usbhid_lib" && grep -Fq 'accepts_console_path' "$usbhid"; then
            echo "pi4-ui-evidence: provider kbd:console present"
        else
            echo "pi4-ui-evidence: provider BLOCKED: consd opens kbd:console but usb-hid has no console path"
            provider_blocked=1
        fi
    else
        echo "pi4-ui-evidence: provider BLOCKED: consd does not open kbd:console"
        provider_blocked=1
    fi

    if [ "$provider_blocked" -ne 0 ]; then
        echo "pi4-ui-evidence: host image checks PASS; provider BLOCKED (see lines above)"
        if [ "$strict" -eq 1 ]; then
            return 2
        fi
        return 0
    fi
    if [ "$board_blocked" -ne 0 ]; then
        echo "pi4-ui-evidence: host image checks PASS; provider checks PASS; board evidence BLOCKED (owner hardware/photo)"
        return 0
    fi
    echo "pi4-ui-evidence: host image checks PASS; board hardware present"
    return 0
}

capture_log() {
    [ -e "$device" ] || fail "UART adapter not found: $device"
    [ -f "$image" ] || fail "image missing: $image"
    log=${log:-build/pi4/ui1-serial.log}
    mkdir -p "$(dirname "$log")" 2>/dev/null || true
    echo "pi4-ui-evidence: capturing $device at 115200 8N1 to $log; Ctrl-C to stop"
    sh scripts/pi4-serial.sh -d "$device" -b 115200 -i "$image" -l "$log"
}

verify_log() {
    [ -n "$log" ] || fail "--verify needs --log LOG"
    [ -n "$photo" ] || fail "--verify needs --photo PHOTO"
    [ -s "$log" ] || fail "serial log is empty or missing: $log"
    [ -s "$photo" ] || fail "photo is empty or missing: $photo"

    failed=0
    gate() {
        if grep -Fq -- "$2" "$log"; then
            echo "pi4-ui-evidence: gate $1 PASS"
        else
            echo "pi4-ui-evidence: gate $1 MISSING"
            failed=1
        fi
    }

    gate kernel 'RavenBSD kernel'
    gate login 'login:'
    gate wrong-password 'Login incorrect'
    gate prompt 'raven#'
    gate hdmi-session 'UI1-HDMI-OK'
    gate uart-fallback 'UI1-UART-OK'
    gate brush-term 'UI1-TERM='
    gate brush-ps1 'UI1-PS1='
    gate ls-bin 'ls /bin'
    gate brush-binary 'brush'
    gate ping-target '8.8.8.8'
    gate ping-reply 'bytes from'

    if grep -Eiq 'panic|EXCEPTION' "$log"; then
        echo "pi4-ui-evidence: forbidden panic/EXCEPTION line in the log"
        failed=1
    fi
    echo "pi4-ui-evidence: log=$log sha256=$(sha256_file "$log")"
    echo "pi4-ui-evidence: photo=$photo sha256=$(sha256_file "$photo")"
    if [ "$failed" -ne 0 ]; then
        fail "board evidence incomplete; preserve the first missing gate"
    fi
    echo "pi4-ui-evidence: board evidence PASS"
}

if [ "$plan" -eq 1 ]; then
    print_plan
    exit 0
fi
if [ "$check" -eq 1 ]; then
    host_check
    exit $?
fi
if [ "$capture" -eq 1 ]; then
    capture_log
    exit 0
fi
if [ "$verify" -eq 1 ]; then
    verify_log
    exit 0
fi
usage >&2
exit 2
