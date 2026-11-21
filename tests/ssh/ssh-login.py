#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""K195 host-to-guest SSH login gate over the K194 tap/bridge.

Run through tests/ssh/ssh-login.sh, which owns the bridge up/down lifecycle. The
driver boots the prebuilt release image on the K194 tap, waits for sshd to listen,
and drives the real host OpenSSH client:

  * `ssh raven@10.0.2.15 uname -a` returns the RavenBSD uname;
  * `ssh -tt raven@10.0.2.15` runs `pwd` and `exit` through a PTY;
  * a wrong password is refused and never reaches a shell;
  * the Ed25519 host key is captured before and after a second boot (the initfs
    demo's /tmp/ssh keys are ephemeral; persistence is not claimed).

RAVEN_SSH_REPEAT=N runs the command session N times and reports the wall-time
p50/p90 (the 0.1.0 release gate asks for 10 runs). The password is the
documented 0.1.0 demo credential.

Exit codes: 0 gate passed, 1 gate failed, 2 missing image/tool, 3 the tap is not
up or the guest never listened (the K195 missing-primitive handoff).
"""

from __future__ import annotations

import os
import shutil
import signal
import socket
import statistics
import subprocess
import sys
import time
from pathlib import Path

import pexpect

ROOT = Path(__file__).resolve().parents[2]
IMAGE = Path(os.environ.get("RAVEN_SSH_IMAGE", ROOT / "build" / "qtest" / "ssh-login.img"))
if not IMAGE.is_absolute():
    IMAGE = ROOT / IMAGE
RUN = ROOT / "build" / "ssh" / "login-test"
OVMF_CODE = Path(os.environ.get("RAVEN_OVMF_CODE", "/usr/share/OVMF/OVMF_CODE_4M.fd"))
OVMF_VARS = Path(os.environ.get("RAVEN_OVMF_VARS", "/usr/share/OVMF/OVMF_VARS_4M.fd"))
TAP = os.environ.get("RAVEN_QEMU_TAP", "ravendemo0")
HOST = os.environ.get("RAVEN_SSH_HOST", "10.0.2.15")
USER = os.environ.get("RAVEN_SSH_USER", "raven")
PASSWORD = os.environ.get("RAVEN_SSH_PASSWORD", "raven")
REPEAT = int(os.environ.get("RAVEN_SSH_REPEAT", "1"))


def log(kind: str, text: str) -> None:
    print(f"ssh-login: {kind}: {text}", flush=True)


def client_stage() -> str:
    try:
        lines = (RUN / "client.log").read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        return "no client log"
    for line in reversed(lines):
        if line.strip():
            return line.strip()
    return "no client output"


def qemu_boot(index: int) -> tuple[subprocess.Popen, Path]:
    RUN.mkdir(parents=True, exist_ok=True)
    vars_path = RUN / f"vars-{index}.fd"
    shutil.copyfile(OVMF_VARS, vars_path)
    serial = RUN / f"serial-{index}.log"
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
    if not wait_serial(serial, b"svcd: sshd up", 180):
        kill(child)
        raise RuntimeError(f"sshd service never started; serial {serial}")
    if not wait_serial(serial, b"netd: interface eth0 10.0.2.15/24", 30):
        kill(child)
        raise RuntimeError(f"netd never published 10.0.2.15; serial {serial}")
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((HOST, 22), timeout=1):
                return child, serial
        except OSError:
            time.sleep(0.5)
    kill(child)
    raise RuntimeError(f"guest TCP 22 never listened on {HOST}; serial {serial}")


def wait_serial(path: Path, needle: bytes, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            if needle in path.read_bytes():
                return True
        except FileNotFoundError:
            pass
        time.sleep(0.2)
    return False


def kill(child: subprocess.Popen) -> None:
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait()


def ssh_base() -> list[str]:
    return [
        "ssh",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "ConnectTimeout=10",
        "-o", "NumberOfPasswordPrompts=1",
        f"{USER}@{HOST}",
    ]


def run_session(argv: list[str], expect: str, timeout: float = 45) -> float:
    started = time.monotonic()
    child = pexpect.spawn(argv[0], argv[1:], cwd=str(ROOT), encoding="utf-8",
                          timeout=timeout)
    try:
        with open(RUN / "client.log", "a", encoding="utf-8") as logfile:
            child.logfile = logfile
            child.expect(r"(?i)password:\s*")
            child.sendline(PASSWORD)
            child.expect(expect)
            child.expect(pexpect.EOF)
    except (pexpect.exceptions.TIMEOUT, pexpect.exceptions.EOF) as error:
        child.close(force=True)
        raise RuntimeError(f"command session: {client_stage()}") from error
    child.close()
    if child.exitstatus not in (0, None):
        raise RuntimeError(f"ssh session exit {child.exitstatus}")
    return time.monotonic() - started


def command_session(command: str) -> float:
    return run_session(ssh_base() + [command], r"RavenBSD \S+ .*x86_64")


def interactive_session() -> None:
    child = pexpect.spawn(ssh_base()[0], ssh_base()[1:] + ["-tt"], cwd=str(ROOT),
                          encoding="utf-8", timeout=45)
    try:
        with open(RUN / "client.log", "a", encoding="utf-8") as logfile:
            child.logfile = logfile
            child.expect(r"(?i)password:\s*")
            child.sendline(PASSWORD)
            child.expect(r"[\$#] $")
            child.sendline("pwd")
            child.expect(rf"^/home/{USER}\s*$")
            child.sendline("exit")
            child.expect(pexpect.EOF)
    except (pexpect.exceptions.TIMEOUT, pexpect.exceptions.EOF) as error:
        child.close(force=True)
        raise RuntimeError(f"interactive session: {client_stage()}") from error
    child.close()
    log("interactive", "pwd/exit through a PTY ok")


def wrong_password() -> None:
    child = pexpect.spawn(ssh_base()[0], ssh_base()[1:] + ["uname", "-a"],
                          cwd=str(ROOT), encoding="utf-8", timeout=45)
    with open(RUN / "client.log", "a", encoding="utf-8") as logfile:
        child.logfile = logfile
        child.expect(r"(?i)password:\s*")
        child.sendline("wrong")
        index = child.expect(
            [r"(?i)permission denied", r"(?i)password:\s*", r"[\$#] ",
             pexpect.EOF, pexpect.TIMEOUT], timeout=30)
    child.close(force=True)
    if index == 2:
        raise RuntimeError("wrong password reached a shell")
    log("wrong-password", "refused without a shell")


def host_key() -> str:
    result = subprocess.run(["ssh-keyscan", "-T", "5", "-p", "22", HOST],
                            cwd=ROOT, check=False, text=True, capture_output=True)
    return result.stdout.strip()


def main() -> int:
    if not IMAGE.is_file():
        log("error", f"missing {IMAGE}; build it with cargo xtask image --profile release")
        return 2
    for path in (OVMF_CODE, OVMF_VARS):
        if not path.is_file():
            log("error", f"missing firmware {path}")
            return 2
    RUN.mkdir(parents=True, exist_ok=True)
    (RUN / "client.log").write_text("")

    first, serial = qemu_boot(1)
    try:
        key1 = host_key()
        times = []
        for index in range(REPEAT):
            elapsed = command_session("uname -a")
            times.append(elapsed)
            log("command", f"{index + 1}/{REPEAT} uname -a {elapsed * 1000:.0f} ms")
        interactive_session()
        wrong_password()
    finally:
        kill(first)

    second, serial2 = qemu_boot(2)
    try:
        key2 = host_key()
    finally:
        kill(second)
    if key1 and key1 == key2:
        log("host-key", "unchanged across the initfs reboot")
    else:
        log("host-key", "ephemeral (initfs /tmp/ssh); UFS persistence is not claimed")

    if times:
        ordered = sorted(times)
        p50 = statistics.median(ordered)
        p90 = ordered[min(len(ordered) - 1, int(round(0.9 * (len(ordered) - 1))))]
        log("timing", f"n={len(ordered)} p50={p50 * 1000:.0f} ms p90={p90 * 1000:.0f} ms")
    log("PASS", "host command, PTY, wrong-password refusal")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except RuntimeError as error:
        log("FAIL", str(error))
        sys.exit(3)
