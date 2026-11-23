#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K495 rev172: per-session resource sampling for `ssh -tt` sessions.

Boots a release image built with the test-only `k449-metrics=2` sampler, runs one `-T` control and
three `-tt` sessions (a builtin `echo`, so the brush post-child path is not involved), and samples
the K449-METRICS procs/threads/ofds line plus ptyd's `files=0 pairs=N` line after each session.
The deltas identify what each `-tt` session creates and does not release.
"""

from __future__ import annotations

import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path("/media/bo/Rustworx/work/ravenbsd-minions/phil/ravenbsd")
IMAGE = ROOT / "build/qtest/ssh-resource.img"
OUT = Path("/tmp/opencode/tty-resource-probe")
OVMF_CODE = Path("/usr/share/OVMF/OVMF_CODE_4M.fd")
OVMF_VARS = Path("/usr/share/OVMF/OVMF_VARS_4M.fd")
ASKPASS = ROOT / "tests/ssh/askpass.sh"
PORT = int(os.environ.get("K495_SSH_PORT", "22222"))
SERIAL = OUT / "serial.log"
OUT.mkdir(parents=True, exist_ok=True)
shutil.copyfile(OVMF_VARS, OUT / "vars.fd")
SERIAL.write_bytes(b"")

METRICS_RE = re.compile(
    r"K449-METRICS heap_used=(\d+) heap_free=(\d+) frames_free=(\d+) frames_alloc=(\d+) "
    r"procs=(\d+) threads=(\d+) ofds=(\d+)"
)
PTYD_RE = re.compile(r"ptyd: files=(\d+) pairs=(\d+)")

argv = [
    "qemu-system-x86_64", "-machine", "q35", "-m", "512M", "-smp", "4",
    "-enable-kvm", "-cpu", "host",
    "-serial", f"file:{SERIAL}", "-display", "none", "-monitor", "none", "-no-reboot",
    "-snapshot",
    "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
    "-drive", f"if=pflash,format=raw,file={OUT / 'vars.fd'}",
    "-drive", f"format=raw,file={IMAGE}",
    "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
    "-nic", "none",
    "-netdev", f"user,id=n0,hostfwd=tcp:127.0.0.1:{PORT}-:22",
    "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
]

qemu = subprocess.Popen(argv, cwd=str(ROOT), stdin=subprocess.DEVNULL,
                        stdout=subprocess.DEVNULL,
                        stderr=open(OUT / "qemu.stderr", "wb"), start_new_session=True)


def kill() -> None:
    try:
        os.killpg(qemu.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    qemu.wait()


def serial_text() -> str:
    try:
        return SERIAL.read_text(errors="replace")
    except FileNotFoundError:
        return ""


def latest_metrics() -> dict[str, int] | None:
    found = METRICS_RE.findall(serial_text())
    if not found:
        return None
    heap_used, heap_free, frames_free, frames_alloc, procs, threads, ofds = found[-1]
    return {
        "heap_used": int(heap_used),
        "heap_free": int(heap_free),
        "frames_free": int(frames_free),
        "frames_alloc": int(frames_alloc),
        "procs": int(procs),
        "threads": int(threads),
        "ofds": int(ofds),
    }


def latest_ptyd() -> dict[str, int] | None:
    found = PTYD_RE.findall(serial_text())
    if not found:
        return None
    files, pairs = found[-1]
    return {"files": int(files), "pairs": int(pairs)}


def wait_for_fresh(pattern: re.Pattern[str], previous: int, timeout_s: float) -> None:
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        if len(pattern.findall(serial_text())) > previous:
            return
        time.sleep(0.2)


def sample(label: str) -> dict[str, object]:
    metrics = latest_metrics() or {}
    ptyd = latest_ptyd() or {}
    row = {"label": label, **metrics, **ptyd}
    print(
        f"{label}: procs={row.get('procs')} threads={row.get('threads')} "
        f"ofds={row.get('ofds')} ptyd_files={row.get('files')} ptyd_pairs={row.get('pairs')}",
        flush=True,
    )
    return row


def session(label: str, tty: bool, command: str = "echo K495") -> int:
    env = dict(os.environ)
    env["SSH_ASKPASS"] = str(ASKPASS)
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["DISPLAY"] = ":0"
    env["K434_SSH_PASSWORD"] = "raven"
    ssh_argv = [
        "ssh", "-p", str(PORT), "-tt" if tty else "-T",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "KbdInteractiveAuthentication=no",
        "-o", "NumberOfPasswordPrompts=1",
        "-o", "ConnectTimeout=20",
        "raven@127.0.0.1", command,
    ]
    try:
        result = subprocess.run(ssh_argv, cwd=str(ROOT), stdin=subprocess.DEVNULL,
                                capture_output=True, timeout=12, env=env,
                                start_new_session=True)
        print(f"{label} (tty={tty}): exit={result.returncode} "
              f"stdout={result.stdout.decode('utf-8', 'replace').strip()!r}", flush=True)
        return result.returncode
    except subprocess.TimeoutExpired as error:
        partial = (error.stdout or b"").decode("utf-8", "replace")
        print(f"{label} (tty={tty}): TIMEOUT after 12 s (the shell faulted and the "
              f"client never saw close); stdout={partial.strip()!r}", flush=True)
        return partial


try:
    deadline = time.monotonic() + 150
    while time.monotonic() < deadline:
        if qemu.poll() is not None:
            break
        if b"svcd: sshd up" in SERIAL.read_bytes():
            break
        time.sleep(0.2)
    wait_for_fresh(METRICS_RE, 0, 15)
    baseline = sample("boot")
    wait_for_fresh(PTYD_RE, 0, 15)
    baseline = sample("boot-ptyd")

    metrics_before = len(METRICS_RE.findall(serial_text()))
    ptyd_before = len(PTYD_RE.findall(serial_text()))
    session("control", tty=False, command="tty")
    wait_for_fresh(METRICS_RE, metrics_before, 8)
    wait_for_fresh(PTYD_RE, ptyd_before, 15)
    control = sample("control-T")

    rows = [baseline, control]
    for number in range(1, 4):
        metrics_before = len(METRICS_RE.findall(serial_text()))
        ptyd_before = len(PTYD_RE.findall(serial_text()))
        session(f"tty{number}", tty=True, command="tty")
        wait_for_fresh(METRICS_RE, metrics_before, 8)
        wait_for_fresh(PTYD_RE, ptyd_before, 15)
        rows.append(sample(f"tty{number}"))

    metrics_before = len(METRICS_RE.findall(serial_text()))
    ptyd_before = len(PTYD_RE.findall(serial_text()))
    session("ps", tty=False, command="ps -e")
    wait_for_fresh(METRICS_RE, metrics_before, 8)
    wait_for_fresh(PTYD_RE, ptyd_before, 15)
    time.sleep(0.5)
    print("=== serial tail after ps -e ===", flush=True)
    for line in serial_text().splitlines()[-80:]:
        print(line, flush=True)
    rows.append(sample("ps"))

    print("=== deltas ===", flush=True)
    for before, after in zip(rows, rows[1:]):
        def delta(key: str) -> int | None:
            left = before.get(key)
            right = after.get(key)
            if left is None or right is None:
                return None
            return int(right) - int(left)

        print(
            f"{before['label']} -> {after['label']}: "
            f"procs +{delta('procs')} threads +{delta('threads')} "
            f"ofds +{delta('ofds')} pairs +{delta('pairs')}",
            flush=True,
        )
finally:
    kill()
