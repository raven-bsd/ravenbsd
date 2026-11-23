#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K547 tap accepted-socket comparison in one boot.

Boots the K547 image with a private static tap, logs in to the guest console,
runs the host tap client and the guest loopback client against the same sshd,
and keeps the serial log. The host client result and the sshd `Connection from`
line are the measurement; the guest loopback client is the control.
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
CLIENT = OUT / "host-client.log"
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


def probe_port(host: str, port: int) -> str:
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=3) as connection:
                connection.settimeout(10)
                return f"{connection.recv(32)!r}"
        except OSError:
            time.sleep(1)
    return "no-connect"


def probe_client(host: str) -> str:
    tap = probe_port(host, 23147)
    ppoll = probe_port(host, 23149)
    return f"tap={tap} ppoll={ppoll}"


def host_client(host: str) -> str:
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
            "echo sshd-trace-command-ok",
        ],
        encoding="utf-8",
        codec_errors="replace",
        timeout=30,
    )
    state = "no-password"
    with open(CLIENT, "w", encoding="utf-8") as out:
        child.logfile = out
        try:
            child.expect(r"(?i)password:\s*")
            child.sendline("raven")
            child.expect("sshd-trace-command-ok", timeout=30)
            state = "command-ok"
            child.expect(pexpect.EOF, timeout=20)
            state += "/closed"
        except pexpect.TIMEOUT:
            if state == "command-ok":
                state += "/close-timeout"
        except pexpect.EOF:
            state += "/eof"
        finally:
            child.close(force=True)
    return state


def console(child: pexpect.spawn, command: str, marker: str, timeout: int) -> str:
    child.sendline(command)
    try:
        child.expect(marker, timeout=timeout)
        return "ok"
    except pexpect.TIMEOUT:
        return "timeout"


def main() -> int:
    if not IMAGE.is_file():
        log("error", f"missing {IMAGE}; run tests/fixtures/sshd-trace/build.sh")
        return 2
    for path in (SERIAL, CLIENT, SUMMARY):
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
    tap = "not-run"
    loop = "not-run"
    tap_tail = "not-run"
    with open(SERIAL, "w", encoding="utf-8") as serial:
        child.logfile = serial
        try:
            child.expect("svcd: getty up", timeout=240)
            child.expect(r"login: ?", timeout=120)
            child.sendline("root")
            child.expect(r"(?i)password: ?", timeout=30)
            child.sendline("raven")
            child.expect("raven# ", timeout=120)
            log("console", "root login")
            probe = probe_client(GUEST)
            log("accept-probe", probe)
            try:
                child.expect("sshd-trace-accept: done", timeout=60)
            except pexpect.TIMEOUT:
                log("accept-probe", "done marker timeout")
            if not wait_tcp(GUEST, 22, 120):
                log("error", f"guest sshd did not accept on {GUEST}:22")
                return 2
            log("ready", f"{GUEST}:22")
            tap = host_client(GUEST)
            log("host-tap", tap)
            tap_tail = console(child, "tail -30 /tmp/sshd-trace.log; M=Q; echo ${M}A",
                               "QA", 90)
            log("tap-log", tap_tail)
            console(child, "/bin/brush /etc/ssh/trace-client.sh > /tmp/loop.log 2>&1 & "
                           "M=Q; echo ${M}S", "QS", 90)
            time.sleep(12)
            loop_tail = console(child, "tail -30 /tmp/sshd-trace.log; M=Q; echo ${M}B",
                                "QB", 90)
            log("loop-log", loop_tail)
            console(child, "cat /tmp/loop.log; M=Q; echo ${M}L", "QL", 60)
        finally:
            child.close(force=True)
    text = SERIAL.read_text(encoding="utf-8", errors="replace")
    tap_connected = "Connection from 10.0.2.2" in text
    loop_connected = "Connection from 127.0.0.1" in text
    loop_ok = "sshd-trace-loop-ok" in text
    loop = "ok" if loop_ok else "missing"
    SUMMARY.write_text(
        f"host_tap={tap}\ntap_tail={tap_tail}\ntap_connection_from={tap_connected}\n"
        f"loop_connection_from={loop_connected}\nguest_loop={loop}\n"
        f"accept_probe={probe}\n",
        encoding="utf-8",
    )
    log("summary", f"host_tap={tap} tap_connection_from={tap_connected} "
                   f"loop_connection_from={loop_connected} guest_loop={loop} "
                   f"accept_probe={probe}")
    if not loop_ok:
        return 2
    log("logs", f"serial={SERIAL} client={CLIENT} summary={SUMMARY}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
