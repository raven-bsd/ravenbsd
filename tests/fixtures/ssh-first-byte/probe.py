#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""K567 rev 336: timestamp the first SSH connection.

Boots the release ssh-login image over the K194 tap, then records:

  * the host monotonic time when the guest serial prints ``svcd: sshd up`` (the
    service readiness is the sshd ``Server listening on 0.0.0.0 port 22.`` line);
  * the TCP 22 listen time;
  * four real ``ssh -vvv`` attempts with the ``Connection established`` and
    ``Remote protocol version`` lines timestamped;
  * a raw socket loop (six connects) that records the TCP connect time and the
    time of the first banner byte, with an 8 s first-byte bound.

The question is whether the server accepts and writes nothing for more than 8 s
on the first connection (product TX defect) or the client connects before the
listener is ready (readiness race).  The absolute host monotonic values and the
deltas from the ready line make that decision.  The image is whatever
``build/qtest/ssh-login.img`` contains; the release image build is blocked by
the ``raven-theme``/serde regression, so the last good image is used and named.
"""

from __future__ import annotations

import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import time
from pathlib import Path

import pexpect

ROOT = Path(__file__).resolve().parents[3]
IMAGE = Path(os.environ.get("RAVEN_SSH_IMAGE", ROOT / "build" / "qtest" / "ssh-login.img"))
if not IMAGE.is_absolute():
    IMAGE = ROOT / IMAGE
RUN = ROOT / "build" / "ssh" / "first-byte"
OVMF_CODE = Path(os.environ.get("RAVEN_OVMF_CODE", "/usr/share/OVMF/OVMF_CODE_4M.fd"))
OVMF_VARS = Path(os.environ.get("RAVEN_OVMF_VARS", "/usr/share/OVMF/OVMF_VARS_4M.fd"))
TAP = os.environ.get("RAVEN_QEMU_TAP", "ravendemo0")
HOST = os.environ.get("RAVEN_SSH_HOST", "10.0.2.15")
USER = os.environ.get("RAVEN_SSH_USER", "raven")
SSH = pexpect.spawn
FIRST_BYTE_BOUND = 8.0
CLIENT_TIMEOUT = 12.0


def log(kind: str, text: str) -> None:
    print(f"ssh-first-byte: {kind}: {text}", flush=True)


def kill(child: subprocess.Popen) -> None:
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait()


SYS_NET = Path("/sys/class/net")


def tap_present(tap: str, root: Path = SYS_NET) -> bool:
    """True when the tap interface exists in the kernel."""
    return (root / tap).exists()


def ensure_tap(tap: str, root: Path = SYS_NET) -> None:
    """Refuse to boot QEMU on a missing tap.

    ``scripts/qemu-net.sh up`` returns the live contract from a global state file; a
    stale state file makes it exit 0 without creating the tap.  Check the interface
    itself so the failure names the tap instead of leaving an empty serial log.
    """
    if not tap_present(tap, root):
        raise RuntimeError(
            f"tap {tap} is not up (missing {root / tap}); "
            "run scripts/qemu-net.sh up (a stale /run/raven-qemu-net.state can "
            "report success without creating it)"
        )


def stderr_tail(path: Path | None, lines: int = 8) -> str:
    """Last lines of QEMU's stderr, or a marker when it was not captured."""
    if path is None:
        return "not captured"
    try:
        text = path.read_text(encoding="utf-8", errors="replace").splitlines()
    except FileNotFoundError:
        return "not captured"
    return " | ".join(text[-lines:]) or "empty"


def write_serial_header(
    serial: Path, image: Path | str, tap: str, argv: list[str]
) -> None:
    """Seed the serial log with the run context before QEMU starts.

    A guest that never boots still leaves a non-empty, self-describing log: the
    image, the tap and the exact QEMU command line.  The ready-marker scan ignores
    the ``#`` header.
    """
    serial.parent.mkdir(parents=True, exist_ok=True)
    serial.write_text(
        f"# ssh-first-byte image={image} tap={tap} argv={' '.join(argv)}\n",
        encoding="utf-8",
    )


def connect_after_ready(
    serial: Path,
    needle: bytes,
    started: float,
    timeout: float,
    connect,
    child: subprocess.Popen | None = None,
    stderr_path: Path | None = None,
):
    """Wait for the sshd ready marker, then run exactly one connect.

    The gate row must never connect before the service readiness line, or an
    early TCP connect is logged as a banner failure at any host load.  The
    helper is the single seam the unit test drives with a synthetic early
    connect.  When ``child`` is given, a QEMU exit is raised immediately with its
    stderr instead of waiting out the whole timeout on a silent serial.
    """
    ready = wait_serial(
        serial, needle, started, timeout, child=child, stderr_path=stderr_path
    )
    if ready is None:
        return None
    connect()
    return ready


def boot() -> tuple[subprocess.Popen, Path, float]:
    ensure_tap(TAP)
    RUN.mkdir(parents=True, exist_ok=True)
    vars_path = RUN / "vars.fd"
    shutil.copyfile(OVMF_VARS, vars_path)
    serial = RUN / "serial.log"
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
    write_serial_header(serial, IMAGE, TAP, argv)
    qemu_log = RUN / "qemu.log"
    qemu_log.write_bytes(b"")
    stderr_file = open(qemu_log, "wb")
    try:
        child = subprocess.Popen(argv, cwd=ROOT, start_new_session=True,
                                 stdout=subprocess.DEVNULL, stderr=stderr_file)
    finally:
        stderr_file.close()
    started = time.monotonic()
    try:
        ready = connect_after_ready(
            serial, b"svcd: sshd up", started, 180, lambda: None,
            child=child, stderr_path=qemu_log,
        )
        if ready is None:
            raise RuntimeError(
                f"sshd service never started; tap={TAP}; serial {serial}; "
                f"qemu stderr: {stderr_tail(qemu_log)}"
            )
        log("ready", f"svcd: sshd up at t={ready - started:.3f}s")
        if not wait_serial(
            serial, b"netd: interface eth0 10.0.2.15/24", started, 30,
            child=child, stderr_path=qemu_log,
        ):
            raise RuntimeError(
                f"netd never published 10.0.2.15; tap={TAP}; serial {serial}; "
                f"qemu stderr: {stderr_tail(qemu_log)}"
            )
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            code = child.poll()
            if code is not None:
                raise RuntimeError(
                    f"qemu exited {code} before TCP 22 listened; tap={TAP}; "
                    f"qemu stderr: {stderr_tail(qemu_log)}; serial {serial}"
                )
            try:
                with socket.create_connection((HOST, 22), timeout=1):
                    listen = time.monotonic()
                    log("listen", f"tcp {HOST}:22 connect at t={listen - started:.3f}s "
                                  f"(ready delta {listen - ready:.3f}s)")
                    return child, serial, ready
            except OSError:
                time.sleep(0.1)
        raise RuntimeError(
            f"guest TCP 22 never listened on {HOST}; tap={TAP}; serial {serial}; "
            f"qemu stderr: {stderr_tail(qemu_log)}"
        )
    except BaseException:
        kill(child)
        raise


def wait_serial(
    path: Path,
    needle: bytes,
    started: float,
    timeout: float,
    child: subprocess.Popen | None = None,
    stderr_path: Path | None = None,
) -> float | None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if child is not None:
            code = child.poll()
            if code is not None:
                raise RuntimeError(
                    f"qemu exited {code} before the serial printed {needle!r}; "
                    f"tap={TAP}; stderr: {stderr_tail(stderr_path)}; serial {path}"
                )
        try:
            if needle in path.read_bytes():
                return time.monotonic()
        except FileNotFoundError:
            pass
        time.sleep(0.05)
    return None


def ssh_attempt(index: int, started: float, ready: float) -> dict:
    spawn_at = time.monotonic()
    child = SSH(
        "ssh",
        ["-vvv", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=no",
         "-o", "UserKnownHostsFile=/dev/null", "-o", "PreferredAuthentications=password",
         "-o", "PubkeyAuthentication=no", "-o", "ConnectTimeout=8",
         f"{USER}@{HOST}", "true"],
        cwd=str(ROOT), encoding="utf-8", timeout=CLIENT_TIMEOUT,
    )
    connect = None
    banner = None
    first_output = None
    text = []
    try:
        while True:
            try:
                line = child.readline()
            except pexpect.TIMEOUT:
                break
            if not line:
                break
            now = time.monotonic() - started
            if first_output is None:
                first_output = now
            text.append(line.rstrip())
            if "Connection established" in line and connect is None:
                connect = now
            if "Remote protocol version" in line and banner is None:
                banner = now
            if connect is not None and banner is not None:
                break
    finally:
        child.close(force=True)
    log("ssh", f"attempt={index} from_ready={spawn_at - ready:.3f}s "
               f"connect={connect} banner={banner} first_output={first_output}")
    return {"index": index, "from_ready": spawn_at - ready, "connect": connect,
            "banner": banner, "first_output": first_output, "lines": text}


def raw_attempt(index: int, started: float, ready: float) -> dict:
    connect_at = time.monotonic()
    try:
        with socket.create_connection((HOST, 22), timeout=FIRST_BYTE_BOUND) as sock:
            connected = time.monotonic()
            sock.settimeout(FIRST_BYTE_BOUND)
            try:
                data = sock.recv(1)
            except socket.timeout:
                data = b""
            first = time.monotonic()
    except OSError as error:
        log("raw", f"raw={index} from_ready={connect_at - ready:.3f}s connect failed: {error}")
        return {"index": index, "from_ready": connect_at - ready, "connected": None,
                "first": None, "bytes": -1}
    log("raw", f"raw={index} from_ready={connect_at - ready:.3f}s "
               f"connect_delta={connected - connect_at:.3f}s "
               f"first_byte={first - started:.3f}s first_byte_delta={first - connected:.3f}s "
               f"bytes={len(data)}")
    return {"index": index, "from_ready": connect_at - ready, "connected": connected,
            "first": first, "bytes": len(data)}


def main() -> int:
    if not IMAGE.is_file():
        log("error", f"missing {IMAGE}")
        return 2
    for path in (OVMF_CODE, OVMF_VARS):
        if not path.is_file():
            log("error", f"missing firmware {path}")
            return 2
    child, serial, ready = boot()
    try:
        attempts = [ssh_attempt(index + 1, ready, ready) for index in range(4)]
        raws = [raw_attempt(index + 1, ready, ready) for index in range(6)]
    finally:
        kill(child)
    log("summary", f"image={IMAGE}")
    summary = RUN / "summary.txt"
    with open(summary, "w", encoding="utf-8") as out:
        out.write(f"image={IMAGE}\n")
        for result in attempts:
            line = (f"ssh attempt={result['index']} from_ready={result['from_ready']:.3f}s "
                    f"connect={result['connect']} banner={result['banner']}")
            log("summary", line)
            out.write(line + "\n")
        for result in raws:
            delta = (None if result["first"] is None or result["connected"] is None
                     else round(result["first"] - result["connected"], 3))
            line = (f"raw={result['index']} from_ready={result['from_ready']:.3f}s "
                    f"first_byte_delta={delta} bytes={result['bytes']}")
            log("summary", line)
            out.write(line + "\n")
    log("logs", str(RUN))
    return 0


if __name__ == "__main__":
    sys.exit(main())
