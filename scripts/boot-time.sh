#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# Boot-to-login timing for the x86_64 release image.
#
# The OS gate is the first `RavenBSD kernel` banner -> `login:` prompt, measured over three
# runs. QEMU process start -> login is reported separately because OVMF dominates it. The image
# is built from the current commit (or supplied with --image) and the guest banner must carry
# that commit; a reused image from another commit is refused.
#
# KVM 4-CPU is the 0.1.0 gate after S83. Until S83 lands this script runs the 1-CPU KVM baseline
# and prints the 4-CPU gate as pending; it never invents a 4-CPU result. --tcg runs the same
# measurement without KVM and reports it without the gate.
#
# Usage:
#   sh scripts/boot-time.sh                  # KVM 4 CPU, 3 runs (OVMF path)
#   sh scripts/boot-time.sh --no-firmware    # S89 direct boot; QEMU->login <1 s gate
#   sh scripts/boot-time.sh --smp 1          # 1-CPU baseline
#   sh scripts/boot-time.sh --tcg            # TCG report, no gate
#   sh scripts/boot-time.sh --image FILE     # use an existing image (commit checked)
#   sh scripts/boot-time.sh --runs 1         # shorter exploratory run
#   sh scripts/boot-time.sh --no-login-checks  # timing only
#
# The timing run stops the VM as soon as the login prompt appears. The login checks then boot
# once more, log in as root, and prove the brush shell, tab completion (`complete -p`, the
# deterministic proof from etc/profile), the QEMU user-network peer (`ping 10.0.2.2`) and the
# interactive command latency. They are skipped with --tcg (latency is only gated under KVM).

set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

arch=x86_64
runs=3
smp=4
kvm=1
no_firmware=0
login_checks=1
exclusive=0
timeout_s=300
image=
out=build/boot-time
ovmf_code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
ovmf_vars=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
qemu_pid=
tmpdir=

usage() {
    cat <<'EOF'
usage: sh scripts/boot-time.sh [--arch x86_64] [--smp N] [--runs N] [--tcg]
                               [--no-firmware] [--no-login-checks] [--exclusive]
                               [--image FILE] [--out DIR] [--timeout SECONDS]

  --arch x86_64  only the x86_64 release gate is supported.
  --smp N        QEMU CPU count (default 4, the 0.1.0 gate).
  --runs N       measured runs (default 3).
  --tcg          disable KVM; report only, no gate.
  --no-firmware  drive the S89 direct-boot path; gate QEMU start -> login <1 s.
  --no-login-checks  skip the post-login shell/completion/network/latency checks.
  --exclusive    refuse a busy host and record the host state before measuring.
  --image FILE   use an existing image instead of building one; its banner commit is checked.
  --out DIR      serial logs and summary (default build/boot-time).
  --timeout N    per-run wall-clock limit (default 300 s).
EOF
}

fail() {
    printf 'boot-time: FAIL %s\n' "$*" >&2
    exit 1
}

cleanup() {
    if [ -n "${qemu_pid:-}" ]; then
        kill "$qemu_pid" 2>/dev/null || true
        wait "$qemu_pid" 2>/dev/null || true
    fi
    if [ -n "${tmpdir:-}" ]; then
        rm -rf "$tmpdir"
    fi
    return 0
}

trap cleanup EXIT HUP INT TERM

while [ "$#" -gt 0 ]; do
    case $1 in
        --arch)
            [ "$#" -ge 2 ] || { printf 'boot-time: --arch needs a value\n' >&2; exit 2; }
            arch=$2
            shift 2
            ;;
        --smp)
            [ "$#" -ge 2 ] || { printf 'boot-time: --smp needs a value\n' >&2; exit 2; }
            smp=$2
            shift 2
            ;;
        --runs)
            [ "$#" -ge 2 ] || { printf 'boot-time: --runs needs a value\n' >&2; exit 2; }
            runs=$2
            shift 2
            ;;
        --timeout)
            [ "$#" -ge 2 ] || { printf 'boot-time: --timeout needs a value\n' >&2; exit 2; }
            timeout_s=$2
            shift 2
            ;;
        --image)
            [ "$#" -ge 2 ] || { printf 'boot-time: --image needs a value\n' >&2; exit 2; }
            image=$2
            shift 2
            ;;
        --out)
            [ "$#" -ge 2 ] || { printf 'boot-time: --out needs a value\n' >&2; exit 2; }
            out=$2
            shift 2
            ;;
        --tcg)
            kvm=0
            shift
            ;;
        --no-firmware)
            no_firmware=1
            shift
            ;;
        --no-login-checks)
            login_checks=0
            shift
            ;;
        --exclusive)
            exclusive=1
            shift
            ;;
        --help | -h)
            usage
            exit 0
            ;;
        *)
            printf 'boot-time: unknown argument: %s\n' "$1" >&2
            exit 2
            ;;
    esac
done

[ "$arch" = "x86_64" ] || {
    printf 'boot-time: only the x86_64 release gate is supported (got %s)\n' "$arch" >&2
    exit 2
}
case $smp in
    '' | *[!0-9]*) printf 'boot-time: invalid --smp value: %s\n' "$smp" >&2; exit 2 ;;
esac
case $runs in
    '' | *[!0-9]*) printf 'boot-time: invalid --runs value: %s\n' "$runs" >&2; exit 2 ;;
esac
[ "$runs" -ge 1 ] || { printf 'boot-time: --runs must be at least 1\n' >&2; exit 2; }
case $timeout_s in
    '' | *[!0-9]*) printf 'boot-time: invalid --timeout value: %s\n' "$timeout_s" >&2; exit 2 ;;
esac

commit=$(git rev-parse --short HEAD)
[ -n "$commit" ] || fail "cannot read HEAD"

# The S89 direct path has no OVMF stage: QEMU starts the PVH loader with -kernel. The 0.1.0
# gate is the full QEMU-process start -> login interval on that path.
if [ "$no_firmware" -eq 1 ]; then
    [ -z "$image" ] || fail "--no-firmware uses the direct loader; --image is not supported"
    mkdir -p "$out"
    printf '==> build kernel from %s\n' "$commit"
    if ! cargo xtask build kernel --arch "$arch" --release >"$out/direct-build-kernel.log" 2>&1; then
        tail -20 "$out/direct-build-kernel.log" >&2
        fail "kernel build failed"
    fi
    printf '==> build release initfs from %s\n' "$commit"
    if ! cargo xtask initfs --arch "$arch" --profile release >"$out/direct-build-initfs.log" 2>&1; then
        tail -20 "$out/direct-build-initfs.log" >&2
        fail "release initfs build failed"
    fi
    set -- --mode full --smp "$smp" --runs "$runs" --out "$out" --timeout "$timeout_s"
    if [ "$kvm" -eq 0 ]; then
        set -- "$@" --tcg
    fi
    if [ "$exclusive" -eq 1 ]; then
        set -- "$@" --exclusive
    fi
    exec sh scripts/boot-direct.sh "$@"
fi

[ -r "$ovmf_code" ] || fail "OVMF code not found: $ovmf_code (set RAVEN_OVMF_CODE)"
[ -r "$ovmf_vars" ] || fail "OVMF vars not found: $ovmf_vars (set RAVEN_OVMF_VARS)"
command -v qemu-system-x86_64 >/dev/null 2>&1 || fail "qemu-system-x86_64 is not installed"
command -v python3 >/dev/null 2>&1 || fail "python3 is required for the byte-level serial reader"
if [ "$kvm" -eq 1 ]; then
    [ -r /dev/kvm ] || fail "/dev/kvm is not readable; use --tcg for a software run"
fi

mkdir -p "$out"
if [ -z "$image" ]; then
    image=$out/release.img
    printf '==> build release image from %s\n' "$commit"
    if ! cargo xtask image --arch x86_64 --platform generic --profile release \
        --output "$image" >"$out/build.log" 2>&1; then
        tail -20 "$out/build.log" >&2
        fail "release image build failed"
    fi
else
    [ -f "$image" ] || fail "image not found: $image"
fi
[ -f "$image" ] || fail "image not found: $image"
image_sha=$(sha256sum "$image" | awk '{print $1}')

if [ "$exclusive" -eq 1 ]; then
    printf 'boot-time: exclusive preflight\n'
    if ! sh scripts/boot-direct.sh --exclusive --preflight-only --mode probe --out "$out" \
        >"$out/preflight.log" 2>&1; then
        cat "$out/preflight.log" >&2
        fail "exclusive preflight refused the host"
    fi
    cat "$out/preflight.log"
    preflight_json=$(sed -n 's/^boot-direct: preflight=//p' "$out/preflight.log")
    printf 'boot-time: exclusive preflight=%s\n' "$preflight_json"
fi

tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/boot-time.XXXXXX")
kernel_file=$tmpdir/kernel-ms
total_file=$tmpdir/total-ms
firmware_file=$tmpdir/firmware-ms
: >"$kernel_file"
: >"$total_file"
: >"$firmware_file"

median() {
    sort -n "$1" | awk '{ value[NR] = $1 } END {
        if (NR == 0) exit 1
        if (NR % 2) print value[(NR + 1) / 2]
        else print int((value[NR / 2] + value[NR / 2 + 1]) / 2)
    }'
}

p90() {
    sort -n "$1" | awk '{ value[NR] = $1 } END {
        if (NR == 0) exit 1
        idx = int((9 * NR + 9) / 10)
        if (idx < 1) idx = 1
        if (idx > NR) idx = NR
        print value[idx]
    }'
}

maximum() {
    sort -n "$1" | tail -n 1
}

run=1
while [ "$run" -le "$runs" ]; do
    vars=$tmpdir/vars-$run.fd
    cp "$ovmf_vars" "$vars"
    log=$out/run-$run.log

    # Read QEMU's serial stream as bytes, not lines: the `login:` prompt has no trailing newline,
    # so a line reader (or a `-serial file:` poll) can miss it until QEMU exits. The reader stops
    # the VM as soon as the prompt appears and prints the elapsed timestamps.
    if ! result=$(python3 - "$image" "$vars" "$log" "$timeout_s" "$smp" "$kvm" "$ovmf_code" "$tmpdir/serial-$run.sock" <<'PY'
import os
import re
import socket
import subprocess
import sys
import time

image, vars_file, serial_log, timeout_s, smp, kvm, ovmf_code, sock_path = sys.argv[1:9]
try:
    os.unlink(sock_path)
except FileNotFoundError:
    pass
# A Unix-socket serial backend is unbuffered. `-serial stdio` and `-serial file:` can hold the
# unterminated `login: ` prompt until a later newline, which turns a sub-second boot into a
# minute-long "measurement".
args = [
    "qemu-system-x86_64",
    "-machine", "q35",
    "-m", "512M",
    "-smp", smp,
    "-serial", f"unix:{sock_path},server=on,wait=off",
    "-display", "none",
    "-monitor", "none",
    "-no-reboot",
]
if kvm == "1":
    args.extend(["-enable-kvm", "-cpu", "host"])
args.extend([
    "-drive", f"if=pflash,format=raw,readonly=on,file={ovmf_code}",
    "-drive", f"if=pflash,format=raw,file={vars_file}",
    "-drive", f"format=raw,file={image}",
    "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
    "-nic", "none", "-netdev", "user,id=n0",
    "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
])

with open(serial_log, "wb") as log:
    start = time.monotonic_ns()
    try:
        proc = subprocess.Popen(
            args,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except OSError as error:
        print(f"boot-time reader: cannot start QEMU: {error}", file=sys.stderr)
        sys.exit(1)
    sock = None
    for _ in range(2000):
        try:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.connect(sock_path)
            break
        except OSError:
            sock = None
            if proc.poll() is not None:
                break
            time.sleep(0.005)
    if sock is None:
        proc.terminate()
        proc.wait()
        print("boot-time reader: cannot connect to the QEMU serial socket", file=sys.stderr)
        sys.exit(1)

    sock.settimeout(0.01)
    deadline = start + int(timeout_s) * 1_000_000_000
    data = b""
    kernel_ns = None
    kernel_id = None
    login_ns = None
    reason = "no login prompt"
    while True:
        now = time.monotonic_ns()
        if now >= deadline:
            reason = "timeout"
            break
        try:
            chunk = sock.recv(8192)
        except socket.timeout:
            continue
        except OSError:
            reason = "QEMU serial closed"
            break
        if not chunk:
            reason = "QEMU exited"
            break
        log.write(chunk)
        log.flush()
        data += chunk
        if kernel_ns is None:
            match = re.search(
                rb"RavenBSD kernel 0\.1 \(x86_64\) ([0-9a-f]+) ([0-9-]+)",
                data,
            )
            if match:
                kernel_ns = time.monotonic_ns()
                kernel_id = match.group(1).decode("ascii")
        if b"login:" in data:
            login_ns = time.monotonic_ns()
            break

    sock.close()
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()

    if kernel_ns is None:
        print("boot-time reader: no RavenBSD kernel banner", file=sys.stderr)
        sys.exit(1)
    if login_ns is None:
        print(f"boot-time reader: {reason} before login", file=sys.stderr)
        sys.exit(1)
    print(f"firmware_ms={(kernel_ns - start) // 1_000_000}")
    print(f"kernel_ms={(login_ns - kernel_ns) // 1_000_000}")
    print(f"total_ms={(login_ns - start) // 1_000_000}")
    print(f"kernel_id={kernel_id}")
PY
    ); then
        [ -f "$log" ] && tail -20 "$log" >&2
        fail "run $run: boot timing failed (log $log)"
    fi

    firmware_ms=$(printf '%s\n' "$result" | sed -n 's/^firmware_ms=//p')
    kernel_ms=$(printf '%s\n' "$result" | sed -n 's/^kernel_ms=//p')
    total_ms=$(printf '%s\n' "$result" | sed -n 's/^total_ms=//p')
    kernel_id=$(printf '%s\n' "$result" | sed -n 's/^kernel_id=//p')
    case $firmware_ms in '' | *[!0-9]*) fail "run $run: invalid firmware_ms";; esac
    case $kernel_ms in '' | *[!0-9]*) fail "run $run: invalid kernel_ms";; esac
    case $total_ms in '' | *[!0-9]*) fail "run $run: invalid total_ms";; esac
    [ "$kernel_id" = "$commit" ] ||
        fail "run $run: guest kernel id $kernel_id does not match HEAD $commit"

    printf 'boot-time: run=%s firmware_ms=%s kernel_ms=%s total_ms=%s kernel_id=%s\n' \
        "$run" "$firmware_ms" "$kernel_ms" "$total_ms" "$kernel_id"
    printf '%s\n' "$firmware_ms" >>"$firmware_file"
    printf '%s\n' "$kernel_ms" >>"$kernel_file"
    printf '%s\n' "$total_ms" >>"$total_file"
    run=$((run + 1))
done

median_firmware=$(median "$firmware_file")
median_kernel=$(median "$kernel_file")
median_total=$(median "$total_file")
p90_firmware=$(p90 "$firmware_file")
p90_kernel=$(p90 "$kernel_file")
p90_total=$(p90 "$total_file")
max_firmware=$(maximum "$firmware_file")
max_kernel=$(maximum "$kernel_file")
max_total=$(maximum "$total_file")

if [ "$kvm" -eq 1 ]; then accel=kvm; else accel=tcg; fi
printf 'boot-time: commit=%s arch=%s accel=%s smp=%s runs=%s image=%s sha256=%s\n' \
    "$commit" "$arch" "$accel" "$smp" "$runs" "$image" "$image_sha"
printf 'boot-time: p50 firmware_ms=%s kernel_ms=%s total_ms=%s\n' \
    "$median_firmware" "$median_kernel" "$median_total"
printf 'boot-time: p90 firmware_ms=%s kernel_ms=%s total_ms=%s\n' \
    "$p90_firmware" "$p90_kernel" "$p90_total"
printf 'boot-time: max firmware_ms=%s kernel_ms=%s total_ms=%s\n' \
    "$max_firmware" "$max_kernel" "$max_total"

# Post-login verification: brush shell, tab completion (`complete -p`, the deterministic proof
# from etc/profile), the QEMU user-network peer, and interactive command latency. Output markers
# are chosen so the serial echo of the typed command cannot match them.
login_check_failed=0
if [ "$login_checks" -eq 1 ] && [ "$kvm" -eq 1 ]; then
    check_vars=$tmpdir/check-vars.fd
    cp "$ovmf_vars" "$check_vars"
    check_log=$out/login-check.log
    if ! check_result=$(python3 - "$image" "$check_vars" "$check_log" "$timeout_s" "$smp" "$ovmf_code" "$tmpdir/login-check.sock" <<'PY2'
import os
import socket
import subprocess
import sys
import time

image, vars_file, log_path, timeout_s, smp, ovmf_code, sock_path = sys.argv[1:8]
try:
    os.unlink(sock_path)
except FileNotFoundError:
    pass
args = [
    "qemu-system-x86_64", "-machine", "q35", "-m", "512M", "-smp", smp,
    "-serial", f"unix:{sock_path},server=on,wait=off", "-display", "none",
    "-monitor", "none", "-no-reboot", "-enable-kvm", "-cpu", "host",
    "-drive", f"if=pflash,format=raw,readonly=on,file={ovmf_code}",
    "-drive", f"if=pflash,format=raw,file={vars_file}",
    "-drive", f"format=raw,file={image}",
    "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
    "-nic", "none", "-netdev", "user,id=n0",
    "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
]
log = open(log_path, "wb")
proc = subprocess.Popen(
    args, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
)
sock = None
for _ in range(2000):
    try:
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.connect(sock_path)
        break
    except OSError:
        sock = None
        if proc.poll() is not None:
            break
        time.sleep(0.005)
if sock is None:
    print("login-check: cannot connect to the serial socket", file=sys.stderr)
    sys.exit(1)
sock.settimeout(0.02)
buf = b""


def feed():
    global buf
    try:
        chunk = sock.recv(8192)
    except socket.timeout:
        return True
    except OSError:
        return False
    if not chunk:
        return False
    log.write(chunk)
    log.flush()
    buf += chunk
    return True


def wait_for(needle, seconds, reset=False):
    global buf
    if reset:
        buf = b""
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if needle in buf:
            return True
        if not feed():
            return False
    return needle in buf


def send(text):
    sock.sendall(text.encode())


def command(cmd, needle, seconds=20):
    global buf
    buf = b""
    start = time.monotonic()
    send(cmd + "\n")
    if not wait_for(needle, seconds):
        return None
    return round((time.monotonic() - start) * 1000.0, 1)


if not wait_for(b"login:", 120, reset=True):
    print("login-check: no login prompt", file=sys.stderr)
    sys.exit(1)
send("root\n")
if not wait_for(b"Password:", 30):
    print("login-check: no password prompt", file=sys.stderr)
    sys.exit(1)
send("raven\n")
if not wait_for(b"raven# ", 60):
    print("login-check: no root prompt", file=sys.stderr)
    sys.exit(1)

shell_ms = command("echo $SHELL", b"/bin/brush", 20)
complete_ms = command("complete -p", b"raven-test", 20)
ping_ms = command("ping -c 1 10.0.2.2", b"rtt=", 30)
echo_ms = command("echo $((6*7))", b"42", 20)
ls_ms = command("ls /", b"init", 20)

send("exit 0\n")
try:
    proc.wait(timeout=10)
except subprocess.TimeoutExpired:
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()
sock.close()
log.close()


def state(value):
    return "ok" if value is not None else "FAIL"


print(
    "shell=%s completion=%s ping=%s latency_ms=echo:%s,ls:%s"
    % (state(shell_ms), state(complete_ms), state(ping_ms), echo_ms, ls_ms)
)
sys.exit(
    0 if all(value is not None for value in (shell_ms, complete_ms, ping_ms, echo_ms, ls_ms)) else 1
)
PY2
    ); then
        [ -f "$check_log" ] && tail -20 "$check_log" >&2
        login_check_failed=1
        printf 'boot-time: login-check FAIL (log %s)\n' "$check_log" >&2
    else
        printf 'boot-time: login-check %s\n' "$check_result"
    fi
else
    printf 'boot-time: login-check SKIPPED (accel=%s)\n' "$accel"
fi

if [ "$kvm" -eq 1 ] && [ "$smp" -eq 4 ]; then
    if [ "$median_kernel" -lt 1000 ]; then
        printf 'boot-time: OS gate: KVM 4-CPU kernel->login median %sms < 1000ms\n' "$median_kernel"
    else
        printf 'boot-time: OS gate FAIL: KVM 4-CPU kernel->login median %sms >= 1000ms\n' "$median_kernel" >&2
        exit 1
    fi
    printf 'boot-time: OVMF total median %sms (firmware %sms); the 1 s QEMU->login gate is the S89 direct path: sh scripts/boot-time.sh --no-firmware\n' \
        "$median_total" "$median_firmware"
elif [ "$kvm" -eq 1 ]; then
    printf 'boot-time: baseline (smp=%s): kernel->login median %sms; the gate runs at -smp 4\n' \
        "$smp" "$median_kernel"
else
    printf 'boot-time: GATE NOT APPLICABLE (TCG): kernel->login median %sms reported only\n' "$median_kernel"
fi

if [ "$login_check_failed" -ne 0 ]; then
    exit 1
fi
