#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Final K195 tap gate: exact host SSH command, 30 s measured ConnectTimeout.

Exits:
  0  login and wrong-password refusal both behaved
  1  failed before the banner/KEXINIT
  3  reached the banner and KEXINIT, then stopped at the known K238 KEX point
"""

from __future__ import annotations

import os
import shutil
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path

import pexpect

ROOT = Path(__file__).resolve().parents[2]
IMAGE = ROOT / "build" / "qtest" / "ssh-login.img"
RUN = ROOT / "build" / "ssh" / "tap-gate"
OVMF_CODE = Path("/usr/share/OVMF/OVMF_CODE_4M.fd")
OVMF_VARS = Path("/usr/share/OVMF/OVMF_VARS_4M.fd")
TAP = "ravendemo0"
HOST = "10.0.2.15"
USER = "raven"
PASSWORD = "raven"
CONNECT_TIMEOUT = "30"


def qemu_boot() -> tuple[subprocess.Popen, Path, Path]:
    RUN.mkdir(parents=True, exist_ok=True)
    vars_path = RUN / "vars.fd"
    shutil.copyfile(OVMF_VARS, vars_path)
    serial = RUN / "serial.log"
    serial.write_bytes(b"")
    argv = [
        "qemu-system-x86_64", "-machine", "q35", "-m", "512M", "-smp", "4",
        "-serial", f"file:{serial}", "-display", "none", "-monitor", "none",
        "-no-reboot",
        "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"format=raw,file={IMAGE}",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-nic", "none",
        "-netdev", f"tap,id=n0,ifname={TAP},script=no,downscript=no",
        "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
    ]
    child = subprocess.Popen(argv, cwd=ROOT, start_new_session=True,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        data = serial.read_bytes() if serial.is_file() else b""
        if b"svcd: sshd up" in data and b"netd: interface eth0 10.0.2.15/24" in data:
            break
        time.sleep(0.2)
    else:
        kill(child)
        raise RuntimeError(f"sshd/netd did not start; serial {serial}")
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((HOST, 22), timeout=1):
                return child, serial, RUN / "client.log"
        except OSError:
            time.sleep(0.5)
    kill(child)
    raise RuntimeError(f"guest TCP 22 not reachable at {HOST}; serial {serial}")


def kill(child: subprocess.Popen) -> None:
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait()


def ssh_base(command: list[str]) -> list[str]:
    return [
        "ssh", "-vv",
        "-o", f"ConnectTimeout={CONNECT_TIMEOUT}",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        f"{USER}@{HOST}",
        *command,
    ]


def run_client(command: list[str], log_path: Path, password: str) -> tuple[str, str]:
    """Return (stage, output). stage is pass, kex, or early."""
    child = pexpect.spawn(ssh_base(command)[0], ssh_base(command)[1:], encoding="utf-8",
                          timeout=45, cwd=str(ROOT))
    with open(log_path, "w", encoding="utf-8") as log:
        child.logfile = log
        try:
            child.expect(r"(?i)password:\s*", timeout=35)
            child.sendline(password)
            index = child.expect(
                [r"RavenBSD", r"(?i)permission denied", r"(?i)authentication failed",
                 pexpect.EOF, pexpect.TIMEOUT],
                timeout=45,
            )
            child.close()
            if child.logfile is not None:
                child.logfile.flush()
            text = log_path.read_text(errors="replace")
            if index == 0:
                return "pass", text
            if index in (1, 2):
                return "refused", text
            return "kex" if "SSH2_MSG_KEXINIT received" in text else "early", text
        except (pexpect.EOF, pexpect.TIMEOUT):
            if child.logfile is not None:
                child.logfile.flush()
            text = log_path.read_text(errors="replace")
            return "kex" if "SSH2_MSG_KEXINIT received" in text else "early", text
        finally:
            child.close(force=True)


def main() -> int:
    if not IMAGE.is_file():
        print(f"tap-gate: missing image {IMAGE}", file=sys.stderr)
        return 1
    child, serial, client_log = qemu_boot()
    try:
        stage, text = run_client(["ls"], client_log, PASSWORD)
        banner = "SSH2_MSG_KEXINIT received" in text
        print(f"tap-gate: stage={stage} banner_kexinit={banner}")
        if stage == "pass":
            wrong_stage, _ = run_client(["ls"], RUN / "wrong.log", "not-the-password")
            print(f"tap-gate: wrong-password stage={wrong_stage}")
            if wrong_stage == "refused":
                print("tap-gate: PASS")
                return 0
            return 1
        if stage == "kex":
            print("tap-gate: BLOCKED at K238 KEX_ECDH_REPLY after banner/KEXINIT")
            return 3
        print("tap-gate: FAIL before banner/KEXINIT")
        return 1
    finally:
        kill(child)
        tail = serial.read_text(errors="replace").splitlines()[-20:] if serial.is_file() else []
        print("tap-gate: serial tail:")
        for line in tail:
            print(f"  {line}")


if __name__ == "__main__":
    sys.exit(main())
