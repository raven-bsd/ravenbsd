#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K495 rev185 no-brush COW/exec probe runner (test-only).

Boots a release image with /bin/k495-cow overlaid, waits for sshd, and runs the fixture's `fork`
and `spawn` modes over `ssh -tt`. The fixture prints the parent and child page addresses and the
first/last bytes (0xAA) for a private anonymous COW fork and for a file-backed MAP_SHARED mapping
with a fork+exec child. It does not use brush.

Environment:
  K495_COW_IMAGE   image path (default build/ssh/k495-rev185/cow.img)
  K495_COW_OUT     artifact directory (default /tmp/opencode/k495-cow-run)
  K495_COW_PORT    host ssh port (ephemeral when unset)
"""

from __future__ import annotations

import os
import shutil
import signal
import socket
import subprocess
import time
from pathlib import Path

ROOT = Path("/media/bo/Rustworx/work/ravenbsd-minions/phil/ravenbsd")
IMAGE = Path(os.environ.get("K495_COW_IMAGE", ROOT / "build/ssh/k495-rev185/cow.img"))
OUT = Path(os.environ.get("K495_COW_OUT", "/tmp/opencode/k495-cow-run"))
OVMF_CODE = Path("/usr/share/OVMF/OVMF_CODE_4M.fd")
OVMF_VARS = Path("/usr/share/OVMF/OVMF_VARS_4M.fd")
ASKPASS = ROOT / "tests/ssh/askpass.sh"


def choose_port() -> int:
    value = os.environ.get("K495_COW_PORT")
    if value:
        return int(value)
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.bind(("127.0.0.1", 0))
        return int(probe.getsockname()[1])


PORT = choose_port()
SERIAL = OUT / "serial.log"
OUT.mkdir(parents=True, exist_ok=True)
shutil.copyfile(OVMF_VARS, OUT / "vars.fd")
SERIAL.write_bytes(b"")

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


def ssh_env() -> dict[str, str]:
    env = dict(os.environ)
    env["SSH_ASKPASS"] = str(ASKPASS)
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["DISPLAY"] = ":0"
    env["K434_SSH_PASSWORD"] = "raven"
    return env


def run(command: str, timeout: int = 25) -> tuple[int, str]:
    ssh = [
        "ssh", "-tt", "-p", str(PORT),
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
        result = subprocess.run(ssh, cwd=str(ROOT), stdin=subprocess.DEVNULL,
                                capture_output=True, timeout=timeout,
                                env=ssh_env(), start_new_session=True)
        return result.returncode, result.stdout.decode("utf-8", "replace")
    except subprocess.TimeoutExpired as error:
        return -1, (error.stdout or b"").decode("utf-8", "replace")


def main() -> int:
    deadline = time.monotonic() + 150
    while time.monotonic() < deadline:
        if qemu.poll() is not None:
            break
        if b"svcd: sshd up" in SERIAL.read_bytes():
            break
        time.sleep(0.2)

    failures: list[str] = []
    for mode in ("fork", "spawn"):
        exit_code, output = run(f"/bin/k495-cow {mode}")
        (OUT / f"{mode}.log").write_text(output, encoding="utf-8")
        print(f"K495-COW-RUN mode={mode} exit={exit_code}", flush=True)
        lines = [line for line in output.splitlines() if "K495-COW" in line]
        for line in lines:
            print(line, flush=True)
        if not lines:
            failures.append(f"{mode}: no fixture output")
            continue
        if mode == "fork":
            child = [line for line in lines if "fork-child" in line]
            if not child or "first=aa" not in child[0] or "last=aa" not in child[0]:
                failures.append("fork: child did not read 0xAA")
            if not any("status=0" in line for line in lines):
                failures.append("fork: child status not 0")
        else:
            child = [line for line in lines if "exec-child" in line]
            if not child or "first=aa" not in child[0] or "last=aa" not in child[0]:
                failures.append("spawn: exec child did not read 0xAA")
            if not any("status=0" in line for line in lines):
                failures.append("spawn: child status not 0")

    if failures:
        print(f"K495-COW-RUN FAIL reasons={';'.join(failures)}", flush=True)
        return 1
    print("K495-COW-RUN PASS", flush=True)
    return 0


try:
    raise SystemExit(main())
finally:
    kill()
