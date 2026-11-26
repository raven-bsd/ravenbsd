#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""K446 multi-boot banner/auth stability: 5 boots x 5 fresh connections.

Per attempt:
  1. a fresh TCP connection reads the raw SSH banner; the bytes must be exactly
     ``SSH-2.0-OpenSSH_10.6p2\\r\\n``; zero bytes is a FAIL for that boot/attempt;
  2. a fresh ``ssh -v`` session authenticates with the password and must print
     ``Authenticated ... using "password"`` and exit 0.

Every attempt is recorded (boot, attempt, banner bytes, banner match, auth exit,
wall times) and never retried. Each boot keeps its serial log and its SHA-256.
Evidence: ``build/ssh-bootloop/<UTC>/`` (or `RAVEN_SSH_EVIDENCE`).
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import signal
import socket
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

import pexpect

ROOT = Path(__file__).resolve().parents[2]
IMAGE = Path(os.environ.get("RAVEN_SSH_IMAGE", ROOT / "build" / "qtest" / "ssh-login.img"))
OVMF_CODE = Path(os.environ.get("RAVEN_OVMF_CODE", "/usr/share/OVMF/OVMF_CODE_4M.fd"))
OVMF_VARS = Path(os.environ.get("RAVEN_OVMF_VARS", "/usr/share/OVMF/OVMF_VARS_4M.fd"))
TAP = os.environ.get("RAVEN_QEMU_TAP", "ravendemo0")
HOST = os.environ.get("RAVEN_SSH_HOST", "10.0.2.15")
USER = os.environ.get("RAVEN_SSH_USER", "raven")
PASSWORD = os.environ.get("RAVEN_SSH_PASSWORD", "raven")
BOOTS = int(os.environ.get("RAVEN_SSH_BOOTS", "5"))
ATTEMPTS = int(os.environ.get("RAVEN_SSH_ATTEMPTS", "5"))
EXPECTED_BANNER = b"SSH-2.0-OpenSSH_10.6p2\r\n"

RUN = Path(
    os.environ.get("RAVEN_SSH_EVIDENCE", ROOT / "build" / "ssh-bootloop")
) / datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


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


def ensure_tap() -> None:
    """The K194 tap is non-persistent and must belong to this user: recreate it."""
    if os.environ.get("RAVEN_BOOTLOOP_SUDO", "0") != "1":
        return
    subprocess.run(
        ["sudo", "-n", "sh", "scripts/qemu-net.sh", "down"],
        cwd=ROOT, check=False,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    subprocess.run(
        ["sudo", "-n", "sh", "scripts/qemu-net.sh", "up"],
        cwd=ROOT, check=False,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )


def boot(index: int) -> tuple[subprocess.Popen, Path]:
    ensure_tap()
    boot_dir = RUN / f"boot-{index}"
    boot_dir.mkdir(parents=True, exist_ok=True)
    vars_path = boot_dir / "vars.fd"
    shutil.copyfile(OVMF_VARS, vars_path)
    serial = boot_dir / "serial.log"
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
    qemu_err = boot_dir / "qemu.err"
    with open(qemu_err, "wb") as err:
        child = subprocess.Popen(
            argv, cwd=ROOT, start_new_session=True,
            stdout=subprocess.DEVNULL, stderr=err,
        )
    if not wait_serial(serial, b"svcd: sshd up", 180):
        kill(child)
        tail = qemu_err.read_text(errors="replace").strip()[-400:]
        raise RuntimeError(
            f"boot {index}: sshd never started; serial {serial}; qemu.err={tail}"
        )
    if not wait_serial(serial, b"netd: interface eth0 10.0.2.15/24", 30):
        kill(child)
        raise RuntimeError(f"boot {index}: netd never published 10.0.2.15")
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((HOST, 22), timeout=1):
                return child, serial
        except OSError:
            time.sleep(0.5)
    kill(child)
    raise RuntimeError(f"boot {index}: guest TCP 22 never listened on {HOST}")


def banner_attempt(boot_index: int, attempt: int) -> dict:
    started = time.monotonic()
    data = b""
    error = ""
    try:
        with socket.create_connection((HOST, 22), timeout=8) as connection:
            connection.settimeout(8)
            while b"\r\n" not in data and len(data) < 4096:
                chunk = connection.recv(4096)
                if not chunk:
                    break
                data += chunk
    except OSError as exc:
        error = str(exc)
    raw_path = RUN / f"boot-{boot_index}" / f"attempt-{attempt}.banner"
    raw_path.write_bytes(data)
    return {
        "banner_bytes": len(data),
        "banner_exact": data == EXPECTED_BANNER,
        "banner_wall_ms": round((time.monotonic() - started) * 1000.0, 1),
        "banner_error": error,
        "banner_raw": data.decode("ascii", errors="replace").rstrip("\r\n"),
    }


def ssh_args() -> list[str]:
    return [
        "-v",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "ConnectTimeout=8",
        "-o", "NumberOfPasswordPrompts=1",
        f"{USER}@{HOST}",
        "true",
    ]


def auth_attempt(boot_index: int, attempt: int) -> dict:
    log_path = RUN / f"boot-{boot_index}" / f"attempt-{attempt}.ssh.log"
    started = time.monotonic()
    authenticated = ""
    exit_status = None
    with open(log_path, "w", encoding="utf-8") as logfile:
        child = pexpect.spawn(
            "ssh", ssh_args(), cwd=str(ROOT), encoding="utf-8", timeout=15
        )
        child.logfile = logfile
        try:
            child.expect(r"(?i)password:\s*")
            child.sendline(PASSWORD)
            child.expect([pexpect.EOF, pexpect.TIMEOUT], timeout=15)
        except (pexpect.TIMEOUT, pexpect.EOF):
            pass
        exit_status = child.exitstatus
        child.close(force=True)
    text = log_path.read_text(encoding="utf-8", errors="replace")
    for line in text.splitlines():
        match = re.search(r'Authenticated to .* using "password"', line)
        if match:
            authenticated = match.group(0)
            break
    return {
        "auth_exit": exit_status,
        "auth_wall_ms": round((time.monotonic() - started) * 1000.0, 1),
        "auth_line": authenticated,
        "auth_ok": bool(authenticated) and exit_status == 0,
    }


def main() -> int:
    if not IMAGE.is_file():
        print(f"bootloop: missing image {IMAGE}", file=sys.stderr)
        return 2
    RUN.mkdir(parents=True, exist_ok=True)
    rows: list[dict] = []
    failures = 0
    for boot_index in range(1, BOOTS + 1):
        try:
            child, serial = boot(boot_index)
        except RuntimeError as exc:
            print(f"bootloop: boot {boot_index} failed: {exc}", file=sys.stderr)
            for attempt in range(1, ATTEMPTS + 1):
                rows.append({
                    "boot": boot_index, "attempt": attempt, "banner_bytes": 0,
                    "banner_exact": False, "banner_wall_ms": 0, "banner_error": str(exc),
                    "auth_exit": None, "auth_wall_ms": 0, "auth_line": "", "auth_ok": False,
                    "serial_sha256": "", "result": "FAIL", "note": f"boot failed: {exc}",
                })
                failures += 1
            continue
        try:
            for attempt in range(1, ATTEMPTS + 1):
                banner = banner_attempt(boot_index, attempt)
                auth = auth_attempt(boot_index, attempt)
                serial_sha = sha256(serial)
                ok = banner["banner_exact"] and auth["auth_ok"]
                note = ""
                if banner["banner_bytes"] == 0:
                    note = "zero-byte banner"
                elif not banner["banner_exact"]:
                    note = "banner mismatch"
                elif not auth["auth_line"]:
                    note = "no Authenticated line"
                elif auth["auth_exit"] != 0:
                    note = f"session exit {auth['auth_exit']}"
                if auth["auth_line"]:
                    note = (note + "; authenticated").lstrip("; ")
                row = {
                    "boot": boot_index,
                    "attempt": attempt,
                    "banner_bytes": banner["banner_bytes"],
                    "banner_exact": banner["banner_exact"],
                    "banner_wall_ms": banner["banner_wall_ms"],
                    "banner_error": banner["banner_error"],
                    **auth,
                    "serial_sha256": serial_sha,
                    "result": "pass" if ok else "FAIL",
                    "note": note,
                }
                rows.append(row)
                print(
                    f"boot {boot_index} attempt {attempt}: bytes={row['banner_bytes']} "
                    f"banner={'ok' if row['banner_exact'] else 'FAIL'} "
                    f"auth={row['auth_exit']} wall={row['banner_wall_ms'] + row['auth_wall_ms']:.0f}ms "
                    f"{row['result']} {note}",
                    flush=True,
                )
                if not ok:
                    failures += 1
                    if note == "zero-byte banner":
                        print(f"bootloop: FAIL zero bytes at boot {boot_index} attempt {attempt}", file=sys.stderr)
        finally:
            kill(child)

    columns = [
        "boot", "attempt", "banner_bytes", "banner_exact", "banner_wall_ms",
        "auth_exit", "auth_wall_ms", "auth_line", "auth_ok", "serial_sha256",
        "result", "note",
    ]
    with open(RUN / "attempts.tsv", "w", encoding="utf-8") as out:
        out.write("\t".join(columns) + "\n")
        for row in rows:
            out.write("\t".join(str(row[c]) for c in columns) + "\n")
    summary = {
        "image": str(IMAGE),
        "image_sha256": sha256(IMAGE),
        "boots": BOOTS,
        "attempts_per_boot": ATTEMPTS,
        "attempts": len(rows),
        "passed": len(rows) - failures,
        "failed": failures,
        "zero_byte_attempts": sum(1 for r in rows if r["note"] == "zero-byte banner"),
        "result": "pass" if failures == 0 else "FAIL",
    }
    (RUN / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    (RUN / "summary.txt").write_text(
        f"bootloop: {summary['result']} {summary['passed']}/{summary['attempts']} "
        f"boots={BOOTS} attempts/boot={ATTEMPTS} image={IMAGE} sha256={summary['image_sha256']}\n",
        encoding="utf-8",
    )
    with open(RUN / "SHA256SUMS", "w", encoding="utf-8") as out:
        for path in sorted(RUN.rglob("*")):
            if path.is_file() and path.name != "SHA256SUMS":
                out.write(f"{sha256(path)}  {path.relative_to(RUN)}\n")
    print(f"bootloop: evidence={RUN} {summary['passed']}/{summary['attempts']} {summary['result']}")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
