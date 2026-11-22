#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K498 host-client sshd session-close trace over the isolated tap.

Boots the K498 DEBUG3 image with a private tap, makes one password login and
one command from the host (the K194 tap path; slirp hostfwd is the known
FIN/RST failure), and keeps the serial console, where the overlay sshd writes
its full debug log. The first session's close state is recorded: a command that
prints and then never closes is the S7/S5 symptom.
"""

from __future__ import annotations

import os
import socket
import subprocess
import sys
import time
from pathlib import Path

import pexpect

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "build" / "fixtures" / "sshd-trace"
IMAGE = OUT / "sshd-trace.img"
SERIAL = OUT / "host-serial.log"
CLIENT1 = OUT / "host-client1.log"
SERVER_LOG = OUT / "host-sshd.log"
SUMMARY = OUT / "host-summary.txt"
TAP = os.environ.get("RAVEN_TRACE_TAP", "ravetrace0")
GUEST = os.environ.get("RAVEN_TRACE_GUEST", "10.0.2.15")
OVMF_CODE = Path(os.environ.get("RAVEN_OVMF_CODE", "/usr/share/OVMF/OVMF_CODE_4M.fd"))
OVMF_VARS = Path(os.environ.get("RAVEN_OVMF_VARS", "/usr/share/OVMF/OVMF_VARS_4M.fd"))


def log(kind: str, text: str) -> None:
    print(f"sshd-trace-host: {kind}: {text}", flush=True)


def wait_tcp(host: str, port: int, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=3):
                return True
        except OSError:
            time.sleep(1)
    return False


def ssh(host: str, command: str, logfile: Path, close_timeout: int, expect: str) -> str:
    child = pexpect.spawn(
        "ssh",
        [
            "-p", "22",
            "-o", "StrictHostKeyChecking=no",
            "-o", "UserKnownHostsFile=/dev/null",
            "-o", "PreferredAuthentications=password",
            "-o", "PubkeyAuthentication=no",
            "-o", "ConnectTimeout=10",
            "-o", "NumberOfPasswordPrompts=1",
            f"raven@{host}",
            command,
        ],
        encoding="utf-8",
        codec_errors="replace",
        timeout=30,
    )
    state = "no-password"
    with open(logfile, "w", encoding="utf-8") as out:
        child.logfile = out
        try:
            child.expect(r"(?i)password:\s*")
            child.sendline("raven")
            child.expect(expect, timeout=30)
            state = "command-ok"
            child.expect(pexpect.EOF, timeout=close_timeout)
            state += "/closed"
        except pexpect.TIMEOUT:
            if state == "command-ok":
                state += "/close-timeout"
        except pexpect.EOF:
            state += "/eof"
        finally:
            child.close(force=True)
    return state


def main() -> int:
    if not IMAGE.is_file():
        log("error", f"missing {IMAGE}; run tests/fixtures/sshd-trace/build.sh")
        return 2
    for path in (SERIAL, CLIENT1, SERVER_LOG, SUMMARY):
        path.unlink(missing_ok=True)
    vars_path = OUT / "vars-host.fd"
    vars_path.write_bytes(OVMF_VARS.read_bytes())
    argv = [
        "-machine", "q35", "-m", "512M", "-smp", "4",
        "-serial", "stdio", "-display", "none", "-monitor", "none", "-no-reboot",
        "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"format=raw,file={IMAGE}",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-nic", "none",
        "-netdev", f"tap,id=n0,ifname={TAP},script=no,downscript=no",
        "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
    ]
    child = pexpect.spawn("qemu-system-x86_64", argv, cwd=str(ROOT),
                          encoding="utf-8", codec_errors="replace", timeout=240)
    first = "not-run"
    with open(SERIAL, "w", encoding="utf-8") as serial:
        child.logfile = serial
        try:
            if not wait_tcp(GUEST, 22, 120):
                log("error", f"guest sshd did not accept on {GUEST}:22")
                return 2
            log("ready", f"{GUEST}:22")
            first = ssh(GUEST, "echo sshd-trace-command-ok", CLIENT1, 20,
                        "sshd-trace-command-ok")
            log("client1", first)
            time.sleep(10)
        finally:
            child.close(force=True)
    text = SERIAL.read_text(encoding="utf-8", errors="replace")
    keep = []
    for line in text.splitlines():
        if (
            "sshd" in line
            or "debug1:" in line
            or "debug2:" in line
            or "debug3:" in line
            or "Server listening" in line
            or "Connection from" in line
            or "Accepted" in line
            or "Received" in line
            or "session" in line
            or "channel" in line
        ):
            keep.append(line)
    SERVER_LOG.write_text("\n".join(keep) + "\n", encoding="utf-8")
    collected = "Server listening" in text and "Connection from" in text
    SUMMARY.write_text(f"client1={first}\ncollected={collected}\n", encoding="utf-8")
    log("summary", f"client1={first} collected={collected}")
    if not collected:
        return 2
    log("logs", f"client1={CLIENT1} sshd={SERVER_LOG} serial={SERIAL} summary={SUMMARY}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
