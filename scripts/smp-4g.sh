#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# S157: 4 GiB / 4 vCPU release-image boot smoke. Runs five KVM and five TCG boots in the
# foreground, logs in, executes uname/ls/ps/du, exits cleanly, and records login/prompt timing,
# serial logs and host load. One failure stops the matrix and is preserved; there is no retry.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd -P)
cd "$root"

image=build/qtest/smp-4g.img
kvm_count=5
tcg_count=5
kvm_timeout=20
tcg_timeout=30
code=${RAVEN_OVMF_CODE:-/usr/share/OVMF/OVMF_CODE_4M.fd}
vars_template=${RAVEN_OVMF_VARS:-/usr/share/OVMF/OVMF_VARS_4M.fd}
out=build/smp-4g/$(date -u +%Y%m%dT%H%M%SZ)

[ -f "$image" ] || {
    echo "smp-4g: missing $image; build the release image first" >&2
    exit 2
}
[ -f "$code" ] || { echo "smp-4g: missing OVMF code $code" >&2; exit 2; }
[ -f "$vars_template" ] || { echo "smp-4g: missing OVMF vars $vars_template" >&2; exit 2; }
mkdir -p "$out"

helper=$(mktemp "${TMPDIR:-/tmp}/smp-4g-login.XXXXXX")
trap 'rm -f "$helper"' EXIT

cat >"$helper" <<'PY'
import os
import pty
import re
import select
import subprocess
import sys
import time

boot_timeout = float(sys.argv[1])
step_timeout = float(sys.argv[2])
raw_path = sys.argv[3]
stamped_path = sys.argv[4]
mode = sys.argv[5]
run = sys.argv[6]
command = sys.argv[7:]
if command and command[0] == "--":
    command = command[1:]

ansi = re.compile(r"\x1b\[[0-9;?]*[ -/]*[@-~]")

master, slave = pty.openpty()
proc = subprocess.Popen(
    command,
    stdin=slave,
    stdout=slave,
    stderr=slave,
    close_fds=True,
)
os.close(slave)
start = time.monotonic()
buffer = ""
cursor = 0
raw = open(raw_path, "wb")
stamped = open(stamped_path, "w", encoding="utf-8")


def pump(until, timeout, stage):
    global buffer, cursor
    deadline = time.monotonic() + timeout
    while True:
        now = time.monotonic() - start
        if time.monotonic() >= deadline:
            print(f"stage={stage} elapsed={now:.3f}", file=sys.stderr)
            return None
        remaining = deadline - time.monotonic()
        ready, _, _ = select.select([master], [], [], min(remaining, 0.2))
        if ready:
            try:
                chunk = os.read(master, 4096)
            except OSError:
                chunk = b""
            if not chunk:
                return None
            raw.write(chunk)
            raw.flush()
            text = ansi.sub("", chunk.decode(errors="replace")).replace("\r", "")
            buffer += text
            stamped.write(f"{now:.6f} {text}\n")
            stamped.flush()
            match = re.search(until, buffer[cursor:], re.MULTILINE | re.IGNORECASE)
            if match:
                cursor += match.end()
                return match
        if proc.poll() is not None:
            return None


def send(text):
    os.write(master, text.encode())


def finish(ok, detail):
    raw.flush()
    stamped.flush()
    raw.close()
    stamped.close()
    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=2)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
    if ok:
        print(detail)
        return 0
    print(f"smp-4g: fail mode={mode} run={run} {detail}", file=sys.stderr)
    return 1


if not pump(r"(?i).*login:\s*$", boot_timeout, "boot-login"):
    sys.exit(finish(False, "login prompt timeout"))
login_at = time.monotonic() - start
send("root\n")
if not pump(r"(?i)password:\s*$", step_timeout, "boot-password"):
    sys.exit(finish(False, "password prompt timeout"))
password_at = time.monotonic() - start
send("raven\n")
if not pump(r"(?i)raven#", step_timeout, "boot-prompt"):
    sys.exit(finish(False, "shell prompt timeout"))
prompt_at = time.monotonic() - start

steps = [
    ("uname -a\n", r"^RavenBSD raven 0\.1 .* x86_64$", "uname"),
    ("ls /bin\n", r"(?m)^uname$", "ls"),
    ("ps --posix\n", r"^PID PPID UID GID CMD$", "ps"),
    ("du /tmp\n", r"^[0-9.]+[KMG]?\s+/tmp$", "du"),
]
for command_text, pattern, label in steps:
    send(command_text)
    if not pump(pattern, step_timeout, f"{label}-output"):
        sys.exit(finish(False, f"step {label} timeout"))
    if not pump(r"(?i)raven#", step_timeout, f"{label}-prompt"):
        sys.exit(finish(False, f"prompt after {label} timeout"))

send("exit 0\n")
if not pump(r"^init exited \(0\)$", step_timeout, "exit"):
    sys.exit(finish(False, "clean init exit timeout"))

sys.exit(
    finish(
        True,
        f"login_s={login_at:.3f} password_to_prompt_s={prompt_at - password_at:.3f}",
    )
)
PY

:
: >"$out/host.tsv"
printf 'mode\trun\tload1_before\tload1_after\tlogin_s\tpassword_to_prompt_s\tstatus\n' >"$out/host.tsv"

run_one() {
    mode=$1
    run=$2
    boot_timeout=$3
    vars="$out/$mode-$run.vars.fd"
    raw="$out/$mode-$run.log"
    stamped="$out/$mode-$run.stamped.log"
    cp "$vars_template" "$vars"
    load_before=$(cut -d' ' -f1 /proc/loadavg)
    if [ "$mode" = kvm ]; then
        output=$(python3 "$helper" "$boot_timeout" 30 "$raw" "$stamped" "$mode" "$run" -- \
            qemu-system-x86_64 -machine q35 -serial stdio -display none \
            -monitor none -no-reboot \
            -drive if=pflash,format=raw,readonly=on,file="$code" \
            -drive if=pflash,format=raw,file="$vars" \
            -drive format=raw,file="$image" \
            -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
            -nic none -netdev user,id=n0 \
            -device virtio-net-pci,netdev=n0,disable-legacy=on \
            -m 4096 -smp 4 -enable-kvm -cpu host)
    else
        output=$(python3 "$helper" "$boot_timeout" 30 "$raw" "$stamped" "$mode" "$run" -- \
            qemu-system-x86_64 -machine q35 -serial stdio -display none \
            -monitor none -no-reboot \
            -drive if=pflash,format=raw,readonly=on,file="$code" \
            -drive if=pflash,format=raw,file="$vars" \
            -drive format=raw,file="$image" \
            -device isa-debug-exit,iobase=0xf4,iosize=0x04 \
            -nic none -netdev user,id=n0 \
            -device virtio-net-pci,netdev=n0,disable-legacy=on \
            -m 4096 -smp 4)
    fi
    status=$?
    load_after=$(cut -d' ' -f1 /proc/loadavg)
    cpu_line=$(grep -aE 'smp: 4 cpus online' "$raw" | tail -n 1 || true)
    mem_mib=$(sed -n 's/^mem: \([0-9][0-9]*\) MiB usable.*/\1/p' "$raw" | head -n 1)
    forbidden=$(grep -aEi 'No child process|i/o error|ECHILD|panic|EXCEPTION|out of memory|oom-kill|killed by page fault' "$raw" | head -n 1 || true)
    if [ -z "$cpu_line" ] || [ -z "$mem_mib" ] || [ "$mem_mib" -lt 3000 ]; then
        echo "smp-4g: wrong CPU/RAM mode=$mode run=$run cpu=${cpu_line:-missing} mem_mib=${mem_mib:-missing}" >&2
        status=1
    fi
    if [ -n "$forbidden" ]; then
        echo "smp-4g: forbidden line mode=$mode run=$run: $forbidden" >&2
        status=1
    fi
    login=$(printf '%s\n' "$output" | sed -n 's/.*login_s=\([0-9.]*\).*/\1/p')
    prompt=$(printf '%s\n' "$output" | sed -n 's/.*password_to_prompt_s=\([0-9.]*\).*/\1/p')
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$mode" "$run" "$load_before" "$load_after" "$login" "$prompt" "$status" >>"$out/host.tsv"
    if [ "$status" -ne 0 ]; then
        echo "smp-4g: FAIL mode=$mode run=$run log=$raw stamped=$stamped" >&2
        printf '%s/%s\n' "$mode" "$run" >"$out/FAILED"
        return 1
    fi
    echo "smp-4g: PASS mode=$mode run=$run login_s=$login prompt_s=$prompt load=$load_before/$load_after"
}

run=1
while [ "$run" -le "$kvm_count" ]; do
    run_one kvm "$run" "$kvm_timeout" || exit 1
    run=$((run + 1))
done
run=1
while [ "$run" -le "$tcg_count" ]; do
    run_one tcg "$run" "$tcg_timeout" || exit 1
    run=$((run + 1))
done

echo "smp-4g: PASS kvm=$kvm_count tcg=$tcg_count logs=$out"
echo "smp-4g: host-table $out/host.tsv"
