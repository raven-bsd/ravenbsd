#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""K434 per-step SSH gate matrix (S0-S8), per-attempt evidence.

Every row asserts one chain step (docs/ssh-gate-chain.md) with exact bytes or
an exact program result. Each attempt records: attempt number, fresh or reused
connection, the exact bytes read, the elapsed time and the row result. There
are no retries to green and no class labels: a red row names the chain step and
the exact observed value. Rows whose prerequisite is not on this base are
reported BLOCKED with the missing path or commit, never guessed.

One run = S0 boot (throw-away echo initfs) + one SSH boot on the K194 tap.
"""

from __future__ import annotations

import hashlib
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
IMAGE = Path(os.environ.get("RAVEN_SSH_IMAGE", "build/qtest/ssh-login.img"))
ECHO_IMAGE = Path(os.environ.get("RAVEN_SSH_ECHO_IMAGE", "build/fixtures/ssh-echo/echo.img"))
if not IMAGE.is_absolute():
    IMAGE = ROOT / IMAGE
if not ECHO_IMAGE.is_absolute():
    ECHO_IMAGE = ROOT / ECHO_IMAGE
OUT = ROOT / "build" / "ssh" / "matrix"
OVMF_CODE = Path(os.environ.get("RAVEN_OVMF_CODE", "/usr/share/OVMF/OVMF_CODE_4M.fd"))
OVMF_VARS = Path(os.environ.get("RAVEN_OVMF_VARS", "/usr/share/OVMF/OVMF_VARS_4M.fd"))
TAP = os.environ.get("RAVEN_QEMU_TAP", "ravendemo0")
HOST = os.environ.get("RAVEN_SSH_HOST", "10.0.2.15")
USER = os.environ.get("RAVEN_SSH_USER", "raven")
PASSWORD = os.environ.get("RAVEN_SSH_PASSWORD", "raven")
ROOT_USER = os.environ.get("RAVEN_SSH_ROOT_USER", "root")
PEER = OUT / "ssh-peer"
PEER_SOURCE = ROOT / "tests" / "fixtures" / "ssh-peer" / "peer.c"
ASKPASS = ROOT / "tests" / "ssh" / "askpass.sh"

EXPECTED_BANNER = b"SSH-2.0-OpenSSH_10.6\r\n"
BANNER_BOUND_S = 10.0
REQUEST_TIMEOUT_S = 15
ECHO_PORT = 23097
ECHO_PAYLOAD = b"K434-S0-ECHO-0123456789abcdef\n"
MARKER = b"K434-SSH-MARKER-2026-11-22\n"

ROW_ORDER = [
    "S0", "S1", "S1-client-first", "S2", "S3", "S4", "S5", "S5-identity",
    "S5-session", "S6", "S6-length", "S7", "S8", "S8-pty-alloc",
]

rows: list[tuple[str, str, str]] = []
attempts: list[dict[str, str]] = []


def record(row: str, status: str, detail: str) -> None:
    rows.append((row, status, detail))
    print(f"matrix: {row} {status}: {detail}", flush=True)


def attempt(row: str, number: int, connection: str, expected: str, received: str,
            elapsed_ms: float, result: str, detail: str = "") -> None:
    attempts.append({
        "row": row,
        "attempt": str(number),
        "connection": connection,
        "expected": expected,
        "received": received,
        "elapsed_ms": f"{elapsed_ms:.0f}",
        "result": result,
        "detail": detail,
    })
    print(f"matrix: {row} #{number} {connection} {result} {elapsed_ms:.0f}ms "
          f"got={received!r}", flush=True)


def literal(data: bytes) -> str:
    """Return one printable token with the exact bytes (hex when not printable)."""
    if not data:
        return "0 bytes"
    if all(32 <= byte < 127 or byte in (9, 10, 13) for byte in data):
        text = data.decode("ascii")
        return text.encode("unicode_escape").decode("ascii")
    return data.hex()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def build_peer() -> bool:
    if PEER.is_file() and PEER_SOURCE.is_file() and PEER.stat().st_mtime >= PEER_SOURCE.stat().st_mtime:
        return True
    cc = shutil.which("cc") or shutil.which("gcc") or shutil.which("clang")
    if cc is None:
        return False
    PEER.parent.mkdir(parents=True, exist_ok=True)
    result = subprocess.run([cc, "-O2", "-Wall", "-Wextra", "-o", str(PEER), str(PEER_SOURCE)],
                            cwd=ROOT, capture_output=True, text=True, timeout=30, check=False)
    return result.returncode == 0


def qemu_argv(image: Path, serial: Path, vars_path: Path) -> list[str]:
    return [
        "qemu-system-x86_64", "-machine", "q35", "-m", "512M", "-smp", "4",
        "-serial", f"file:{serial}", "-display", "none", "-monitor", "none", "-no-reboot",
        "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"format=raw,file={image}",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-nic", "none",
        "-netdev", f"tap,id=n0,ifname={TAP},script=no,downscript=no",
        "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
    ]


def kill(child: subprocess.Popen) -> None:
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait()


def boot(image: Path, ready: bytes, serial_name: str, vars_name: str,
         ready_timeout: int = 180) -> tuple[subprocess.Popen, Path]:
    OUT.mkdir(parents=True, exist_ok=True)
    serial = OUT / serial_name
    vars_path = OUT / vars_name
    shutil.copyfile(OVMF_VARS, vars_path)
    serial.write_bytes(b"")
    child = subprocess.Popen(qemu_argv(image, serial, vars_path), cwd=ROOT,
                             start_new_session=True,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    deadline = time.monotonic() + ready_timeout
    while time.monotonic() < deadline:
        try:
            if ready in serial.read_bytes():
                return child, serial
        except FileNotFoundError:
            pass
        time.sleep(0.1)
    kill(child)
    raise RuntimeError(f"guest never reached {ready!r}; serial {serial}")


def banner_probe(label: str, send_client: bool) -> dict[str, object]:
    """Fresh TCP connection; read the server identification line."""
    started = time.monotonic()
    connection = None
    connect_error = ""
    deadline = started + 3.0
    while time.monotonic() < deadline:
        try:
            connection = socket.create_connection((HOST, 22), timeout=1)
            break
        except OSError as error:
            connect_error = str(error)
            time.sleep(0.05)
    if connection is None:
        elapsed = (time.monotonic() - started) * 1000
        return {"bytes": b"", "elapsed_ms": elapsed, "error": connect_error,
                "connect_ms": elapsed}
    connected_ms = (time.monotonic() - started) * 1000
    data = bytearray()
    error_text = ""
    try:
        connection.settimeout(0.5)
        if send_client:
            connection.sendall(b"SSH-2.0-K434Probe\r\n")
        deadline = time.monotonic() + BANNER_BOUND_S
        while time.monotonic() < deadline:
            try:
                chunk = connection.recv(4096)
            except socket.timeout:
                continue
            except OSError as error:
                error_text = str(error)
                break
            if not chunk:
                error_text = "eof"
                break
            data.extend(chunk)
            if data.endswith(b"\n"):
                break
    finally:
        connection.close()
    elapsed = (time.monotonic() - started) * 1000
    log = OUT / f"{label}.log"
    log.write_text(
        f"label: {label}\n"
        f"send_client: {send_client}\n"
        f"connect_ms: {connected_ms:.0f}\n"
        f"elapsed_ms: {elapsed:.0f}\n"
        f"bytes_hex: {bytes(data).hex()}\n"
        f"bytes_len: {len(data)}\n"
        f"error: {error_text}\n",
        encoding="utf-8")
    return {"bytes": bytes(data), "elapsed_ms": elapsed, "error": error_text,
            "connect_ms": connected_ms}


def wait_serial(serial: Path, needle: bytes, timeout_s: float) -> bytes:
    """Wait for a serial line; return the whole serial log at that point (or at the cap)."""
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        data = serial.read_bytes()
        if needle in data:
            return data
        time.sleep(0.2)
    return serial.read_bytes()


def row_s0() -> None:
    if not ECHO_IMAGE.is_file():
        record("S0", "BLOCKED",
               f"guest echo fixture image missing ({ECHO_IMAGE}); build it with "
               "sh tests/fixtures/ssh-echo/build.sh")
        attempt("S0", 1, "fresh", literal(ECHO_PAYLOAD), "0 bytes", 0, "BLOCKED",
                "fixture image missing")
        return
    child = None
    try:
        child, serial = boot(ECHO_IMAGE, b"ssh-echo: listening", "serial-echo.log", "vars-echo.fd",
                             ready_timeout=180)
        started = time.monotonic()
        connection = None
        error_text = ""
        deadline = started + 30
        while time.monotonic() < deadline:
            try:
                connection = socket.create_connection((HOST, ECHO_PORT), timeout=2)
                break
            except OSError as error:
                error_text = str(error)
                time.sleep(0.2)
        if connection is None:
            elapsed = (time.monotonic() - started) * 1000
            record("S0", "FAIL", f"could not connect to {HOST}:{ECHO_PORT}: {error_text}")
            attempt("S0", 1, "fresh", literal(ECHO_PAYLOAD), "0 bytes", elapsed, "FAIL",
                    error_text)
            return
        received = bytearray()
        try:
            connection.settimeout(5)
            connection.sendall(ECHO_PAYLOAD)
            while len(received) < len(ECHO_PAYLOAD):
                chunk = connection.recv(len(ECHO_PAYLOAD) - len(received))
                if not chunk:
                    break
                received.extend(chunk)
        finally:
            connection.close()
        elapsed = (time.monotonic() - started) * 1000
        log = OUT / "s0-echo.log"
        log.write_text(
            f"payload_hex: {ECHO_PAYLOAD.hex()}\n"
            f"received_hex: {bytes(received).hex()}\n"
            f"received_len: {len(received)}\n"
            f"elapsed_ms: {elapsed:.0f}\n",
            encoding="utf-8")
        if bytes(received) == ECHO_PAYLOAD:
            record("S0", "PASS", f"guest echoed {len(ECHO_PAYLOAD)} exact bytes in "
                                 f"{elapsed:.0f}ms (C1; serial-echo.log)")
            attempt("S0", 1, "fresh", literal(ECHO_PAYLOAD), literal(bytes(received)),
                    elapsed, "PASS")
        else:
            record("S0", "FAIL", f"echo mismatch: expected {literal(ECHO_PAYLOAD)}, "
                                 f"got {literal(bytes(received))} (C1)")
            attempt("S0", 1, "fresh", literal(ECHO_PAYLOAD), literal(bytes(received)),
                    elapsed, "FAIL")
    finally:
        if child is not None:
            kill(child)


def row_s1() -> None:
    probes = [
        (1, False, "fresh"),
        (2, False, "fresh"),
        (3, True, "fresh"),
    ]
    results = []
    for number, send_client, connection in probes:
        probe = banner_probe(f"s1-probe-{number}", send_client)
        got = probe["bytes"]
        assert isinstance(got, bytes)
        results.append((number, send_client, probe))
        result = "PASS" if got == EXPECTED_BANNER else "FAIL"
        detail = "sent client identification first" if send_client else "no client bytes sent"
        attempt("S1", number, connection, literal(EXPECTED_BANNER), literal(got),
                float(probe["elapsed_ms"]), result, detail)
    first = results[0][2]
    second = results[1][2]
    if first["bytes"] == EXPECTED_BANNER and second["bytes"] == EXPECTED_BANNER:
        record("S1", "PASS",
               f"fresh banner exact on attempt 1 ({first['elapsed_ms']:.0f}ms) and control "
               f"attempt 2 ({second['elapsed_ms']:.0f}ms) (C1/C2; kex.c:1297)")
    else:
        record("S1", "FAIL",
               f"attempt 1 {literal(first['bytes'])}, control attempt 2 "
               f"{literal(second['bytes'])}; expected {literal(EXPECTED_BANNER)} "
               f"(C1/C2; kex.c:1297)")
    third = results[2][2]
    if third["bytes"] == EXPECTED_BANNER:
        record("S1-client-first", "PASS",
               f"a fresh connection that sends its identification first received the exact "
               f"banner in {third['elapsed_ms']:.0f}ms (characterisation only)")
    else:
        record("S1-client-first", "FAIL",
               f"client-first probe got {literal(third['bytes'])} "
               f"(characterisation only)")


def row_s2() -> None:
    log = OUT / "s2-peer.log"
    started = time.monotonic()
    subprocess.run([str(PEER), "--host", HOST, "--port", "22", "--mode", "client",
                    "--send-banner", "--send-kexinit", "--idle-ms", "3000",
                    "--log", str(log)],
                   cwd=ROOT, capture_output=True, text=True, timeout=25, check=False)
    elapsed = (time.monotonic() - started) * 1000
    text = log.read_text(errors="replace")
    record_line = next((line for line in text.splitlines() if "record #1" in line), "")
    length = re.search(r"packet_length=(\d+)", record_line)
    padding = re.search(r"padding=(\d+)", record_line)
    total = re.search(r"total=(\d+)", record_line)
    payload = re.search(r"payload=(\d+)", record_line)
    if "type=20" in record_line and length and padding and total and payload:
        packet_length = int(length.group(1))
        padding_length = int(padding.group(1))
        total_length = int(total.group(1))
        payload_length = int(payload.group(1))
        framed = total_length == packet_length + 4
        consistent = packet_length == padding_length + payload_length + 1
        if framed and consistent and padding_length >= 4 and payload_length > 1:
            record("S2", "PASS",
                   f"KEXINIT exact: total={total_length} packet_length={packet_length} "
                   f"padding={padding_length} payload={payload_length} type=20 (RFC 4251 6; "
                   f"C2/C3)")
            attempt("S2", 1, "fresh", "type=20 total=4+packet_length padding>=4",
                    record_line.strip(), elapsed, "PASS")
            return
    record("S2", "FAIL", f"no consistent KEXINIT record: `{record_line.strip()}` ({log})")
    attempt("S2", 1, "fresh", "type=20 total=4+packet_length padding>=4",
            record_line.strip() or "0 records", elapsed, "FAIL")


def ssh_run(command: list[str], log_name: str, user: str = USER,
            password: str = PASSWORD, extra: list[str] | None = None,
            timeout: int = REQUEST_TIMEOUT_S) -> dict[str, object]:
    argv = [
        "ssh",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "ConnectTimeout=8",
        "-o", "NumberOfPasswordPrompts=1",
    ]
    if extra:
        argv.extend(extra)
    argv.append(f"{user}@{HOST}")
    argv.extend(command)
    env = os.environ.copy()
    env["SSH_ASKPASS"] = str(ASKPASS)
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["DISPLAY"] = ":0"
    env["K434_SSH_PASSWORD"] = password
    started = time.monotonic()
    timed_out = False
    try:
        completed = subprocess.run(argv, cwd=ROOT, stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   env=env, timeout=timeout, check=False)
        returncode = completed.returncode
        stdout = completed.stdout
        stderr = completed.stderr
    except subprocess.TimeoutExpired as error:
        timed_out = True
        returncode = 124
        stdout = error.stdout or b""
        stderr = error.stderr or b""
    elapsed = (time.monotonic() - started) * 1000
    log = OUT / log_name
    log.write_text(
        f"$ {' '.join(argv)}\n"
        f"exit: {returncode}\n"
        f"timed_out: {timed_out}\n"
        f"elapsed_ms: {elapsed:.0f}\n"
        f"stdout_hex: {stdout.hex()}\n"
        f"stdout_text: {stdout.decode('utf-8', 'replace')}\n"
        f"stderr_text: {stderr.decode('utf-8', 'replace')}\n",
        encoding="utf-8")
    return {"returncode": returncode, "stdout": stdout, "stderr": stderr,
            "elapsed_ms": elapsed, "timed_out": timed_out, "argv": argv}


def row_s3_start() -> dict[str, object]:
    good = ssh_run(["true"], "s3-raven.log", extra=["-v"])
    good_text = (good["stderr"] + good["stdout"]).decode("utf-8", "replace")
    auth_line = next((line for line in good_text.splitlines()
                      if line.startswith("Authenticated to ") and 'using "password"' in line), "")
    client_auth = bool(auth_line)
    attempt("S3-auth-raven", 1, "fresh", 'Authenticated to ... using "password"',
            auth_line or f"exit {good['returncode']}",
            float(good["elapsed_ms"]), "PASS" if client_auth else "FAIL")
    bad = ssh_run(["true"], "s3-wrong.log", password="wrong-password",
                  extra=["-v"])
    bad_text = (bad["stderr"] + bad["stdout"]).decode("utf-8", "replace")
    refused = bad["returncode"] == 255 and "Permission denied" in bad_text
    attempt("S3-wrong", 1, "fresh", "exit 255 + Permission denied",
            f"exit {bad['returncode']} + {'Permission denied' if 'Permission denied' in bad_text else 'no refusal line'}",
            float(bad["elapsed_ms"]), "PASS" if refused else "FAIL")
    return {"client_auth": client_auth, "auth_line": auth_line,
            "refused": refused, "bad_exit": bad["returncode"]}


def row_s3_finish(serial: Path, state: dict[str, object]) -> None:
    started = time.monotonic()
    data = wait_serial(serial, b"Accepted password for raven", 120)
    wait_ms = (time.monotonic() - started) * 1000
    serial_text = data.decode("utf-8", "replace")
    accepted = [line.replace("\r", "").strip() for line in serial_text.splitlines()
                if "Accepted password for raven " in line]
    attempt("S3-server-line", 1, "fresh",
            "Accepted password for raven from 10.0.2.2 port N ssh2",
            accepted[0] if accepted else "not in serial.log", wait_ms,
            "PASS" if accepted else "FAIL")
    client_auth = bool(state["client_auth"])
    refused = bool(state["refused"])
    if client_auth and accepted and refused:
        record("S3", "PASS",
               f"client `{state['auth_line']}`; serial server line `{accepted[0]}`; wrong password "
               f"exit 255 + Permission denied (auth-passwd; C5/C9)")
    else:
        record("S3", "FAIL",
               f"client auth {'yes' if client_auth else 'no'}; server line "
               f"{'present' if accepted else 'missing'} ({accepted[0] if accepted else 'none'}); "
               f"wrong exit {state['bad_exit']} (auth-passwd; C5/C9)")


def row_s4() -> None:
    result = ssh_run(["uname", "-a"], "s4-vv.log", extra=["-vv"])
    text = (result["stderr"] + result["stdout"]).decode("utf-8", "replace")
    auth = "Authenticated to " in text and 'using "password"' in text
    hostkeys_line = next((line for line in text.splitlines()
                          if "client_input_global_request: rtype hostkeys-00@openssh.com" in line), "")
    hostkeys = bool(hostkeys_line)
    no_efault = "EFAULT" not in text
    output = result["stdout"].decode("utf-8", "replace")
    uname_ok = output.startswith("RavenBSD ")
    observed = f"hostkeys={hostkeys} uname_output={uname_ok} exit={result['returncode']} efault={not no_efault}"
    attempt("S4", 1, "fresh", "hostkeys-00@openssh.com + no EFAULT",
            observed, float(result["elapsed_ms"]),
            "PASS" if auth and hostkeys and no_efault else "FAIL")
    if auth and hostkeys and no_efault:
        record("S4", "PASS",
               f"post-auth read returned `{hostkeys_line.strip()}`; no EFAULT (POSIX read; "
               f"C2/C7). The command output did not arrive (exit {result['returncode']}); that is "
               f"the post-exec path, not this read")
    elif not auth:
        record("S4", "BLOCKED",
               f"auth not reached (exit {result['returncode']}); post-auth read not exercised "
               f"(C2/C7)")
    else:
        record("S4", "FAIL",
               f"{observed} (POSIX read; C2/C7)")


def row_s5_s6() -> None:
    result = ssh_run(["true"], "s5-vvv.log", extra=["-vvv"])
    text = (result["stderr"] + result["stdout"]).decode("utf-8", "replace")
    auth = "Authenticated to " in text and 'using "password"' in text
    open_confirm = "debug2: channel_input_open_confirmation: channel 0" in text
    confirm_line = next((line for line in text.splitlines()
                         if "channel 0: open confirm rwindow" in line), "")
    type91_line = next((line for line in text.splitlines()
                        if "receive packet: type 91" in line), "")
    broken = next((line for line in text.splitlines()
                   if "read failed" in line), "")
    attempt("S5", 1, "fresh", "channel_input_open_confirmation + open confirm",
            confirm_line or f"exit {result['returncode']}",
            float(result["elapsed_ms"]), "PASS" if auth and open_confirm and confirm_line else "FAIL")
    attempt("S6", 1, "reuse", "receive packet type 91 + channel 0",
            type91_line or f"exit {result['returncode']}",
            float(result["elapsed_ms"]), "PASS" if auth and type91_line and open_confirm else "FAIL")
    attempt("S5-identity", 1, "fresh", "host peer exact N bytes on the same file/sid",
            "not observable", 0, "BLOCKED",
            "K431 tests/fixtures/socket-write-identity is not on origin/master")
    if auth and open_confirm and confirm_line:
        record("S5", "PASS",
               f"client received `{confirm_line.strip()}` on the authenticated connection "
               f"(sosend; C5/C6/C8); file/sid identity is S5-identity BLOCKED (K431)")
    else:
        record("S5", "FAIL",
               f"no channel-open confirmation (exit {result['returncode']}); (sosend; C5/C6/C8)")
    if auth and type91_line and open_confirm:
        record("S6", "PASS",
               f"`{type91_line.strip()}` and `{confirm_line.strip()}`: message type 91 on the "
               f"client's channel 0 (RFC 4254 5.1; C5); 44-byte plaintext length is not in the "
               f"client log")
    else:
        record("S6", "FAIL",
               f"type 91 not observed (exit {result['returncode']}); (RFC 4254 5.1; C5)")
    attempt("S6-length", 1, "reuse", "44-byte plaintext packet",
            "not observable", 0, "BLOCKED",
            "client exposes type and recipient channel, not the packet length")
    if broken:
        record("S5-session", "FAIL",
               f"after the confirmation the client reports `{broken.strip()}`; command output "
               f"never arrived (exit {result['returncode']}); this is the post-exec/session path, "
               f"not the confirmation")


def row_s7() -> None:
    marker_run = ssh_run(["cat", "/tmp/ssh-marker"], "s7-marker.log")
    marker_ok = marker_run["returncode"] == 0 and marker_run["stdout"] == MARKER
    attempt("S7-marker", 1, "fresh", literal(MARKER), literal(marker_run["stdout"]),
            float(marker_run["elapsed_ms"]), "PASS" if marker_ok else "FAIL",
            f"exit {marker_run['returncode']}")
    passed = 0
    for number in range(1, 11):
        session = ssh_run(["true"], f"s7-run-{number}.log")
        ok = session["returncode"] == 0
        passed += 1 if ok else 0
        attempt("S7-10/10", number, "fresh", "exit 0", f"exit {session['returncode']}",
                float(session["elapsed_ms"]), "PASS" if ok else "FAIL")
    if marker_ok and passed == 10:
        record("S7", "PASS",
               f"`cat /tmp/ssh-marker` returned exactly {literal(MARKER)}; 10/10 command "
               f"sessions exit 0 (gate; C1-C10)")
    else:
        record("S7", "FAIL",
               f"marker {'exact' if marker_ok else 'mismatch/error'} "
               f"({literal(marker_run['stdout'])}); {passed}/10 command sessions (gate; C1-C10)")


def row_s8() -> None:
    result = ssh_run(["-tt", "tty"], "s8-tt.log", extra=["-tt"])
    output = result["stdout"].decode("utf-8", "replace")
    error = result["stderr"].decode("utf-8", "replace")
    match = re.search(r"/dev/pts/[0-9]+", output)
    if match:
        record("S8", "PASS", f"`ssh -tt tty` returned {match.group(0)} (openpty; C9)")
        attempt("S8", 1, "fresh", "/dev/pts/N", match.group(0), float(result["elapsed_ms"]),
                "PASS")
    elif "PTY allocation request failed" in error or "PTY allocation request failed" in output:
        record("S8", "FAIL",
               "`PTY allocation request failed on channel 0` (pts_alloc/openpty; C9; K430 not "
               "merged)")
        attempt("S8", 1, "fresh", "/dev/pts/N", "PTY allocation request failed",
                float(result["elapsed_ms"]), "FAIL")
    else:
        record("S8", "FAIL",
               f"`ssh -tt tty` exit {result['returncode']}, no /dev/pts/N (openpty; C9)")
        attempt("S8", 1, "fresh", "/dev/pts/N", literal(result["stdout"][:80]),
                float(result["elapsed_ms"]), "FAIL")
    attempt("S8-pty-alloc", 1, "fresh", "tests/libc/c/libc-pty-alloc.c case passes",
            "not on origin/master", 0, "BLOCKED",
            "K430 tests/libc/c/libc-pty-alloc.c and tests/qtest/cases/pty-alloc.toml are not merged")


def write_tables() -> None:
    attempts_path = OUT / "attempts.tsv"
    columns = ["row", "attempt", "connection", "expected", "received", "elapsed_ms",
               "result", "detail"]
    order = {name: index for index, name in enumerate(ROW_ORDER)}
    ordered_attempts = sorted(attempts, key=lambda item: (
        order.get(item["row"], len(ROW_ORDER)), item["row"], int(item["attempt"])))
    with open(attempts_path, "w", encoding="utf-8") as handle:
        handle.write("\t".join(columns) + "\n")
        for item in ordered_attempts:
            handle.write("\t".join(item[column].replace("\t", " ") for column in columns) + "\n")
    rows_path = OUT / "matrix.tsv"
    with open(rows_path, "w", encoding="utf-8") as handle:
        handle.write("row\tstatus\tdetail\n")
        for row, status, detail in sorted(rows, key=lambda item: (
                order.get(item[0], len(ROW_ORDER)), item[0])):
            handle.write(f"{row}\t{status}\t{detail}\n")
    print(f"matrix: attempts {attempts_path}")
    print(f"matrix: table {rows_path}")


def main() -> int:
    if not IMAGE.is_file():
        print(f"matrix: missing image {IMAGE}", file=sys.stderr)
        return 2
    if not ASKPASS.is_file():
        print(f"matrix: missing askpass helper {ASKPASS}", file=sys.stderr)
        return 2
    OUT.mkdir(parents=True, exist_ok=True)
    for stale in OUT.glob("serial*.log"):
        stale.unlink()
    if not build_peer():
        print("matrix: cannot build the peer tracer", file=sys.stderr)
        return 2

    row_s0()

    child = None
    try:
        child, serial = boot(IMAGE, b"svcd: sshd up", "serial.log", "vars.fd",
                             ready_timeout=180)
        row_s1()
        row_s2()
        s3_state = row_s3_start()
        row_s4()
        row_s5_s6()
        row_s3_finish(serial, s3_state)
        row_s7()
        row_s8()
    except Exception as error:  # noqa: BLE001 - a boot failure is a BLOCKED row, not a crash
        record("S1", "BLOCKED", f"SSH boot/run failed: {error}")
        for row in ("S2", "S3", "S4", "S5", "S6", "S7", "S8"):
            record(row, "BLOCKED", f"SSH boot/run failed before this row: {error}")
    finally:
        if child is not None:
            kill(child)
    write_tables()
    serial = OUT / "serial.log"
    if serial.is_file():
        print(f"matrix: serial sha256 {sha256(serial)}")
    failed = [row for row, status, _ in rows if status in ("FAIL", "BLOCKED", "PARTIAL")]
    print(f"matrix: {'RED' if failed else 'GREEN'} ({len(failed)} red rows)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
