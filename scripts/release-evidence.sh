#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Capture and verify the 0.1.0 release gates without inventing evidence.
#
# Usage:
#   scripts/release-evidence.sh --check
#   scripts/release-evidence.sh --qemu
#   scripts/release-evidence.sh --pi4-log LOG [--pi4-net-log LOG]
#   scripts/release-evidence.sh --record [--pi4-log LOG] [--pi4-net-log LOG]
#
# --check validates the gate inputs and any recorded transcript/log; --qemu runs
# scripts/qemu-release.sh and checks its release-login transcript; --pi4-log checks an existing
# Pi 4 serial log with scripts/pi4-release.sh (and the GENET net evidence when available);
# --record writes build/release/<version>/evidence.txt with the commit, the QEMU and Pi 4 image
# hashes, the provenance hash and the transcript/log hashes. A missing gate artifact is a hard
# error: the script never emits a fake hash, transcript or tag. Board claims stay UNVERIFIED until
# the owner supplies a log for the REL3/P8/P10/P12/REL4d gate.

set -eu
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/lib.sh"
cd "$ROOT"

VERSION=${VERSION:-0.1.0}
ARCH=${ARCH:-x86_64}
OUT=${OUT:-$ROOT/build/release/$VERSION}
IMAGE=${IMAGE:-$ROOT/build/$ARCH/ravenbsd-$ARCH.img}
PI4_IMAGE=${PI4_IMAGE:-$ROOT/build/aarch64/ravenbsd-pi4.img}
MODE=check
PI4_LOG=
PI4_NET_LOG=
QEMU_LOG=
ARTIFACT_COMMIT=

usage() {
    cat <<'EOF'
usage: scripts/release-evidence.sh [--check|--qemu|--pi4-log LOG|--record]

  --check           validate gate inputs and recorded transcripts/logs
  --qemu            run scripts/qemu-release.sh and check its transcript
  --pi4-log LOG     check an existing Pi 4 release serial log
  --pi4-net-log LOG check the GENET net evidence log (defaults to the Pi 4 log)
  --record          write evidence.txt from the checked image/transcripts
  --record-qemu     write evidence-qemu.txt (QEMU artifact only; Pi 4 UNVERIFIED)
  --qemu-log LOG    transcript to validate/record (default build/release/0.1.0/qemu-release.log)
  --commit HASH     image build commit for the evidence record (default current HEAD)
  --pi4-image PATH  Pi 4 release image (default build/aarch64/ravenbsd-pi4.img)
  --out DIR         evidence/output directory (default build/release/0.1.0)
  --image PATH      release image (default build/x86_64/ravenbsd-x86_64.img, the
                    `make image ARCH=x86_64 PROFILE=release` output)

Environment: VERSION, ARCH, OUT, IMAGE.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --check) MODE=check ;;
        --qemu) MODE=qemu ;;
        --pi4-log)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            MODE=pi4
            PI4_LOG=$2
            shift
            ;;
        --pi4-net-log)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            PI4_NET_LOG=$2
            shift
            ;;
        --record)
            MODE=record
            ;;
        --record-qemu)
            MODE=record-qemu
            ;;
        --qemu-log)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            QEMU_LOG=$2
            shift
            ;;
        --commit)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            ARTIFACT_COMMIT=$2
            shift
            ;;
        --pi4-image)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            PI4_IMAGE=$2
            shift
            ;;
        --out)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            OUT=$2
            shift
            ;;
        --image)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            IMAGE=$2
            shift
            ;;
        --help|-h) usage; exit 0 ;;
        *) usage >&2; die "unknown option: $1" ;;
    esac
    shift
done

mkdir -p "$OUT"

sha256_file() {
    if have sha256sum; then
        sha256sum "$1" | awk '{print $1}'
    elif have shasum; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        die "sha256sum or shasum is required"
    fi
}

missing=""
add_missing() {
    missing="$missing $1"
}

release_profile() {
    for candidate in profiles/release.toml profiles/release-*.toml; do
        if [ -f "$candidate" ]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

gate_inputs() {
    missing=""
    profile=$(release_profile || true)
    [ -n "$profile" ] || add_missing "profiles/release.toml (REL2a/REL2b)"
    for file in \
        scripts/qemu-release.sh \
        tests/fixtures/release/check-transcript.sh \
        docs/release-qemu.md \
        scripts/pi4-release.sh \
        tests/fixtures/pi4-release/gate-markers.txt \
        docs/pi4-release.md \
        scripts/pi4-net-evidence.sh \
        docs/pi4-net.md \
        scripts/release-verify.sh \
        docs/release-verify.md \
        PROVENANCE.toml
    do
        [ -f "$file" ] || add_missing "$file"
    done
    if [ -n "$missing" ]; then
        printf 'release-evidence: blocked: missing%s\n' "$missing" >&2
        return 1
    fi
}

qemu_inputs() {
    missing=""
    profile=$(release_profile || true)
    [ -n "$profile" ] || add_missing "profiles/release.toml (REL2a/REL2b)"
    for file in \
        scripts/qemu-release.sh \
        tests/fixtures/release/check-transcript.sh \
        docs/release-qemu.md \
        PROVENANCE.toml
    do
        [ -f "$file" ] || add_missing "$file"
    done
    if [ -n "$missing" ]; then
        printf 'release-evidence: blocked: missing%s\n' "$missing" >&2
        return 1
    fi
}

pi4_inputs() {
    missing=""
    for file in \
        scripts/pi4-release.sh \
        tests/fixtures/pi4-release/gate-markers.txt \
        docs/pi4-release.md \
        scripts/pi4-net-evidence.sh \
        docs/pi4-net.md \
        PROVENANCE.toml
    do
        [ -f "$file" ] || add_missing "$file"
    done
    if [ -n "$missing" ]; then
        printf 'release-evidence: blocked: missing%s\n' "$missing" >&2
        return 1
    fi
}

qemu_transcript() {
    transcript=$1
    [ -s "$transcript" ] || {
        printf 'release-evidence: blocked: missing QEMU transcript %s\n' "$transcript" >&2
        return 1
    }
    sh tests/fixtures/release/check-transcript.sh "$transcript"
}

pi4_evidence() {
    log=$1
    [ -s "$log" ] || {
        printf 'release-evidence: blocked: missing Pi 4 serial log %s\n' "$log" >&2
        return 1
    }
    sh scripts/pi4-release.sh --log "$log"
    net=${PI4_NET_LOG:-$log}
    [ -s "$net" ] || {
        printf 'release-evidence: blocked: missing Pi 4 net evidence %s\n' "$net" >&2
        return 1
    }
    sh scripts/pi4-net-evidence.sh "$net"
}

release_image() {
    [ -s "$IMAGE" ] || {
        printf 'release-evidence: blocked: missing release image %s\n' "$IMAGE" >&2
        return 1
    }
}

pi4_image_check() {
    [ -s "$PI4_IMAGE" ] || {
        printf 'release-evidence: blocked: missing Pi 4 release image %s\n' "$PI4_IMAGE" >&2
        return 1
    }
    sh scripts/pi4-release.sh --check -i "$PI4_IMAGE"
}

record() {
    gate_inputs
    release_image
    pi4_image_check
    qemu=${QEMU_LOG:-$OUT/qemu-release.log}
    pi4=${PI4_LOG:-$OUT/pi4-serial.log}
    net=${PI4_NET_LOG:-$pi4}
    qemu_transcript "$qemu"
    pi4_evidence "$pi4"
    commit=$(git rev-parse HEAD)
    dirty="clean"
    [ -n "$(git status --porcelain)" ] && dirty="dirty"
    {
        printf 'version=%s\n' "$VERSION"
        printf 'commit=%s\n' "$commit"
        printf 'working_tree=%s\n' "$dirty"
        printf 'image=%s\n' "$IMAGE"
        printf 'image_sha256=%s\n' "$(sha256_file "$IMAGE")"
        printf 'provenance_sha256=%s\n' "$(sha256_file PROVENANCE.toml)"
        printf 'qemu_transcript=%s\n' "$qemu"
        printf 'qemu_transcript_sha256=%s\n' "$(sha256_file "$qemu")"
        printf 'qemu_gate=PASS\n'
        printf 'pi4_serial_log=%s\n' "$pi4"
        printf 'pi4_serial_sha256=%s\n' "$(sha256_file "$pi4")"
        printf 'pi4_gate=PASS\n'
        printf 'pi4_net_log=%s\n' "$net"
        printf 'pi4_net_sha256=%s\n' "$(sha256_file "$net")"
        printf 'pi4_net_gate=PASS\n'
        printf 'board=OBSERVED owner-supplied log\n'
    } >"$OUT/evidence.txt"
    log "evidence: $OUT/evidence.txt"
}

record_qemu() {
    qemu_inputs
    release_image
    pi4_image_check
    qemu=${QEMU_LOG:-$OUT/qemu-release.log}
    qemu_transcript "$qemu"
    commit=${ARTIFACT_COMMIT:-$(git rev-parse HEAD)}
    dirty="clean"
    [ -n "$(git status --porcelain)" ] && dirty="dirty"
    {
        printf 'version=%s\n' "$VERSION"
        printf 'commit=%s\n' "$commit"
        printf 'working_tree=%s\n' "$dirty"
        printf 'image=%s\n' "$IMAGE"
        printf 'image_sha256=%s\n' "$(sha256_file "$IMAGE")"
        printf 'provenance_sha256=%s\n' "$(sha256_file PROVENANCE.toml)"
        printf 'qemu_transcript=%s\n' "$qemu"
        printf 'qemu_transcript_sha256=%s\n' "$(sha256_file "$qemu")"
        printf 'qemu_gate=PASS (qtest release-login; check-transcript)\n'
        printf 'pi4_image=%s\n' "$PI4_IMAGE"
        printf 'pi4_image_sha256=%s\n' "$(sha256_file "$PI4_IMAGE")"
        printf 'pi4_host_check=PASS\n'
        printf 'pi4_gate=UNVERIFIED (REL3/P8/P10/P12/REL4d board log missing)\n'
        printf 'tag=none (Pi 4 board gate and full make check still required)\n'
    } >"$OUT/evidence-qemu.txt"
    log "QEMU evidence: $OUT/evidence-qemu.txt"
}

case "$MODE" in
    check)
        gate_inputs
        qemu=${QEMU_LOG:-$OUT/qemu-release.log}
        pi4=${PI4_LOG:-$OUT/pi4-serial.log}
        net=${PI4_NET_LOG:-$pi4}
        release_image
        pi4_image_check
        qemu_transcript "$qemu"
        pi4_evidence "$pi4"
        log "release evidence: all gates present"
        ;;
    qemu)
        qemu_inputs
        mkdir -p "$OUT"
        sh scripts/qemu-release.sh
        cp build/release/qemu-release.log "$OUT/qemu-release.log" 2>/dev/null || true
        qemu_transcript "$OUT/qemu-release.log"
        log "QEMU transcript: $OUT/qemu-release.log"
        ;;
    pi4)
        pi4_inputs
        pi4_evidence "$PI4_LOG"
        log "Pi 4 evidence: $PI4_LOG"
        ;;
    record)
        record
        ;;
    record-qemu)
        record_qemu
        ;;
esac
