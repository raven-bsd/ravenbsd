#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K498 sshd session-close trace.

Boots the release image with the K498 DEBUG3 sshd overlay over a private QEMU
user-net host forward, makes one password login and one command from the host,
then logs into the guest serial console and prints /tmp/sshd-trace.log. The full
serial transcript and the host client log are the trace artifacts.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

import pexpect

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "build" / "fixtures" / "sshd-trace"
IMAGE = OUT / "sshd-trace.img"
SERIAL = OUT / "serial.log"
CLIENT = OUT / "client.log"
OVMF_CODE = Path(os.environ.get("RAVEN_OVMF_CODE", "/usr/share/OVMF/OVMF_CODE_4M.fd"))
OVMF_VARS = Path(os.environ.get("RAVEN_OVMF_VARS", "/usr/share/OVMF/OVMF_VARS_4M.fd"))
PORT = os.environ.get("SSHD_TRACE_PORT", "2498")


def log(kind: str, text: str) -> None:
    print(f"sshd-trace: {kind}: {text}", flush=True)


def qemu() -> pexpect.spawn:
    vars_path = OUT / "vars.fd"
    shutil.copyfile(OVMF_VARS, vars_path)
    argv = [
        "-machine", "q35", "-m", "512M", "-smp", "4",
        "-serial", "stdio", "-display", "none", "-monitor", "none", "-no-reboot",
        "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"format=raw,file={IMAGE}",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-nic", "none",
        "-netdev", f"user,id=n0,hostfwd=tcp:127.0.0.1:{PORT}-:22",
        "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
    ]
    child = pexpect.spawn("qemu-system-x86_64", argv, cwd=str(ROOT),
                          encoding="utf-8", codec_errors="replace", timeout=180)
    child.logfile = open(SERIAL, "w", encoding="utf-8")
    return child


def client() -> str:
    child = pexpect.spawn(
        "ssh",
        [
            "-p", PORT,
            "-o", "StrictHostKeyChecking=no",
            "-o", "UserKnownHostsFile=/dev/null",
            "-o", "PreferredAuthentications=password",
            "-o", "PubkeyAuthentication=no",
            "-o", "ConnectTimeout=10",
            "-o", "NumberOfPasswordPrompts=1",
            "raven@127.0.0.1",
            "echo sshd-trace-command-ok",
        ],
        cwd=str(ROOT), encoding="utf-8", codec_errors="replace", timeout=30,
    )
    with open(CLIENT, "w", encoding="utf-8") as logfile:
        child.logfile = logfile
        state = "no-password"
        try:
            child.expect(r"(?i)password:\s*")
            child.sendline("raven")
            child.expect("sshd-trace-command-ok", timeout=30)
            state = "command-ok"
            child.expect(pexpect.EOF, timeout=15)
            state = "closed"
        except pexpect.TIMEOUT:
            if state == "command-ok":
                state = "command-ok-close-timeout"
        except pexpect.EOF:
            pass
        child.close(force=True)
    log("client", state)
    return state


def console(child: pexpect.spawn) -> None:
    child.expect(r"login: ?", timeout=120)
    child.sendline("root")
    child.expect(r"(?i)password: ?", timeout=30)
    child.sendline("raven")
    child.expect("raven# ", timeout=60)
    child.sendline("cat /tmp/sshd-trace.log; echo sshd-trace: log-end")
    child.expect("sshd-trace: log-end", timeout=180)
    child.sendline("exit")
    try:
        child.expect("init exited", timeout=30)
    except pexpect.TIMEOUT:
        pass


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    for path in (SERIAL, CLIENT):
        path.unlink(missing_ok=True)
    child = qemu()
    try:
        child.expect("svcd: sshd up", timeout=180)
        log("boot", "sshd up")
        state = client()
        console(child)
    finally:
        child.close(force=True)
    log("serial", str(SERIAL))
    return 0 if state.startswith("command-ok") else 2


if __name__ == "__main__":
    sys.exit(main())
