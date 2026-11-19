#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K145: aarch64 QEMU-virt cold-start login baseline.
#
# Boots the release aarch64 image under AAVMF/virt, waits for getty's `login:` prompt, sends
# `root`, waits for `Password:`, sends `raven`, and waits for the shell prompt. It records the
# two leg times per sample and a min/median/p90/max summary. A login-stage failure is reported
# as BLOCKED with the preserved serial log; it is never converted into a fake sample.
#
# This is the aarch64 counterpart of K143's x86 harness. It uses the same table shape and the
# QEMU virt firmware/CPU settings as the aarch64 kernel-gate cases. No Pi 4 board is implied.

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

samples=5
smp=1
boot_timeout=60
image=build/aarch64/login-cold-start-release.img
out=build/login-cold-start-aarch64
accel=tcg
check=0

usage() {
    cat <<'EOF'
usage: sh scripts/login-cold-start-aarch64.sh [--samples N] [--smp N] [--timeout SECONDS]
                                             [--image FILE] [--out DIR] [--check]

  --samples N      login samples (default 5)
  --smp N          QEMU CPU count (default 1)
  --timeout N      per-boot wall-clock limit (default 60 s)
  --image FILE     release aarch64 image (default build/aarch64/login-cold-start-release.img)
  --out DIR        logs/table directory (default build/login-cold-start-aarch64)
  --check          exit 1 when a measured leg exceeds 1000 ms
EOF
}

fail() {
    printf 'login-cold-start-aarch64: FAIL %s\n' "$*" >&2
    exit 2
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --samples) [ "$#" -ge 2 ] || fail "--samples needs a value"; samples=$2; shift 2 ;;
        --smp) [ "$#" -ge 2 ] || fail "--smp needs a value"; smp=$2; shift 2 ;;
        --timeout) [ "$#" -ge 2 ] || fail "--timeout needs a value"; boot_timeout=$2; shift 2 ;;
        --image) [ "$#" -ge 2 ] || fail "--image needs a value"; image=$2; shift 2 ;;
        --out) [ "$#" -ge 2 ] || fail "--out needs a value"; out=$2; shift 2 ;;
        --check) check=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; fail "unknown argument: $1" ;;
    esac
done

case "$samples" in ''|*[!0-9]*) fail "invalid --samples: $samples" ;; esac
case "$smp" in ''|*[!0-9]*) fail "invalid --smp: $smp" ;; esac
case "$boot_timeout" in ''|*[!0-9]*) fail "invalid --timeout: $boot_timeout" ;; esac
[ "$samples" -ge 1 ] || fail "--samples must be at least 1"

command -v qemu-system-aarch64 >/dev/null 2>&1 || fail "qemu-system-aarch64 not installed"
command -v python3 >/dev/null 2>&1 || fail "python3 is required for the pty login driver"
[ -r /usr/share/AAVMF/AAVMF_CODE.fd ] || fail "AAVMF code not found"
[ -r /usr/share/AAVMF/AAVMF_VARS.fd ] || fail "AAVMF vars not found"

mkdir -p "$out"
commit=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)

if [ ! -f "$image" ]; then
    printf '==> build aarch64 release image from %s\n' "$commit"
    if ! timeout 150 cargo xtask image --arch aarch64 --platform generic --profile release \
        --output "$image" >"$out/image-build.log" 2>&1; then
        tail -20 "$out/image-build.log" >&2
        fail "aarch64 release image build failed (see $out/image-build.log)"
    fi
fi
image_sha=$(sha256sum "$image" | awk '{print $1}')
load1=$(cut -d ' ' -f1 /proc/loadavg)
table=$out/samples.tsv
summary=$out/summary.tsv
: >"$table"
: >"$summary"

printf 'login-cold-start-aarch64: commit=%s image=%s sha256=%s accel=%s smp=%s samples=%s load1=%s\n' \
    "$commit" "$image" "$image_sha" "$accel" "$smp" "$samples" "$load1"

sample=1
while [ "$sample" -le "$samples" ]; do
    vars=$out/vars-$sample.fd
    cp /usr/share/AAVMF/AAVMF_VARS.fd "$vars"
    log=$out/sample-$sample.log
    if result=$(timeout "$boot_timeout" python3 - "$image" "$vars" "$log" "$smp" "$boot_timeout" <<'PY'
import os
import pty
import select
import subprocess
import sys
import time

image, vars_file, log_path, smp, timeout = sys.argv[1:6]
timeout = float(timeout)
args = [
    "qemu-system-aarch64",
    "-M", "virt,gic-version=2",
    "-cpu", "cortex-a72",
    "-m", "512M",
    "-smp", smp,
    "-nographic",
    "-semihosting-config", "enable=on,target=native",
    "-drive", "if=pflash,format=raw,readonly=on,file=/usr/share/AAVMF/AAVMF_CODE.fd",
    "-drive", f"if=pflash,format=raw,file={vars_file}",
    "-drive", f"format=raw,file={image}",
]
master, slave = pty.openpty()
log = open(log_path, "wb")
proc = subprocess.Popen(args, stdin=slave, stdout=slave, stderr=slave, close_fds=True)
os.close(slave)

def read_until(pattern, timeout, fail_on_exit=False):
    deadline = time.monotonic() + timeout
    data = bytearray()
    while time.monotonic() < deadline:
        if proc.poll() is not None:
            try:
                data.extend(os.read(master, 65536))
            except OSError:
                pass
            return None, bytes(data)
        ready, _, _ = select.select([master], [], [], 0.05)
        if not ready:
            continue
        try:
            chunk = os.read(master, 65536)
        except OSError:
            return None, bytes(data)
        if not chunk:
            return None, bytes(data)
        log.write(chunk)
        log.flush()
        data.extend(chunk)
        if pattern in data:
            return bytes(data), b""
        if fail_on_exit and (b"process: child exited" in data or b"init exited" in data):
            return None, bytes(data)
    return None, bytes(data)

def send(text):
    os.write(master, text)
    log.write(text)
    log.flush()

try:
    seen, _ = read_until(b"login: ", timeout)
    if seen is None:
        print("BLOCKED stage=login-prompt")
        sys.exit(3)
    t0 = time.monotonic()
    send(b"root\n")
    seen, tail = read_until(b"Password: ", timeout, True)
    if seen is None:
        text = (seen or b"") + tail
        if b"process: child exited" in text or b"init exited" in text:
            print("BLOCKED stage=login-exit-before-password")
        else:
            print("BLOCKED stage=password-prompt")
        sys.exit(3)
    t1 = time.monotonic()
    send(b"raven\n")
    seen, _ = read_until(b"raven# ", timeout, True)
    if seen is None:
        print("BLOCKED stage=shell-prompt")
        sys.exit(3)
    t2 = time.monotonic()
    print(f"sample login_ms={int((t1-t0)*1000)} password_ms={int((t2-t1)*1000)}")
    send(b"exit 0\n")
    time.sleep(0.2)
finally:
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
    try:
        os.close(master)
    except OSError:
        pass
    log.close()
PY
    ); then
        rc=0
    else
        rc=$?
    fi
    if [ "$rc" -eq 124 ]; then
        printf 'login-cold-start-aarch64: BLOCKED stage=timeout after %ss; preserved log: %s\n' \
            "$boot_timeout" "$log" >&2
        exit 3
    fi
    case "$result" in
        sample*)
            login_ms=$(printf '%s\n' "$result" | sed -n 's/.*login_ms=\([0-9]*\).*/\1/p')
            password_ms=$(printf '%s\n' "$result" | sed -n 's/.*password_ms=\([0-9]*\).*/\1/p')
            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$sample" "$load1" "$accel" "$smp" "$commit" "$login_ms" "$password_ms" >>"$table"
            printf 'sample %s: login=%sms password=%sms log=%s\n' "$sample" "$login_ms" "$password_ms" "$log"
            ;;
        BLOCKED*)
            printf 'login-cold-start-aarch64: %s\n' "$result" >&2
            printf 'login-cold-start-aarch64: BLOCKED at sample %s; preserved log: %s\n' "$sample" "$log" >&2
            tail -20 "$log" | tr -d '\r' >&2
            exit 3
            ;;
        *)
            printf 'login-cold-start-aarch64: unexpected driver result: %s (rc=%s, log=%s)\n' "$result" "$rc" "$log" >&2
            tail -20 "$log" | tr -d '\r' >&2
            exit 2
            ;;
    esac
    sample=$((sample + 1))
done

printf 'sample\tload1\taccel\tsmp\tcommit\tlogin_ms\tpassword_ms\n' >"$summary"
cat "$table" >>"$summary"
printf '%s\n' '--- summary ---'
for spec in login_ms:6 password_ms:7; do
    column=${spec%%:*}
    field=${spec##*:}
    values=$(awk -F '\t' -v c="$field" 'NF {print $c}' "$table" | sort -n)
    n=$(printf '%s\n' "$values" | awk 'NF{n++} END{print n+0}')
    min=$(printf '%s\n' "$values" | head -n 1)
    max=$(printf '%s\n' "$values" | tail -n 1)
    median=$(printf '%s\n' "$values" | awk '{v[NR]=$1} END{ if (NR==0) print 0; else if (NR%2) print v[(NR+1)/2]; else print int((v[NR/2]+v[NR/2+1])/2) }')
    p90=$(printf '%s\n' "$values" | awk '{v[NR]=$1} END{ if (NR==0) print 0; else {i=int((9*NR+9)/10); if(i<1)i=1; if(i>NR)i=NR; print v[i]} }')
    printf '%s n=%s min=%s median=%s p90=%s max=%s\n' "$column" "$n" "$min" "$median" "$p90" "$max"
done
printf 'table: %s\nsummary: %s\n' "$table" "$summary"

if [ "$check" -eq 1 ]; then
    if awk -F '\t' 'NR>0 && ($6>1000 || $7>1000) {exit 1}' "$table"; then
        exit 0
    fi
    printf 'login-cold-start-aarch64: --check FAIL: a measured leg exceeded 1000 ms\n' >&2
    exit 1
fi
