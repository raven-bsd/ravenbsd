#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Pi 4 full test hammer: run the complete Pi 4 host matrix and record one table, or run the
# board capture/evidence commands on the owner's machine.
#
# Usage:
#   sh scripts/pi4-hammer.sh --host [--out docs/pi4-test-matrix.md]
#   sh scripts/pi4-hammer.sh --board [--device /dev/ttyUSB0] [--log PATH]
#
# --host builds the aarch64 kernel and the `pi4` image, runs every Pi 4 qtest and host package,
# then `make check-fast`, and rewrites the matrix. A missing qtest case is a SKIP with the owning
# ticket, not a pass. --board never claims a hardware result without a captured log; without a
# device or log it writes the UNVERIFIED procedure and exits 0.
#
# The script only reads the tree and writes `docs/pi4-test-matrix.md` (host), the board template
# under `build/pi4-hammer/`, and per-step logs under `build/pi4-hammer/`.

set -u

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
ROOT_PHYSICAL=$(cd "$ROOT" && pwd -P)
cd "$ROOT" || {
    echo "pi4-hammer: cannot enter repository root from $0" >&2
    exit 2
}
. "$ROOT/scripts/lib/mgmt.sh"

WORK=build/pi4-hammer
RESULTS=$WORK/results.tsv
HOST_OUT=
BOARD_OUT=$WORK/board-matrix.md
IMAGE=build/aarch64/ravenbsd-pi4.img

MODE=
OUT=
DEVICE=
BOARD_LOG=
FAILURES=0
SKIPS=0
FIRMWARE=

usage() {
    cat <<'EOF'
usage:
  sh scripts/pi4-hammer.sh --host [--out PATH]
  sh scripts/pi4-hammer.sh --board [--device TTY] [--log PATH]

  --host       build the aarch64 kernel and pi4 image, run the Pi 4 qtests and host packages,
               run make check-fast, and write the matrix under RAVEN_MGMT_ROOT
  --board      capture/check the Pi 4 board evidence; no device or log writes the UNVERIFIED
               procedure only
  --out PATH   matrix output path (host default:
               RAVEN_MGMT_ROOT/tickets/evidence/pi4-test-matrix.md,
               board default: build/pi4-hammer/board-matrix.md)
  --device TTY serial adapter for --board capture (default: /dev/ttyUSB0 when --board runs)
  --log PATH   serial log for --board (captures to build/pi4-hammer/serial.log when --device is set)
  -h, --help   show this help
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --host)
            MODE=host
            ;;
        --board)
            MODE=board
            ;;
        --out)
            shift
            OUT=${1:-}
            [ -n "$OUT" ] || usage >&2
            ;;
        --device)
            shift
            DEVICE=${1:-}
            [ -n "$DEVICE" ] || usage >&2
            ;;
        --log)
            shift
            BOARD_LOG=${1:-}
            [ -n "$BOARD_LOG" ] || usage >&2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "pi4-hammer: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
    shift
done

if [ -z "$MODE" ]; then
    echo "pi4-hammer: one of --host or --board is required" >&2
    usage >&2
    exit 2
fi

if [ -z "$OUT" ] && [ "$MODE" = host ]; then
    HOST_OUT=$(mgmt_path tickets/evidence/pi4-test-matrix.md) || exit 2
fi

mkdir -p "$WORK" || exit 2

# The firmware lives in the read-only workspace resources, not in the clone. xtask looks beside
# the physical clone; a minion checkout can sit on another disk, so also check the workspace
# convention and an explicit override.
find_firmware() {
    if [ -n "${RAVEN_RPI_FIRMWARE:-}" ]; then
        if [ -d "$RAVEN_RPI_FIRMWARE" ]; then
            FIRMWARE=$RAVEN_RPI_FIRMWARE
            return 0
        fi
        echo "pi4-hammer: RAVEN_RPI_FIRMWARE is not a directory: $RAVEN_RPI_FIRMWARE" >&2
        return 1
    fi
    for candidate in \
        "$ROOT/../ravenbsd-resources/rpi-firmware/boot" \
        "$ROOT/../../ravenbsd-resources/rpi-firmware/boot" \
        "$ROOT/../../../ravenbsd-resources/rpi-firmware/boot" \
        "$ROOT/../../../../ravenbsd-resources/rpi-firmware/boot" \
        "${HOME:-/nonexistent}/work/ravenos-workspace/ravenbsd-resources/rpi-firmware/boot" \
        "${HOME:-/nonexistent}/ravenbsd-resources/rpi-firmware/boot"
    do
        if [ -d "$candidate" ]; then
            FIRMWARE=$candidate
            RAVEN_RPI_FIRMWARE=$candidate
            export RAVEN_RPI_FIRMWARE
            return 0
        fi
    done
    if [ -n "${RAVENBSD_WORKSPACE:-}" ] &&
        [ -d "$RAVENBSD_WORKSPACE/ravenbsd-resources/rpi-firmware/boot" ]; then
        FIRMWARE=$RAVENBSD_WORKSPACE/ravenbsd-resources/rpi-firmware/boot
        RAVEN_RPI_FIRMWARE=$FIRMWARE
        export RAVEN_RPI_FIRMWARE
        return 0
    fi
    return 1
}

slugify() {
    printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9._-]/-/g; s/--*/-/g; s/^-//; s/-$//'
}

last_nonempty() {
    awk 'NF { line = $0 } END { if (line != "") print line }' "$1"
}

md_escape() {
    printf '%s' "$1" | tr '\n\t' '  ' |
        sed 's/^[[:space:]]*//; s/[[:space:]]*$//; s/|/\\|/g'
}

qtest_result_line() {
    awk '/^--- result ---$/ { getline; print; exit }' "$1"
}

record() {
    rec_section=$1
    rec_name=$2
    rec_arch=$3
    rec_owner=$4
    rec_status=$5
    rec_command=$6
    rec_evidence=$7
    rec_detail=$8
    rec_log=$9
    rec_evidence=$(md_escape "$rec_evidence")
    rec_detail=$(md_escape "$rec_detail")
    rec_log=$(md_escape "$rec_log")
    rec_evidence=$(printf '%s' "$rec_evidence" | sed "s|$ROOT/||g; s|$ROOT_PHYSICAL/||g")
    rec_detail=$(printf '%s' "$rec_detail" | sed "s|$ROOT/||g; s|$ROOT_PHYSICAL/||g")
    rec_log=$(printf '%s' "$rec_log" | sed "s|$ROOT/||g; s|$ROOT_PHYSICAL/||g")
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$rec_section" "$rec_name" "$rec_arch" "$rec_owner" "$rec_status" \
        "$rec_command" "$rec_evidence" "$rec_detail" "$rec_log" >>"$RESULTS"
}

run_command_step() {
    step_section=$1
    step_name=$2
    step_arch=$3
    step_owner=$4
    step_required=$5
    step_command=$6
    step_pass=$7
    step_slug=$(slugify "$step_section-$step_name")
    step_log="$WORK/$step_slug.log"

    printf '[ RUN  ] %s\n' "$step_command"
    sh -c "$step_command" >"$step_log" 2>&1 </dev/null
    step_code=$?
    if [ "$step_code" -eq 0 ]; then
        step_status=PASS
        step_evidence=$(grep -E "$step_pass" "$step_log" 2>/dev/null | head -n 1)
        [ -n "$step_evidence" ] || step_evidence=$(last_nonempty "$step_log")
        step_detail=
    elif [ "$step_code" -eq 77 ]; then
        step_status=SKIP
        step_evidence=$(qtest_result_line "$step_log")
        [ -n "$step_evidence" ] || step_evidence=$(last_nonempty "$step_log")
        SKIPS=$((SKIPS + 1))
        step_detail=
    else
        step_status=FAIL
        step_detail=$(qtest_result_line "$step_log")
        [ -n "$step_detail" ] || step_detail=$(last_nonempty "$step_log")
        step_evidence="FAIL (see Failures)"
        if [ "$step_required" = yes ]; then
            FAILURES=$((FAILURES + 1))
        fi
    fi

    record "$step_section" "$step_name" "$step_arch" "$step_owner" "$step_status" \
        "$step_command" "$step_evidence" "$step_detail" "$step_log"
    printf '[ %s ] %s\n' "$step_status" "$step_name"
    if [ "$step_status" = FAIL ]; then
        printf '         %s\n' "$step_detail"
    fi
}

run_case() {
    case_name=$1
    case_arch=$2
    case_owner=$3
    case_command="cargo xtask qtest --case $case_name --arch $case_arch"
    if [ ! -f "tests/qtest/cases/$case_name.toml" ]; then
        record qtest "$case_name" "$case_arch" "$case_owner" SKIP "$case_command" \
            "case file missing: owning ticket not merged" "" ""
        printf '[ SKIP ] %s: tests/qtest/cases/%s.toml is missing\n' "$case_name" "$case_name"
        SKIPS=$((SKIPS + 1))
        return
    fi
    run_command_step qtest "$case_name" "$case_arch" "$case_owner" yes "$case_command" \
        "^$case_name: pass$"
}

run_host_test() {
    test_label=$1
    test_package=$2
    test_owner=$3
    run_command_step hosttest "$test_label" "" "$test_owner" yes \
        "cargo test -p $test_package" '^test result: ok\.'
}

image_sha() {
    if [ -f "$IMAGE" ]; then
        if command -v sha256sum >/dev/null 2>&1; then
            sha256sum "$IMAGE" | awk '{print $1}'
        else
            printf 'unavailable\n'
        fi
    else
        printf 'not built\n'
    fi
}

write_host_matrix() {
    matrix_out=${OUT:-$HOST_OUT}
    matrix_tmp=$WORK/.host-matrix.$$
    matrix_commit=$(git rev-parse --short=12 HEAD 2>/dev/null || printf 'unknown')
    matrix_host=$(uname -srm 2>/dev/null || printf 'unknown')
    matrix_rust=$(rustc --version 2>/dev/null || printf 'unavailable')
    matrix_qemu=$(qemu-system-aarch64 --version 2>/dev/null | sed -n '1p')
    [ -n "$matrix_qemu" ] || matrix_qemu=unavailable
    matrix_image_sha=$(image_sha)
    matrix_total=$(awk -F '\t' '$1 == "qtest" { n++ } END { print n + 0 }' "$RESULTS")
    matrix_pass=$(awk -F '\t' '$1 == "qtest" && $5 == "PASS" { n++ } END { print n + 0 }' "$RESULTS")
    matrix_skip=$(awk -F '\t' '$1 == "qtest" && $5 == "SKIP" { n++ } END { print n + 0 }' "$RESULTS")
    matrix_fail=$(awk -F '\t' '$1 == "qtest" && $5 == "FAIL" { n++ } END { print n + 0 }' "$RESULTS")
    if [ "$FAILURES" -eq 0 ]; then
        matrix_result=PASS
    else
        matrix_result=FAIL
    fi

    mkdir -p "$(dirname -- "$matrix_out")" || return 1
    {
        printf '<!-- SPDX-License-Identifier: BSD-3-Clause -->\n'
        printf '<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n\n'
        printf '# Pi 4 test matrix\n\n'
        printf 'Generated by `scripts/pi4-hammer.sh --host` at %s on product base commit `%s`.\n\n' \
            "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$matrix_commit"
        printf '**Required result: %s.** Pi 4 qtests: %s total, %s pass, %s fail, %s skip. ' \
            "$matrix_result" "$matrix_total" "$matrix_pass" "$matrix_fail" "$matrix_skip"
        printf 'Board results are UNVERIFIED until the owner runs `--board` on a Pi 4.\n\n'
        printf '## Environment\n\n'
        printf -- '- Host: `%s`\n' "$matrix_host"
        printf -- '- Rust: `%s`\n' "$matrix_rust"
        printf -- '- QEMU aarch64: `%s`\n' "$matrix_qemu"
        printf -- '- Pi 4 image: `%s` sha256 `%s`\n' "$IMAGE" "$matrix_image_sha"
        if [ -n "$FIRMWARE" ]; then
            case "$FIRMWARE" in
                */ravenbsd-resources/rpi-firmware/boot)
                    matrix_firmware=ravenbsd-resources/rpi-firmware/boot
                    ;;
                *)
                    matrix_firmware=$FIRMWARE
                    ;;
            esac
            printf -- '- Firmware: `%s` (external read-only checkout)\n' "$matrix_firmware"
        else
            printf -- '- Firmware: not found; set `RAVEN_RPI_FIRMWARE`\n'
        fi
        printf '\n## Build and image\n\n'
        printf '| Step | Owner | Command | Result | Evidence |\n'
        printf '|---|---|---|---|---|\n'
        awk -F '\t' '$1 == "build" {
            evidence = ($7 == "" ? "-" : $7)
            printf "| %s | %s | `%s` | **%s** | %s |\n", $2, $4, $6, $5, evidence
        }' "$RESULTS"
        printf '\n## Pi 4 qtests\n\n'
        printf '| Case | Arch | Owner | Command | Result | Evidence |\n'
        printf '|---|---|---|---|---|---|\n'
        awk -F '\t' '$1 == "qtest" {
            evidence = ($7 == "" ? "-" : $7)
            printf "| `%s` | %s | %s | `%s` | **%s** | %s |\n", $2, $3, $4, $6, $5, evidence
        }' "$RESULTS"
        printf '\nA SKIP row means the case file is absent on this base; the hammer does not treat a\n'
        printf 'missing case as a pass. The owning ticket in the row is the follow-up.\n'
        printf '\n## Host packages\n\n'
        printf '| Package | Owner | Command | Result | Evidence |\n'
        printf '|---|---|---|---|---|\n'
        awk -F '\t' '$1 == "hosttest" {
            evidence = ($7 == "" ? "-" : $7)
            printf "| %s | %s | `%s` | **%s** | %s |\n", $2, $4, $6, $5, evidence
        }' "$RESULTS"
        printf '\n## Ticket gate\n\n'
        printf '| Step | Owner | Command | Result | Evidence |\n'
        printf '|---|---|---|---|---|\n'
        awk -F '\t' '$1 == "gate" {
            evidence = ($7 == "" ? "-" : $7)
            printf "| %s | %s | `%s` | **%s** | %s |\n", $2, $4, $6, $5, evidence
        }' "$RESULTS"
        printf '\n## Failures\n\n'
        if [ "$FAILURES" -eq 0 ]; then
            printf 'No required step failed. A missing qtest case is recorded as SKIP above.\n'
        else
            awk -F '\t' '$5 == "FAIL" {
                detail = ($8 == "" ? $7 : $8)
                printf "### `%s` (%s)\n\n", $2, $1
                printf "- Owner: %s\n", $4
                printf "- Smallest reproducer: `%s`\n", $6
                printf "- Exact output line: `%s`\n", detail
                printf "- Log: `%s`\n\n", $9
            }' "$RESULTS"
        fi
        printf '\n'
        cat <<'EOF'
## Board evidence (UNVERIFIED)

`--host` never opens a serial port. On the owner's machine run:

```sh
sh scripts/pi4-hammer.sh --board --device /dev/ttyUSB0
```

| Step | Owner | Exact command | Status |
|---|---|---|---|
| Serial capture and FAT manifest | jerry/J3 | `scripts/pi4-serial.sh -d /dev/ttyUSB0 -i build/aarch64/ravenbsd-pi4.img` | UNVERIFIED |
| GENET board markers | otto/O4 | `scripts/pi4-genet-evidence.sh build/pi4/<image-sha256>.log` | UNVERIFIED |
| GENET/audio soak markers | otto/O6 | `scripts/pi4-soak.sh build/pi4/<image-sha256>.log` | UNVERIFIED |
| Root mount | jerry/J5 | capture the log and look for `svcd: root mounted at /root` and `fsd-ufs: mounted block:` | UNVERIFIED (pi4-root case absent: J5 is blocked) |
| PWM audio register read-backs | otto/B46 | follow the Pi 4 audio note section 5 and record every register/counter | UNVERIFIED |

A board row becomes PASS only when the captured log contains the helper's required markers. No
line in this section is a hardware result until the owner attaches the log and audio recording.
EOF
    } >"$matrix_tmp"
    mv "$matrix_tmp" "$matrix_out"
    printf 'pi4-hammer: matrix=%s\n' "$matrix_out"
}

host_mode() {
    : >"$RESULTS"
    if find_firmware; then
        printf 'pi4-hammer: firmware=%s\n' "$FIRMWARE"
    else
        printf 'pi4-hammer: warning: Raspberry Pi firmware not found; the pi4 image step will fail\n' >&2
    fi

    run_command_step build "aarch64 kernel" aarch64 "dave/S13" yes \
        "cargo xtask build kernel --arch aarch64" 'Finished'
    run_command_step build "pi4 image" aarch64 "jerry/K35" yes \
        "cargo xtask image --arch aarch64 --platform pi4 --profile pi4" 'ravenbsd-pi4\.img'
    if [ -f "$IMAGE" ]; then
        run_command_step build "pi4 image manifest" aarch64 "jerry/J3" yes \
            "scripts/pi4-serial.sh --check -i $IMAGE" 'pi4-manifest: PASSED'
    else
        record build "pi4 image manifest" aarch64 "jerry/J3" FAIL \
            "scripts/pi4-serial.sh --check -i $IMAGE" "FAIL (see Failures)" \
            "image not produced" ""
        FAILURES=$((FAILURES + 1))
        printf '[ FAIL ] pi4 image manifest: image not produced\n'
    fi

    while read -r case_name case_arch case_owner; do
        [ -n "$case_name" ] || continue
        run_case "$case_name" "$case_arch" "$case_owner"
    done <<'EOF'
boot-uname-aarch64 aarch64 dave/S14c
kernel-early-boot-aarch64 aarch64 dave/S13
kernel-exc-aarch64 aarch64 dave/S13
kernel-paging-aarch64 aarch64 dave/S14a
kernel-timer-aarch64 aarch64 dave/S14b
driver-aarch64 aarch64 dave/S21d
scheme-aarch64 aarch64 dave/B47a
sched-kernel-threads-aarch64 aarch64 dave/S14b
aarch64-libc aarch64 stuart/S56
pi4-devd aarch64 dave/K32
pi4-emmc2 aarch64 jerry/J4
pi4-root aarch64 jerry/J5
EOF

    while read -r test_label test_package test_owner; do
        [ -n "$test_label" ] || continue
        run_host_test "$test_label" "$test_package" "$test_owner"
    done <<'EOF'
raven-pi4 raven-pi4 jerry/J1
emmc2 emmc2 jerry/J4
raven-genet raven-genet otto/O1
genet genet otto/O4
raven-pwm raven-pwm otto/B46
pwm-audio pwm-audio otto/B46
raven-plat raven-plat dave/B47b
platd raven-platd dave/B47b
EOF

    run_command_step gate "make check-fast" "" "jerry/J6" yes \
        "make check-fast" 'check-fast: PASSED'

    write_host_matrix || {
        echo "pi4-hammer: cannot write the matrix" >&2
        exit 2
    }

    if [ "$FAILURES" -eq 0 ]; then
        printf 'pi4-hammer: PASS\n'
    else
        printf 'pi4-hammer: FAIL (%s required step(s))\n' "$FAILURES"
    fi
    printf 'pi4-hammer: logs=%s\n' "$WORK"
    [ "$FAILURES" -eq 0 ]
}

board_check() {
    board_label=$1
    board_command=$2
    board_slug=$(slugify "board-$board_label")
    board_out=$WORK/$board_slug.log
    printf '[ BOARD ] %s\n' "$board_command"
    sh -c "$board_command" >"$board_out" 2>&1 </dev/null
    board_code=$?
    if [ "$board_code" -eq 0 ]; then
        board_status=PASS
    else
        board_status=FAIL
    fi
    board_evidence=$(last_nonempty "$board_out")
    printf '[ %s ] %s: %s\n' "$board_status" "$board_label" "$board_evidence"
}

write_board_matrix() {
    matrix_out=${OUT:-$BOARD_OUT}
    matrix_tmp=$WORK/.board-matrix.$$
    matrix_commit=$(git rev-parse --short=12 HEAD 2>/dev/null || printf 'unknown')
    SERIAL_EVIDENCE=$(md_escape "$SERIAL_EVIDENCE")
    GENET_EVIDENCE=$(md_escape "$GENET_EVIDENCE")
    SOAK_EVIDENCE=$(md_escape "$SOAK_EVIDENCE")
    ROOT_EVIDENCE=$(md_escape "$ROOT_EVIDENCE")
    AUDIO_EVIDENCE=$(md_escape "$AUDIO_EVIDENCE")
    mkdir -p "$(dirname -- "$matrix_out")" || return 1
    {
        printf '<!-- SPDX-License-Identifier: BSD-3-Clause -->\n'
        printf '<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n\n'
        printf '# Pi 4 board matrix\n\n'
        printf 'Generated by `scripts/pi4-hammer.sh --board` at %s on product base commit `%s`.\n\n' \
            "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$matrix_commit"
        printf 'A row is PASS only when the captured log contains the helper markers; everything\n'
        printf 'else stays UNVERIFIED. Hardware results are the owner'"'"'s to claim.\n\n'
        printf '| Step | Owner | Command | Status | Evidence |\n'
        printf '|---|---|---|---|---|\n'
        printf '| Serial capture | jerry/J3 | `scripts/pi4-serial.sh -d %s -i %s` | **%s** | %s |\n' \
            "${DEVICE:-/dev/ttyUSB0}" "$IMAGE" "$SERIAL_STATUS" "$SERIAL_EVIDENCE"
        printf '| GENET markers | otto/O4 | `scripts/pi4-genet-evidence.sh %s` | **%s** | %s |\n' \
            "${BOARD_LOG:-<log>}" "$GENET_STATUS" "$GENET_EVIDENCE"
        printf '| GENET/audio soak | otto/O6 | `scripts/pi4-soak.sh %s` | **%s** | %s |\n' \
            "${BOARD_LOG:-<log>}" "$SOAK_STATUS" "$SOAK_EVIDENCE"
        printf '| Root mount markers | jerry/J5 | inspect `%s` | **%s** | %s |\n' \
            "${BOARD_LOG:-<log>}" "$ROOT_STATUS" "$ROOT_EVIDENCE"
        printf '| PWM audio register read-backs | otto/B46 | `the Pi 4 audio note` section 5 | **%s** | %s |\n' \
            "$AUDIO_STATUS" "$AUDIO_EVIDENCE"
        printf '\n## Board procedure\n\n'
        printf '```sh\n'
        printf 'RAVEN_RPI_FIRMWARE=/path/to/rpi-firmware/boot \\\n'
        printf '    cargo xtask image --arch aarch64 --platform pi4 --profile pi4\n'
        printf 'sh scripts/pi4-hammer.sh --board --device /dev/ttyUSB0\n'
        printf '```\n\n'
        printf 'Stop the capture with Ctrl-C after the shell or the first failure. Then run\n'
        printf '`scripts/pi4-genet-evidence.sh` and `scripts/pi4-soak.sh` on the same log.\n'
    } >"$matrix_tmp"
    mv "$matrix_tmp" "$matrix_out"
    printf 'pi4-hammer: board matrix=%s\n' "$matrix_out"
}

board_mode() {
    SERIAL_STATUS=UNVERIFIED
    SERIAL_EVIDENCE="no serial device supplied"
    GENET_STATUS=UNVERIFIED
    GENET_EVIDENCE="no captured log"
    SOAK_STATUS=UNVERIFIED
    SOAK_EVIDENCE="no captured log"
    ROOT_STATUS=UNVERIFIED
    ROOT_EVIDENCE="no captured log"
    AUDIO_STATUS=UNVERIFIED
    AUDIO_EVIDENCE="no hardware marker script; follow the Pi 4 audio note section 5"

    if [ -n "$DEVICE" ] && [ -z "$BOARD_LOG" ]; then
        BOARD_LOG=$WORK/serial.log
    fi

    if [ -n "$DEVICE" ]; then
        if [ ! -f "$IMAGE" ]; then
            find_firmware >/dev/null 2>&1 || true
            if ! sh -c "cargo xtask image --arch aarch64 --platform pi4 --profile pi4" \
                >"$WORK/board-image.log" 2>&1; then
                printf 'pi4-hammer: cannot build the pi4 image; see %s\n' \
                    "$WORK/board-image.log" >&2
                SERIAL_STATUS=FAIL
                SERIAL_EVIDENCE="image build failed"
                write_board_matrix || exit 2
                exit 1
            fi
        fi
        printf '[ BOARD ] serial capture on %s (stop with Ctrl-C)\n' "$DEVICE"
        if sh scripts/pi4-serial.sh -d "$DEVICE" -i "$IMAGE" -l "$BOARD_LOG"; then
            SERIAL_STATUS=PASS
        else
            SERIAL_STATUS=FAIL
        fi
        SERIAL_EVIDENCE="log: $BOARD_LOG"
    fi

    if [ -n "$BOARD_LOG" ]; then
        if [ ! -s "$BOARD_LOG" ]; then
            printf 'pi4-hammer: serial log is empty or missing: %s\n' "$BOARD_LOG" >&2
            SERIAL_STATUS=FAIL
            SERIAL_EVIDENCE="empty or missing log: $BOARD_LOG"
        else
            board_check genet "scripts/pi4-genet-evidence.sh $BOARD_LOG"
            GENET_STATUS=$board_status
            GENET_EVIDENCE=$board_evidence
            board_check soak "scripts/pi4-soak.sh $BOARD_LOG"
            SOAK_STATUS=$board_status
            SOAK_EVIDENCE=$board_evidence
            if grep -Fq 'svcd: root mounted at /root' "$BOARD_LOG" &&
                grep -Fq 'fsd-ufs: mounted block:' "$BOARD_LOG"; then
                ROOT_STATUS=PASS
                ROOT_EVIDENCE="root mount and fsd-ufs mount markers found"
            else
                ROOT_STATUS=FAIL
                ROOT_EVIDENCE="missing root mount or fsd-ufs mount marker"
            fi
        fi
    fi

    write_board_matrix || exit 2
    if [ "$SERIAL_STATUS" = FAIL ] || [ "$GENET_STATUS" = FAIL ] ||
        [ "$SOAK_STATUS" = FAIL ] || [ "$ROOT_STATUS" = FAIL ]; then
        printf 'pi4-hammer: board evidence FAIL\n'
        exit 1
    fi
    if [ "$SERIAL_STATUS" = UNVERIFIED ] && [ "$GENET_STATUS" = UNVERIFIED ] &&
        [ "$SOAK_STATUS" = UNVERIFIED ] && [ "$ROOT_STATUS" = UNVERIFIED ]; then
        printf 'pi4-hammer: board procedure written; every result is UNVERIFIED\n'
    else
        printf 'pi4-hammer: board checks: serial=%s genet=%s soak=%s root=%s\n' \
            "$SERIAL_STATUS" "$GENET_STATUS" "$SOAK_STATUS" "$ROOT_STATUS"
    fi
}

if [ "$MODE" = host ]; then
    host_mode
else
    board_mode
fi
