#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""Host-to-guest SSH login test over the K194 tap helper.

Requires `scripts/qemu-net.sh up` (the wrapper does that). Boots the prebuilt release image with
the tap device, waits for sshd, then drives the host OpenSSH client for a command, an interactive
PTY session and a wrong-password refusal. The initfs host key is captured before and after a
second boot so the ephemeral-key behaviour is visible.
"""

from __future__ import annotations

import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
IMAGE = ROOT / "build" / "qtest" / "ssh-login.img"
OVMF_CODE = Path("/usr/share/OVMF/OVMF_CODE_4M.fd")
OVMF_VARS = Path("/usr/share/OVMF/OVMF_VARS_4M.fd")
RUN = ROOT / "build" / "ssh" / "login-test"


def wait_log(path: Path, needle: bytes, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.is_file() and needle in path.read_bytes():
            return True
        time.sleep(0.1)
    return False


def qemu_boot(index: int) -> subprocess.Popen:
    RUN.mkdir(parents=True, exist_ok=True)
    vars_path = RUN / f"vars-{index}.fd"
    shutil.copyfile(OVMF_VARS, vars_path)
    log = RUN / f"serial-{index}.log"
    log.write_bytes(b"")
    argv = [
        "qemu-system-x86_64",
        "-machine", "q35",
        "-m", "512M",
        "-smp", "4",
        "-serial", f"file:{log}",
        "-display", "none",
        "-monitor", "none",
        "-no-reboot",
        "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"format=raw,file={IMAGE}",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-nic", "none",
        "-netdev", "tap,id=n0,ifname=ravendemo0,script=no,downscript=no",
        "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
    ]
    child = subprocess.Popen(argv, cwd=ROOT, start_new_session=True)
    if not wait_log(log, b"svcd: sshd up", 60):
        kill(child)
        raise RuntimeError(f"sshd did not start; serial {log}")
    if not wait_log(log, b"netd: interface eth0 10.0.2.15/24", 10):
        kill(child)
        raise RuntimeError(f"netd did not get 10.0.2.15; serial {log}")
    # The service is spawned before sshd finishes generating keys and binding; wait for TCP 22.
    import socket

    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(("10.0.2.15", 22), timeout=1):
                return child
        except OSError:
            time.sleep(0.5)
    kill(child)
    raise RuntimeError(f"sshd did not listen on 10.0.2.15:22; serial {log}")


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
        "root@10.0.2.15",
    ]


def host_key() -> str:
    result = subprocess.run(
        ["ssh-keyscan", "-T", "5", "-p", "22", "10.0.2.15"],
        cwd=ROOT,
        check=False,
        text=True,
        capture_output=True,
    )
    return result.stdout.strip()


def command_session() -> str:
    import pexpect

    child = pexpect.spawn(ssh_base()[0], ssh_base()[1:] + ["uname -a"], cwd=str(ROOT),
                          encoding="utf-8", timeout=30)
    child.logfile = sys.stdout
    child.expect(r"(?i)password:\s*")
    child.sendline("raven")
    child.expect(r"RavenBSD .*x86_64")
    child.expect(pexpect.EOF)
    child.close()
    if child.exitstatus not in (0, None):
        raise RuntimeError(f"ssh command exit {child.exitstatus}")
    return "RavenBSD uname-ok"


def interactive_session() -> str:
    import pexpect

    child = pexpect.spawn(ssh_base()[0], ssh_base()[1:] + ["-tt"], cwd=str(ROOT),
                          encoding="utf-8", timeout=30)
    child.logfile = sys.stdout
    child.expect(r"(?i)password:\s*")
    child.sendline("raven")
    child.expect(r"raven#\s")
    child.sendline("pwd")
    child.expect(r"^/\s*$")
    child.sendline("exit")
    child.expect(pexpect.EOF)
    child.close()
    return "ssh -tt pwd=ok"


def wrong_password() -> str:
    import pexpect

    child = pexpect.spawn(ssh_base()[0], ssh_base()[1:] + ["uname", "-a"], cwd=str(ROOT),
                          encoding="utf-8", timeout=30)
    child.logfile = sys.stdout
    child.expect(r"(?i)password:\s*")
    child.sendline("wrong")
    child.expect(r"(?i)permission denied|password:\s*")
    if "raven# " in (child.before or ""):
        raise RuntimeError("wrong password reached a shell")
    child.close(force=True)
    return "wrong-password rejected"


def main() -> int:
    if not IMAGE.is_file():
        print(f"ssh-login: missing image {IMAGE}", file=sys.stderr)
        return 2
    results = []
    first = qemu_boot(1)
    try:
        key1 = host_key()
        results.append(command_session())
        results.append(interactive_session())
        results.append(wrong_password())
    finally:
        kill(first)

    second = qemu_boot(2)
    try:
        key2 = host_key()
    finally:
        kill(second)
    if key1 and key1 == key2:
        results.append("host-key unchanged across initfs reboot")
    else:
        results.append("host-key ephemeral (initfs /tmp/ssh; UFS persistence not claimed)")

    print("ssh-login: PASS")
    for result in results:
        print(f"ssh-login: {result}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
