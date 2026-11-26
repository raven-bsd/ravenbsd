#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""K195 host-to-guest SSH gate with failure classification.

Run through tests/ssh/ssh-login.sh, which owns the K194 bridge up/down cycle and
needs the prebuilt release image. One command runs the whole gate:

  * ten command sessions `ssh raven@<guest> ls` (RAVEN_SSH_REPEAT overrides),
  * the wrong-password refusal,
  * the interactive `ssh -tt` PTY case (`pwd`/`exit`),
  * a raw byte probe with the K375 fake peer tracer before the sessions.

A failure is classified and quoted with its evidence:

  a  banner not received   - bytes arrived but the client never saw SSH-2.0,
  b  TX wedge              - the peer read 0 bytes on a fresh connection
                             (TxError::Busy / no bytes), client stuck at banner,
  c  auth refused          - the password prompt was reached and the password
                             was rejected (or the wrong password was accepted),
  d  session/command       - authenticated but the command/PTY did not complete.

Exit codes: 0 pass, 2 setup error, 10 class a, 11 class b, 12 class c,
13 class d. A non-zero exit with class b and the 0-byte evidence is a valid
partial gate result while K382 (virtio-net TX) is open; login stays unclaimed.
"""

from __future__ import annotations

import os
import re
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
REPEAT = int(os.environ.get("RAVEN_SSH_REPEAT", "10"))
PEER_SOURCE = ROOT / "tests" / "fixtures" / "ssh-peer" / "peer.c"
PEER = Path(os.environ.get("RAVEN_SSH_PEER", RUN / "ssh-peer"))

EXIT_PASS = 0
EXIT_SETUP = 2
EXIT_BANNER = 10
EXIT_TXWEDGE = 11
EXIT_AUTH = 12
EXIT_SESSION = 13


def log(kind: str, text: str) -> None:
    print(f"ssh-login: {kind}: {text}", flush=True)


def build_peer() -> bool:
    """Build the K375 fake peer tracer; return True when a binary is usable."""
    if PEER.is_file() and PEER_SOURCE.is_file():
        if PEER.stat().st_mtime >= PEER_SOURCE.stat().st_mtime:
            return True
    cc = shutil.which("cc") or shutil.which("gcc") or shutil.which("clang")
    if cc is None or not PEER_SOURCE.is_file():
        return False
    PEER.parent.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        [cc, "-O2", "-Wall", "-Wextra", "-o", str(PEER), str(PEER_SOURCE)],
        cwd=ROOT, capture_output=True, text=True, timeout=30, check=False)
    if result.returncode != 0:
        log("warn", f"peer tracer build failed: {result.stderr.strip()[:200]}")
        return False
    return True


def raw_probe() -> dict:
    """One raw connection with the full fake-peer exchange.

    The peer sends its banner, KEXINIT and KEX_ECDH_INIT; a healthy guest answers
    with its banner and at least one record (its KEXINIT). `total == 0` is the
    no-bytes wedge; `records == 0` after the banner is the same TX wedge (the
    answer never left the guest).
    """
    if build_peer():
        path = RUN / "raw-probe.log"
        result = subprocess.run(
            [str(PEER), "--host", HOST, "--port", "22", "--mode", "client",
             "--send-banner", "--send-kexinit", "--send-kex-ecdh-init",
             "--idle-ms", "3000", "--log", str(path)],
            cwd=ROOT, capture_output=True, text=True, timeout=25, check=False)
        text = path.read_text(errors="replace") if path.is_file() else result.stdout
        total_match = re.search(r"total_read=(\d+)", text)
        records_match = re.search(r"total_records=(\d+)", text)
        total = int(total_match.group(1)) if total_match else -1
        records = int(records_match.group(1)) if records_match else -1
        summary = next((line for line in reversed(text.splitlines())
                        if "idle" in line or "eof" in line or "error" in line), "")
        transcript = [line.strip() for line in text.splitlines()
                      if line.strip() and not line.startswith("0000")]
        return {"source": "tests/fixtures/ssh-peer", "total": total,
                "records": records, "summary": summary.strip(),
                "transcript": transcript[-4:], "log": str(path)}
    # Fallback: a plain socket read, same 0-byte question.
    deadline = time.monotonic() + 5
    while True:
        try:
            with socket.create_connection((HOST, 22), timeout=2) as sock:
                sock.settimeout(3)
                try:
                    data = sock.recv(4096)
                except socket.timeout:
                    data = b""
                return {"source": "socket", "total": len(data), "records": -1,
                        "summary": f"socket recv {len(data)} bytes",
                        "transcript": [], "log": ""}
        except OSError as error:
            if time.monotonic() >= deadline:
                return {"source": "socket", "total": -1, "records": -1,
                        "summary": f"connect failed: {error}", "transcript": [],
                        "log": ""}
            time.sleep(0.3)


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
        "-o", "ConnectTimeout=8",
        "-o", "NumberOfPasswordPrompts=1",
        f"{USER}@{HOST}",
    ]


def client_evidence(path: Path) -> str:
    """The client line that names the stage, for the classification evidence."""
    try:
        text = path.read_text(errors="replace")
    except FileNotFoundError:
        return ""
    for needle in ("banner exchange", "kex_exchange_identification", "Connection closed",
                   "Permission denied", "Connection timed out", "Connection refused"):
        for line in reversed(text.splitlines()):
            if needle.lower() in line.lower():
                return line.strip()
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    return lines[-1] if lines else ""


def command_attempt(index: int) -> dict:
    path = RUN / f"attempt-{index}.log"
    started = time.monotonic()
    child = pexpect.spawn(ssh_base()[0], ssh_base()[1:] + ["ls"], cwd=str(ROOT),
                          encoding="utf-8", timeout=12)
    prompt = False
    kind = "banner"
    with open(path, "w", encoding="utf-8") as logfile:
        child.logfile = logfile
        try:
            child.expect(r"(?i)password:\s*")
            prompt = True
            child.sendline(PASSWORD)
            match = child.expect([r"RavenBSD", r"(?i)permission denied", r"(?i)bin\\s*",
                                  pexpect.EOF, pexpect.TIMEOUT], timeout=12)
            if match == 0 or match == 2:
                kind = "ok"
            elif match == 1:
                kind = "auth-refused"
            else:
                kind = "session"
        except pexpect.TIMEOUT:
            kind = "banner" if not prompt else "session"
        except pexpect.EOF:
            kind = "banner" if not prompt else "session"
        child.close(force=True)
    return {"kind": kind, "seconds": time.monotonic() - started,
            "evidence": client_evidence(path), "log": str(path)}


def interactive_attempt() -> dict:
    path = RUN / "attempt-pty.log"
    started = time.monotonic()
    child = pexpect.spawn(ssh_base()[0], ssh_base()[1:] + ["-tt"], cwd=str(ROOT),
                          encoding="utf-8", timeout=12)
    prompt = False
    kind = "banner"
    with open(path, "w", encoding="utf-8") as logfile:
        child.logfile = logfile
        try:
            child.expect(r"(?i)password:\s*")
            prompt = True
            child.sendline(PASSWORD)
            child.expect(r"[\$#] ", timeout=12)
            child.sendline("pwd")
            child.expect(rf"^/home/{USER}\s*$", timeout=12)
            child.sendline("exit")
            child.expect(pexpect.EOF, timeout=12)
            kind = "ok"
        except pexpect.TIMEOUT:
            kind = "banner" if not prompt else "session"
        except pexpect.EOF:
            kind = "banner" if not prompt else "session"
        child.close(force=True)
    return {"kind": kind, "seconds": time.monotonic() - started,
            "evidence": client_evidence(path), "log": str(path)}


def wrong_password_attempt() -> dict:
    path = RUN / "attempt-wrong.log"
    started = time.monotonic()
    child = pexpect.spawn(ssh_base()[0], ssh_base()[1:] + ["ls"], cwd=str(ROOT),
                          encoding="utf-8", timeout=12)
    prompt = False
    kind = "banner"
    with open(path, "w", encoding="utf-8") as logfile:
        child.logfile = logfile
        try:
            child.expect(r"(?i)password:\s*")
            prompt = True
            child.sendline("wrong-password")
            match = child.expect([r"(?i)permission denied", r"[\$#] ", r"(?i)RavenBSD",
                                  pexpect.EOF, pexpect.TIMEOUT], timeout=12)
            if match == 0:
                kind = "refused"
            elif match == 1 or match == 2:
                kind = "accepted"
            else:
                kind = "session"
        except pexpect.TIMEOUT:
            kind = "banner" if not prompt else "session"
        except pexpect.EOF:
            kind = "banner" if not prompt else "session"
        child.close(force=True)
    return {"kind": kind, "seconds": time.monotonic() - started,
            "evidence": client_evidence(path), "log": str(path)}


def serial_evidence(path: Path) -> list[str]:
    try:
        text = path.read_text(errors="replace").replace("\r", "")
    except FileNotFoundError:
        return []
    wanted = ("svcd: sshd up", "netd: interface eth0", "svcd: getty up",
              "svcd: all services up", "virtio-net: queue1 tx size")
    return [line.strip() for line in text.splitlines()
            if any(needle in line for needle in wanted)]


def classify(attempts: list[dict], interactive: dict, wrong: dict,
             probes: list[dict]) -> tuple[str, str]:
    """Return (class, human line). Class is `pass`, `a`, `b`, `c` or `d`.

    A raw probe that read 0 bytes while the guest accepts the connection is the
    TX-wedge signature (netd TxError::Busy / dropped segments); it is classified
    as (b) even when later connections reach auth and then stall, because the
    transport is provably dropping bytes. A pure session/command failure (d)
    needs every probe to have received the banner.
    """
    ok = all(attempt["kind"] == "ok" for attempt in attempts)
    if ok and interactive["kind"] == "ok" and wrong["kind"] == "refused":
        return "pass", "all 10 command sessions, the PTY case and the wrong-password refusal passed"
    if wrong["kind"] == "accepted":
        return "c", "the wrong password reached a shell"
    if (any(attempt["kind"] == "auth-refused" for attempt in attempts)
            or interactive["kind"] == "auth-refused"):
        return "c", "the password prompt was reached and the password was refused"
    wedged = [probe for probe in probes
              if probe["total"] == 0 or probe["records"] == 0]
    if wedged:
        probe = wedged[0]
        if probe["total"] == 0:
            detail = "the guest sent 0 bytes on a fresh connection"
        else:
            detail = (f"the guest sent only {probe['total']} bytes (banner) and no SSH "
                      f"record on a fresh connection")
        return "b", f"{detail} ({probe['source']}): {probe['summary']}"
    if any(attempt["kind"] == "session" for attempt in attempts) or interactive["kind"] == "session":
        return "d", "authenticated, but the command or PTY session did not complete"
    if any(probe["total"] > 0 for probe in probes):
        return "a", "the guest sent bytes, but the client never received a usable SSH banner"
    return "a", "no usable SSH banner and no raw byte evidence"


CLASS_EXIT = {"a": EXIT_BANNER, "b": EXIT_TXWEDGE, "c": EXIT_AUTH, "d": EXIT_SESSION}


def main() -> int:
    if not IMAGE.is_file():
        log("error", f"missing {IMAGE}; build it with cargo xtask image --profile release")
        return EXIT_SETUP
    for path in (OVMF_CODE, OVMF_VARS):
        if not path.is_file():
            log("error", f"missing firmware {path}")
            return EXIT_SETUP
    RUN.mkdir(parents=True, exist_ok=True)

    child, serial = qemu_boot(1)
    probes = []
    try:
        probes.append(raw_probe())
        log("raw", f"before: {probes[-1]['summary']} [{probes[-1]['source']}]")
        attempts = []
        for index in range(REPEAT):
            result = command_attempt(index + 1)
            attempts.append(result)
            log("command", f"{index + 1}/{REPEAT} {result['kind']} "
                           f"{result['seconds'] * 1000:.0f} ms")
        interactive = interactive_attempt()
        log("pty", f"{interactive['kind']} {interactive['seconds'] * 1000:.0f} ms")
        wrong = wrong_password_attempt()
        log("wrong-password", f"{wrong['kind']}")
        probes.append(raw_probe())
        log("raw", f"after attempts: {probes[-1]['summary']} [{probes[-1]['source']}]")
        probes.append(raw_probe())
        log("raw", f"after pty/wrong: {probes[-1]['summary']} [{probes[-1]['source']}]")
    finally:
        kill(child)

    serial_lines = serial_evidence(serial)
    klass, line = classify(attempts, interactive, wrong, probes)
    table = " ".join(f"{index + 1}:{result['kind']}" for index, result in enumerate(attempts))
    evidence = [f"raw {probe['source']}: {probe['summary']}" for probe in probes]
    for probe in probes:
        for entry in probe["transcript"]:
            evidence.append(f"raw transcript: {entry}")
    evidence.append(f"client (first attempt): {attempts[0]['evidence']}" if attempts
                    else "client: -")
    evidence.append(f"client (pty): {interactive['evidence']}")
    report = RUN / "classification.txt"
    with open(report, "w", encoding="utf-8") as out:
        out.write(f"result: {'PASS' if klass == 'pass' else 'FAIL'}\n")
        out.write(f"class: {klass}\n")
        out.write(f"line: {line}\n")
        out.write(f"attempts: {table}\n")
        out.write(f"interactive: {interactive['kind']}\n")
        out.write(f"wrong-password: {wrong['kind']}\n")
        for probe in probes:
            out.write(f"raw: {probe['source']} {probe['summary']}\n")
        for item in evidence:
            out.write(f"evidence: {item}\n")
        out.write(f"serial: {serial}\n")
        out.write(f"logs: {RUN}\n")
    log("classification", f"class {klass}: {line}")
    log("evidence", f"attempts {table}; pty={interactive['kind']}; wrong={wrong['kind']}")
    for item in evidence:
        log("evidence", item)
    for line in serial_lines[:5]:
        log("serial", line)
    if klass == "pass":
        if attempts:
            ordered = sorted(result["seconds"] for result in attempts)
            p50 = statistics.median(ordered)
            p90 = ordered[min(len(ordered) - 1, int(round(0.9 * (len(ordered) - 1))))]
            log("timing", f"n={len(ordered)} p50={p50 * 1000:.0f} ms p90={p90 * 1000:.0f} ms")
        log("PASS", "host command sessions, PTY and wrong-password refusal")
        return EXIT_PASS
    log("FAIL", f"class {klass} ({line}); see {report}")
    return CLASS_EXIT[klass]


if __name__ == "__main__":
    try:
        sys.exit(main())
    except RuntimeError as error:
        log("FAIL", f"setup: {error}")
        sys.exit(EXIT_SETUP)
