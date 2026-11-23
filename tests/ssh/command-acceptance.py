#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K514 five-command SSH acceptance: one session per command.

Boots the release image once (existing image, no rebuild), waits for the sshd ready line and
TCP 22, then runs exactly one SSH session per command. Each row requires exit 0 and its expected
stdout bytes, and records the serial segment's child-fault/panic line for that session. One
GREEN/RED line is printed per command; the exit status is 0 only when every row is green.

Usage:
  RAVEN_SSH_IMAGE=build/qtest/ssh-login.img python3 tests/ssh/command-acceptance.py
  python3 tests/ssh/command-acceptance.py --image PATH [--timeout 20] [--keep]

The script never rebuilds an image and does not retry a failed row.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
IMAGE = Path(os.environ.get("RAVEN_SSH_IMAGE", ROOT / "build" / "qtest" / "ssh-login.img"))
OVMF_CODE = Path(os.environ.get("RAVEN_OVMF_CODE", "/usr/share/OVMF/OVMF_CODE_4M.fd"))
OVMF_VARS = Path(os.environ.get("RAVEN_OVMF_VARS", "/usr/share/OVMF/OVMF_VARS_4M.fd"))
ASKPASS = ROOT / "tests" / "ssh" / "askpass.sh"
USER = os.environ.get("RAVEN_SSH_USER", "raven")
PASSWORD = os.environ.get("RAVEN_SSH_PASSWORD", "raven")
HOST = os.environ.get("RAVEN_SSH_HOST", "127.0.0.1")
ROW_TIMEOUT_S = 20.0

FAULT_RE = re.compile(
    r"process: .* killed by page fault at (0x[0-9a-f]+)|"
    r"KERNEL PANIC:.*|K434 alloc-fail.*"
)
ROOT_NAMES = (b"bin", b"dev", b"etc", b"home", b"init", b"lib", b"libexec",
              b"root", b"sbin", b"tmp", b"usr", b"var")


def root_listing(out: bytes) -> bool:
    words = out.split()
    return all(name in words for name in ROOT_NAMES)


ROWS = (
    ("uname", ["uname"], lambda out: out == b"RavenBSD\n", "RavenBSD\\n"),
    ("pwd", ["pwd"], lambda out: out == b"/\n", "/\\n"),
    ("ls", ["ls", "/"], root_listing, "root namespace listing"),
    ("cat-marker", ["cat", "/etc/ssh-matrix-marker"],
     lambda out: out == b"K434-SSH-MARKER-2026-11-22\n",
     "K434-SSH-MARKER-2026-11-22\\n"),
    ("echo-builtin", ["echo", "K514-BUILTIN"],
     lambda out: out == b"K514-BUILTIN\n", "K514-BUILTIN\\n"),
)


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def wait_serial(serial: Path, needle: bytes, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            if needle in serial.read_bytes():
                return True
        except FileNotFoundError:
            pass
        time.sleep(0.2)
    return False


def boot(run: Path, image: Path, port: int) -> tuple[subprocess.Popen, Path]:
    run.mkdir(parents=True, exist_ok=True)
    vars_path = run / "vars.fd"
    shutil.copyfile(OVMF_VARS, vars_path)
    serial = run / "serial.log"
    serial.write_bytes(b"")
    argv = [
        "qemu-system-x86_64", "-machine", "q35", "-m", "512M", "-smp", "4",
        "-enable-kvm", "-cpu", "host",
        "-serial", f"file:{serial}", "-display", "none", "-monitor", "none", "-no-reboot",
        "-snapshot",
        "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"format=raw,file={image}",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-netdev", f"user,id=n0,hostfwd=tcp:127.0.0.1:{port}-:22",
        "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
    ]
    err = run / "qemu.err"
    with open(err, "wb") as err_file:
        child = subprocess.Popen(argv, cwd=ROOT, start_new_session=True,
                                 stdout=subprocess.DEVNULL, stderr=err_file)
    if not wait_serial(serial, b"svcd: sshd up", 180):
        child.kill()
        raise RuntimeError(f"sshd never started; serial {serial}")
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((HOST, port), timeout=1):
                return child, serial
        except OSError:
            time.sleep(0.5)
    child.kill()
    raise RuntimeError(f"guest TCP 22 never listened; serial {serial}")


def run_row(serial: Path, port: int, argv: list[str], timeout: float) -> dict:
    before = len(serial.read_bytes()) if serial.is_file() else 0
    env = dict(os.environ)
    env["SSH_ASKPASS"] = str(ASKPASS)
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["DISPLAY"] = ":0"
    env["K434_SSH_PASSWORD"] = PASSWORD
    ssh = [
        "ssh", "-v", "-p", str(port),
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "ConnectTimeout=8",
        "-o", "NumberOfPasswordPrompts=1",
        f"{USER}@{HOST}",
        *argv,
    ]
    started = time.monotonic()
    child = subprocess.Popen(ssh, cwd=ROOT, stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                             env=env, start_new_session=True)
    timed_out = False
    try:
        stdout, stderr = child.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(child.pid, signal.SIGKILL)
        stdout, stderr = child.communicate()
    elapsed = (time.monotonic() - started) * 1000
    segment = serial.read_bytes()[before:] if serial.is_file() else b""
    fault = next((m.group(0) for m in FAULT_RE.finditer(segment.decode("utf-8", "replace"))), "")
    return {
        "exit": None if timed_out else child.returncode,
        "timed_out": timed_out,
        "stdout": stdout,
        "stderr": stderr,
        "wall_ms": round(elapsed),
        "auth": b"Authenticated to " in stderr,
        "fault": fault,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path, default=IMAGE)
    parser.add_argument("--timeout", type=float, default=ROW_TIMEOUT_S)
    parser.add_argument("--keep", action="store_true", help="leave QEMU running on failure")
    args = parser.parse_args()
    image = args.image if args.image.is_absolute() else ROOT / args.image
    if not image.is_file():
        print(f"command-acceptance: missing image {image}", file=sys.stderr)
        return 2
    for path in (OVMF_CODE, OVMF_VARS, ASKPASS):
        if not path.is_file():
            print(f"command-acceptance: missing {path}", file=sys.stderr)
            return 2
    run = ROOT / "build" / "ssh" / "command-acceptance" / time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    port = free_port()
    child = None
    failures = 0
    print(f"command-acceptance: image={image} port={port} run={run}")
    try:
        child, serial = boot(run, image, port)
        for name, argv, check, expected in ROWS:
            row = run_row(serial, port, argv, args.timeout)
            ok = (not row["timed_out"] and row["exit"] == 0 and check(row["stdout"]))
            if not ok:
                failures += 1
            fault = row["fault"] or "-"
            stderr_tail = row["stderr"].decode("utf-8", "replace").splitlines()[-1:] or [""]
            print(
                f"ROW {'GREEN' if ok else 'RED'} name={name} command={' '.join(argv)} "
                f"auth={int(row['auth'])} exit={row['exit']} timed_out={int(row['timed_out'])} "
                f"stdout_len={len(row['stdout'])} stdout_hex={row['stdout'].hex()} "
                f"expected={expected!r} fault={fault!r} stderr_tail={stderr_tail[0]!r}"
            )
    finally:
        if child is not None and not args.keep:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            child.wait()
    print(f"command-acceptance: {'PASS' if failures == 0 else 'FAIL'} failures={failures}")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
