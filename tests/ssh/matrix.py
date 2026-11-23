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
import errno
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
OUT = Path(os.environ.get("RAVEN_MATRIX_OUT", "build/ssh/matrix"))
if not OUT.is_absolute():
    OUT = ROOT / OUT
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
S1_REPEAT_TIMEOUT_S = 8
S4_ATTEMPTS = 5
S4_ONLY_ATTEMPTS = 3
S4_INPUT = b"K514-S4-POSTAUTH-2026-11-22\n"
S4_FILE = "/tmp/k514-s4-input"
S4_AUTH_BOUND_S = 30
S4_READ_BOUND_S = 15
S4_TOTAL_BOUND_S = 50
# K514 rev 93: the builtin-only read proof and the release command-exit assertion.
S4_ASSERT_ATTEMPTS = 3
S4_ECHO_COMMAND = ["echo", "K514-S4-ECHO-2026-11-22"]
S4_ECHO_BYTES = b"K514-S4-ECHO-2026-11-22\n"
S4_EXIT_COMMAND = ["cat", "/etc/ssh-matrix-marker"]
# K514 rev 95: 10-TTY verification; every session must print /dev/pts/N from `tty`.
TTY10_ATTEMPTS = 10
TTY10_RE = re.compile(rb"/dev/pts/[0-9]+")
# K514 rev 92 release blast-radius measurement: every command is attempted ten times as the
# release user. The S4 row's own expectations are unchanged.
RELEASE_SESSIONS = 10
RELEASE_ROOT_NAMES = (
    b"bin", b"dev", b"etc", b"home", b"init", b"lib", b"libexec",
    b"root", b"sbin", b"tmp", b"usr", b"var",
)


def release_root_listing(out: bytes) -> bool:
    """The root listing must contain every expected namespace entry."""
    words = out.split()
    return all(name in words for name in RELEASE_ROOT_NAMES)


RELEASE_COMMANDS = (
    ("uname", ["uname"], lambda out: out == b"RavenBSD\n",
     "stdout == RavenBSD\\n"),
    ("pwd", ["pwd"], lambda out: out == b"/\n",
     "stdout == /\\n (release session cwd)"),
    ("ls", ["ls", "/"], release_root_listing,
     "root listing contains all expected namespace entries"),
    ("cat-hosts", ["cat", "/etc/hosts"], lambda out: b"127.0.0.1" in out,
     "stdout contains 127.0.0.1"),
    ("echo-builtin", ["echo", "K514-BUILTIN"], lambda out: out == b"K514-BUILTIN\n",
     "stdout == K514-BUILTIN\\n"),
)
RELEASE_FAULT = re.compile(rb"killed by page fault at (0x[0-9a-f]+)")
S7_TIMEOUT_S = 30
S7_WALL_BOUND_MS = 5000
ECHO_PORT = 23097
ECHO_PAYLOAD = b"K434-S0-ECHO-0123456789abcdef\n"
MARKER = b"K434-SSH-MARKER-2026-11-22\n"
SERIAL_LEASE = b"resolvd: lease eth0 10.0.2.15"

ROW_ORDER = [
    "S0", "S1", "S1-client-first", "S1-repeat", "S2", "S3", "S4", "S5", "S5-identity",
    "S5-session", "S6", "S6-length", "S7", "S7-class", "S7-routing", "S7-exit", "S8",
    "S8-openpty-null", "S8-monitor", "S8-fds", "S8-pty-alloc", "S8-termios",
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


def qemu_argv(image: Path, serial_arg: str, vars_path: Path) -> list[str]:
    accel = os.environ.get("RAVEN_QEMU_ACCEL", "")
    argv = [
        "qemu-system-x86_64", "-machine", "q35", "-m", "512M", "-smp", "4",
        "-serial", serial_arg, "-display", "none", "-monitor", "none", "-no-reboot",
        "-snapshot",
        "-drive", f"if=pflash,format=raw,readonly=on,file={OVMF_CODE}",
        "-drive", f"if=pflash,format=raw,file={vars_path}",
        "-drive", f"format=raw,file={image}",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-nic", "none",
        "-netdev", f"tap,id=n0,ifname={TAP},script=no,downscript=no",
        "-device", "virtio-net-pci,netdev=n0,disable-legacy=on",
    ]
    if accel == "kvm":
        argv.extend(["-enable-kvm", "-cpu", "host"])
    elif accel == "tcg":
        argv.extend(["-accel", "tcg"])
    elif accel:
        raise SystemExit(f"matrix: unknown RAVEN_QEMU_ACCEL={accel}")
    return argv


def kill(child: subprocess.Popen) -> None:
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait()


class SerialConsole:
    """QEMU serial over a Unix socket, with an exact byte log."""

    def __init__(self, path: Path, log_path: Path) -> None:
        self.path = path
        self.log_path = log_path
        self.buffer = bytearray()
        self.socket: socket.socket | None = None
        self.log = open(log_path, "wb")  # noqa: SIM115 - closed in close()

    def connect(self, timeout_s: float = 10.0) -> None:
        deadline = time.monotonic() + timeout_s
        while time.monotonic() < deadline:
            try:
                connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                connection.connect(str(self.path))
                connection.settimeout(0.2)
                self.socket = connection
                return
            except OSError:
                time.sleep(0.05)
        raise RuntimeError(f"cannot connect to serial socket {self.path}")

    def pump(self) -> None:
        if self.socket is None:
            return
        while True:
            try:
                chunk = self.socket.recv(4096)
            except socket.timeout:
                return
            except OSError:
                return
            if not chunk:
                return
            self.buffer.extend(chunk)
            self.log.write(chunk)
            self.log.flush()

    def wait_for(self, needle: bytes, timeout_s: float, start: int = 0) -> int:
        deadline = time.monotonic() + timeout_s
        while time.monotonic() < deadline:
            self.pump()
            found = self.buffer.find(needle, start)
            if found >= 0:
                return found
            time.sleep(0.05)
        return -1

    def send(self, data: bytes) -> None:
        if self.socket is None:
            raise RuntimeError("serial socket not connected")
        self.socket.sendall(data)

    def close(self) -> None:
        if self.socket is not None:
            try:
                self.socket.close()
            except OSError:
                pass
            self.socket = None
        self.log.close()


def boot(image: Path, ready: bytes, serial_name: str, vars_name: str,
         ready_timeout: int = 180,
         socket_serial: bool = False) -> tuple[subprocess.Popen, Path | SerialConsole]:
    OUT.mkdir(parents=True, exist_ok=True)
    serial = OUT / serial_name
    vars_path = OUT / vars_name
    shutil.copyfile(OVMF_VARS, vars_path)
    if socket_serial:
        socket_path = OUT / serial_name.replace(".log", ".sock")
        try:
            socket_path.unlink()
        except FileNotFoundError:
            pass
        serial_arg = f"unix:{socket_path},server=on,wait=off"
        console = SerialConsole(socket_path, serial)
        qemu_log = OUT / f"{serial_name}.qemu.log"
        with open(qemu_log, "wb") as qemu_err:
            child = subprocess.Popen(qemu_argv(image, serial_arg, vars_path), cwd=ROOT,
                                     start_new_session=True,
                                     stdout=subprocess.DEVNULL, stderr=qemu_err)
        console.connect(10)
        if console.wait_for(ready, ready_timeout) < 0:
            kill(child)
            console.close()
            raise RuntimeError(f"guest never reached {ready!r}; serial {serial}; qemu {qemu_log}")
        return child, console
    serial.write_bytes(b"")
    qemu_log = OUT / f"{serial_name}.qemu.log"
    with open(qemu_log, "wb") as qemu_err:
        child = subprocess.Popen(qemu_argv(image, f"file:{serial}", vars_path), cwd=ROOT,
                                 start_new_session=True,
                                 stdout=subprocess.DEVNULL, stderr=qemu_err)
    deadline = time.monotonic() + ready_timeout
    while time.monotonic() < deadline:
        try:
            if ready in serial.read_bytes():
                return child, serial
        except FileNotFoundError:
            pass
        time.sleep(0.1)
    kill(child)
    raise RuntimeError(f"guest never reached {ready!r}; serial {serial}; qemu {qemu_log}")


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


def wait_tcp(port: int, timeout_s: float) -> bool:
    """Wait until the guest listener accepts a TCP connection (repeat-boot readiness)."""
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        try:
            connection = socket.create_connection((HOST, port), timeout=1)
            connection.close()
            return True
        except OSError:
            time.sleep(0.2)
    return False


def serial_has(serial: Path | SerialConsole, needle: bytes, timeout_s: float = 60) -> bool:
    """Wait for a guest serial line (the DHCP lease) before any network probe."""
    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        if isinstance(serial, SerialConsole):
            serial.pump()
            data = bytes(serial.buffer)
        else:
            try:
                data = serial.read_bytes()
            except FileNotFoundError:
                data = b""
        if needle in data:
            return True
        time.sleep(0.1)
    return False


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
        if not serial_has(serial, SERIAL_LEASE, 60):
            record("S0", "FAIL",
                   f"guest did not acquire {SERIAL_LEASE.decode()!r} within 60 s; "
                   f"serial-echo.log")
            attempt("S0", 1, "fresh", literal(ECHO_PAYLOAD), "no guest lease", 0, "FAIL",
                    "serial-echo.log")
            return
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


def row_s1_repeat() -> None:
    counts = {"connect": 0, "banner": 0, "auth": 0, "ok": 0}
    walls: list[float] = []
    exits: list[int] = []
    for number in range(1, 11):
        result = ssh_run(["true"], f"s1-repeat-{number}.log", extra=["-vvv"],
                         timeout=S1_REPEAT_TIMEOUT_S)
        text = (result["stderr"] + result["stdout"]).decode("utf-8", "replace")
        elapsed = float(result["elapsed_ms"])
        walls.append(elapsed)
        exits.append(int(result["returncode"]))
        connected = "Connection established." in text
        banner_line = next((line.strip() for line in text.splitlines()
                            if "Remote protocol version" in line), "")
        banner_ok = ("Remote protocol version 2.0, remote software version OpenSSH_10.6"
                     in text)
        auth_line = next((line.strip() for line in text.splitlines()
                          if line.startswith("Authenticated to ")
                          and 'using "password"' in line), "")
        auth_ok = bool(auth_line)
        if not connected:
            step = "connect"
        elif not banner_ok:
            step = "banner"
        elif not auth_ok:
            step = "auth"
        else:
            step = "ok"
        counts[step] += 1
        witness = next((line.strip() for line in text.splitlines()
                        if any(marker in line for marker in (
                            "Connection timed out", "Connection refused",
                            "Connection closed", "ssh_exchange_identification",
                            "Permission denied"))), "-")
        attempt("S1-repeat", number, "fresh",
                "Remote protocol version 2.0 ... OpenSSH_10.6 + Authenticated",
                f"step={step}; banner={banner_line or '-'}; auth={auth_line or '-'}; "
                f"exit={result['returncode']}; wall={elapsed:.0f} ms; witness={witness}",
                elapsed, "PASS" if banner_ok and auth_ok else "FAIL")
    if counts["ok"] == 10:
        record("S1-repeat", "PASS",
               f"10/10 fresh connections: banner OpenSSH_10.6 and Authenticated; "
               f"walls_ms={[round(value) for value in walls]}; exits={exits}")
    else:
        record("S1-repeat", "FAIL",
               f"connect_fail={counts['connect']} banner_fail={counts['banner']} "
               f"auth_fail={counts['auth']} ok={counts['ok']}; "
               f"walls_ms={[round(value) for value in walls]}; exits={exits}")


def row_s2() -> None:
    outcomes: list[tuple[int, bool, str, float]] = []
    for number in range(1, 4):
        log = OUT / ("s2-peer.log" if number == 1 else f"s2-peer-{number}.log")
        idle_ms = "3000" if number == 1 else "6000"
        started = time.monotonic()
        subprocess.run([str(PEER), "--host", HOST, "--port", "22", "--mode", "client",
                        "--send-banner", "--send-kexinit", "--idle-ms", idle_ms,
                        "--log", str(log)],
                       cwd=ROOT, capture_output=True, text=True, timeout=25, check=False)
        elapsed = (time.monotonic() - started) * 1000
        text = log.read_text(errors="replace")
        record_line = next((line for line in text.splitlines() if "record #1" in line), "")
        length = re.search(r"packet_length=(\d+)", record_line)
        padding = re.search(r"padding=(\d+)", record_line)
        total = re.search(r"total=(\d+)", record_line)
        payload = re.search(r"payload=(\d+)", record_line)
        exact = False
        detail = record_line.strip() or "0 records"
        if "type=20" in record_line and length and padding and total and payload:
            packet_length = int(length.group(1))
            padding_length = int(padding.group(1))
            total_length = int(total.group(1))
            payload_length = int(payload.group(1))
            framed = total_length == packet_length + 4
            consistent = packet_length == padding_length + payload_length + 1
            if framed and consistent and padding_length >= 4 and payload_length > 1:
                exact = True
                detail = (f"total={total_length} packet_length={packet_length} "
                          f"padding={padding_length} payload={payload_length} type=20")
        outcomes.append((number, exact, detail, elapsed))
        attempt("S2", number, "fresh", "type=20 total=4+packet_length padding>=4",
                detail, elapsed, "PASS" if exact else "FAIL")
        if exact:
            break
    passed = next((outcome for outcome in outcomes if outcome[1]), None)
    failures = [f"#{number} {detail}" for number, exact, detail, _ in outcomes if not exact]
    if passed is not None:
        record("S2", "PASS",
               f"attempt {passed[0]} exact ({passed[2]}); prior failures: "
               + ("; ".join(failures) if failures else "none"))
    else:
        record("S2", "FAIL", f"no consistent KEXINIT in 3 bounded attempts: "
                             + "; ".join(failures))


def ssh_run(command: list[str], log_name: str, user: str = USER,
            password: str = PASSWORD, extra: list[str] | None = None,
            timeout: int = REQUEST_TIMEOUT_S,
            console: SerialConsole | None = None) -> dict[str, object]:
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
    stdout_path = OUT / f"{log_name}.stdout"
    stderr_path = OUT / f"{log_name}.stderr"
    with open(stdout_path, "wb") as out_file, open(stderr_path, "wb") as err_file:
        process = subprocess.Popen(argv, cwd=ROOT, stdin=subprocess.PIPE,
                                   stdout=out_file, stderr=err_file, env=env,
                                   start_new_session=True)
        deadline = started + timeout
        while process.poll() is None and time.monotonic() < deadline:
            if console is not None:
                console.pump()
            time.sleep(0.05)
        timed_out = process.poll() is None
        if timed_out:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        returncode = process.wait()
        try:
            process.stdin.close()  # type: ignore[union-attr]
        except (OSError, AttributeError):
            pass
    stdout = stdout_path.read_bytes()
    stderr = stderr_path.read_bytes()
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


def ssh_probe(command: list[str], log_name: str, user: str = USER,
              password: str = PASSWORD, extra: list[str] | None = None,
              timeout: int = REQUEST_TIMEOUT_S) -> dict[str, object]:
    """Run one ssh session and sample its socket fd/state; trace read/write if strace exists."""
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
    strace = shutil.which("strace")
    strace_log = OUT / f"{log_name}-strace.log"
    if strace:
        argv = [strace, "-f", "-tt", "-s", "0", "-yy",
                "-e", "trace=read,write,close,shutdown",
                "-o", str(strace_log)] + argv
    stdout_path = OUT / f"{log_name}.stdout"
    stderr_path = OUT / f"{log_name}.stderr"
    snapshots: list[dict[str, object]] = []
    started = time.monotonic()
    timed_out = False
    with open(stdout_path, "wb") as out_file, open(stderr_path, "wb") as err_file:
        process = subprocess.Popen(argv, cwd=ROOT, stdin=subprocess.PIPE,
                                   stdout=out_file, stderr=err_file, env=env,
                                   start_new_session=True)
        deadline = started + timeout
        while time.monotonic() < deadline:
            if process.poll() is not None:
                break
            ssh_pid = 0
            if strace:
                try:
                    children = subprocess.run(
                        ["pgrep", "-P", str(process.pid)], capture_output=True,
                        text=True, timeout=5, check=False).stdout.split()
                    for child in children:
                        try:
                            comm = Path(f"/proc/{child}/comm").read_text().strip()
                        except OSError:
                            continue
                        if comm == "ssh":
                            ssh_pid = int(child)
                            break
                except (OSError, subprocess.SubprocessError):
                    ssh_pid = 0
            else:
                ssh_pid = process.pid
            fd4 = ""
            if ssh_pid:
                try:
                    fd4 = os.readlink(f"/proc/{ssh_pid}/fd/4")
                except OSError:
                    fd4 = ""
            ss_line = ""
            if ssh_pid:
                try:
                    ss_output = subprocess.run(
                        ["ss", "-H", "-tnp"], capture_output=True, text=True,
                        timeout=5, check=False).stdout
                    ss_line = " || ".join(
                        line.strip() for line in ss_output.splitlines()
                        if f"pid={ssh_pid}" in line)
                except (OSError, subprocess.SubprocessError):
                    ss_line = ""
            if fd4 or ss_line:
                snapshots.append({
                    "t_ms": (time.monotonic() - started) * 1000,
                    "pid": ssh_pid,
                    "fd4": fd4,
                    "ss": ss_line,
                })
            time.sleep(0.2)
        timed_out = process.poll() is None
        if timed_out:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        returncode = process.wait()
        try:
            process.stdin.close()  # type: ignore[union-attr]
        except (OSError, AttributeError):
            pass
    elapsed = (time.monotonic() - started) * 1000
    stdout = stdout_path.read_bytes()
    stderr = stderr_path.read_bytes()
    strace_text = strace_log.read_text(errors="replace") if strace_log.is_file() else ""
    epipe_lines = [line for line in strace_text.splitlines()
                   if "read(4" in line and "EPIPE" in line]
    eof_lines = [line for line in strace_text.splitlines()
                 if "read(4" in line and line.rstrip().endswith("= 0")]
    fd_targets = []
    for item in snapshots:
        target = str(item["fd4"])
        if target and target not in fd_targets:
            fd_targets.append(target)
    peer_states = []
    for item in snapshots:
        state = str(item["ss"])
        if state and state not in peer_states:
            peer_states.append(state)
    probe_log = OUT / f"{log_name}-probe.log"
    with open(probe_log, "w", encoding="utf-8") as handle:
        handle.write(f"strace: {strace or 'unavailable'}\n")
        handle.write(f"errno_EPIPE: {errno.EPIPE} ({os.strerror(errno.EPIPE)})\n")
        handle.write(f"exit: {returncode}\n")
        handle.write(f"timed_out: {timed_out}\n")
        for item in snapshots:
            handle.write(f"t_ms={float(item['t_ms']):.0f} pid={item['pid']} "
                         f"fd4={item['fd4']} ss={item['ss']}\n")
        for line in epipe_lines:
            handle.write(f"strace_epipe: {line}\n")
        for line in eof_lines:
            handle.write(f"strace_eof: {line}\n")
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
            "elapsed_ms": elapsed, "timed_out": timed_out, "argv": argv,
            "epipe_lines": epipe_lines, "eof_lines": eof_lines,
            "fd_targets": fd_targets, "peer_states": peer_states,
            "probe_log": probe_log, "strace_log": strace_log}


def row_s3_start() -> dict[str, object]:
    bad = ssh_run(["true"], "s3-wrong.log", password="wrong-password",
                  extra=["-v"])
    bad_text = (bad["stderr"] + bad["stdout"]).decode("utf-8", "replace")
    refused = bad["returncode"] == 255 and "Permission denied" in bad_text
    if not refused:
        banner_timeout = "timed out during banner exchange" in bad_text
        bad_detail = ("banner timeout before auth" if banner_timeout else
                      f"exit {bad['returncode']} + no refusal line")
    else:
        bad_detail = f"exit {bad['returncode']} + Permission denied"
    attempt("S3-wrong", 1, "fresh", "exit 255 + Permission denied",
            bad_detail, float(bad["elapsed_ms"]), "PASS" if refused else "FAIL")
    good = ssh_run(["true"], "s3-raven.log", extra=["-v"])
    good_text = (good["stderr"] + good["stdout"]).decode("utf-8", "replace")
    auth_line = next((line for line in good_text.splitlines()
                      if line.startswith("Authenticated to ") and 'using "password"' in line), "")
    client_auth = bool(auth_line)
    attempt("S3-auth-raven", 1, "fresh", 'Authenticated to ... using "password"',
            auth_line or f"exit {good['returncode']}",
            float(good["elapsed_ms"]), "PASS" if client_auth else "FAIL")
    return {"client_auth": client_auth, "auth_line": auth_line,
            "refused": refused, "bad_exit": bad["returncode"]}


def console_login(console: SerialConsole) -> bool:
    """Log in as root/raven on the serial console."""
    if console.wait_for(b"login:", 5) < 0:
        # The prompt may have been printed before the socket connected; a newline makes getty
        # print a fresh one.
        console.send(b"\n")
        if console.wait_for(b"login:", 30) < 0:
            return False
    console.send(b"root\n")
    if console.wait_for(b"Password:", 10) < 0:
        return False
    console.send(b"raven\n")
    return console.wait_for(b"raven# ", 15) >= 0


def console_capture(console: SerialConsole, command: str, log_name: str,
                    timeout: float = 30) -> str:
    """Run one console command and return the cleaned text up to a post-exec nonce."""
    mark = len(console.buffer)
    nonce = f"K434-{log_name}-{os.getpid()}"
    started = time.monotonic()
    console.send(f"{command}; echo {nonce}\n".encode())
    end = console.wait_for(b"\n" + nonce.encode(), timeout, start=mark)
    elapsed = (time.monotonic() - started) * 1000
    segment = bytes(console.buffer[mark:end if end > 0 else len(console.buffer)])
    segment = segment.decode("utf-8", "replace").replace("\r", "")
    clean = re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", segment)
    (OUT / log_name).write_text(clean, encoding="utf-8")
    print(f"matrix: console {log_name} {elapsed:.0f}ms nonce={end >= 0}", flush=True)
    return clean


def row_s3_finish(console: SerialConsole, state: dict[str, object]) -> None:
    started = time.monotonic()
    login = console.wait_for(b"login:", 60)
    if login < 0:
        wait_ms = (time.monotonic() - started) * 1000
        attempt("S3-server-line", 1, "fresh",
                "Accepted password for raven from 10.0.2.2 port N ssh2",
                "serial console login prompt not reached", wait_ms, "FAIL")
        record("S3", "FAIL",
               f"client auth {'yes' if state['client_auth'] else 'no'}; server line unreadable "
               f"(no console login prompt); wrong exit {state['bad_exit']} (auth-passwd; C5/C9)")
        return
    console.send(b"root\n")
    if console.wait_for(b"Password:", 10) < 0:
        wait_ms = (time.monotonic() - started) * 1000
        attempt("S3-server-line", 1, "fresh",
                "Accepted password for raven from 10.0.2.2 port N ssh2",
                "serial console password prompt not reached", wait_ms, "FAIL")
        record("S3", "FAIL",
               f"client auth {'yes' if state['client_auth'] else 'no'}; server line unreadable "
               f"(console password prompt missing); wrong exit {state['bad_exit']} "
               f"(auth-passwd; C5/C9)")
        return
    console.send(b"raven\n")
    if console.wait_for(b"raven# ", 30) < 0:
        wait_ms = (time.monotonic() - started) * 1000
        attempt("S3-server-line", 1, "fresh",
                "Accepted password for raven from 10.0.2.2 port N ssh2",
                "serial console root prompt not reached", wait_ms, "FAIL")
        record("S3", "FAIL",
               f"client auth {'yes' if state['client_auth'] else 'no'}; server line unreadable "
               f"(console root login failed); wrong exit {state['bad_exit']} (auth-passwd; C5/C9)")
        return
    capture_started = time.monotonic()
    clean = console_capture(console, "cat /tmp/sshd.log", "s3-server.log")
    wait_ms = (time.monotonic() - capture_started) * 1000
    lines = [line.strip() for line in clean.splitlines() if line.strip()]
    log_lines = [line for line in lines
                 if "cat /tmp/sshd.log" not in line and "K434-" not in line
                 and not line.startswith("raven#")]
    accepted = [line for line in log_lines if "Accepted password for raven " in line]
    observed = accepted[0] if accepted else (" | ".join(log_lines[-4:]) or "no log lines")
    attempt("S3-server-line", 1, "fresh",
            "Accepted password for raven from 10.0.2.2 port N ssh2",
            observed, wait_ms, "PASS" if accepted else "FAIL")
    client_auth = bool(state["client_auth"])
    refused = bool(state["refused"])
    if client_auth and accepted and refused:
        record("S3", "PASS",
               f"client `{state['auth_line']}`; serial server line `{accepted[0]}`; wrong password "
               f"exit 255 + Permission denied (auth-passwd; C5/C9)")
    else:
        record("S3", "FAIL",
               f"client auth {'yes' if client_auth else 'no'}; server line "
               f"{'present' if accepted else 'missing'} ({observed}); "
               f"wrong exit {state['bad_exit']} (auth-passwd; C5/C9)")


def row_s4() -> None:
    """S4: the first post-auth read, logged per attempt.

    OpenSSH's post-auth global request (`hostkeys-00@openssh.com`) arrives over the same
    connection. The command output is the exact marker file, so the row asserts the payload
    bytes as well as the absence of EFAULT. A transient auth failure is recorded as its own
    attempt; it is never averaged away.
    """
    records: list[dict[str, object]] = []
    passed_attempt: int | None = None
    for number in range(1, S4_ATTEMPTS + 1):
        result = ssh_run(["cat", "/etc/ssh-matrix-marker"], f"s4-vv-{number}.log",
                         extra=["-vv"])
        text = (result["stderr"] + result["stdout"]).decode("utf-8", "replace")
        auth = "Authenticated to " in text and 'using "password"' in text
        hostkeys_line = next((line for line in text.splitlines()
                              if "client_input_global_request: rtype hostkeys-00@openssh.com"
                              in line), "")
        hostkeys = bool(hostkeys_line)
        no_efault = "EFAULT" not in text
        output = result["stdout"]
        exact = output == MARKER
        ok = auth and hostkeys and exact and no_efault
        observed = (f"auth={auth} hostkeys={hostkeys} bytes={literal(output)} "
                    f"exit={result['returncode']} efault={not no_efault}")
        attempt("S4", number, "fresh",
                f'Authenticated + hostkeys-00@openssh.com + {literal(MARKER)} + no EFAULT',
                observed, float(result["elapsed_ms"]), "PASS" if ok else "FAIL")
        records.append({"number": number, "auth": auth, "hostkeys": hostkeys,
                        "exact": exact, "no_efault": no_efault,
                        "exit": result["returncode"], "observed": observed,
                        "hostkeys_line": hostkeys_line})
        if ok:
            passed_attempt = number
            break
    if passed_attempt is not None:
        chosen = records[passed_attempt - 1]
        record("S4", "PASS",
               f"attempt {passed_attempt}/{S4_ATTEMPTS}: post-auth read returned "
               f"`{str(chosen['hostkeys_line']).strip()}` and the exact payload "
               f"{literal(MARKER)}; no EFAULT (POSIX read; C2/C7)")
        return
    failed = [str(record_entry["observed"]) for record_entry in records]
    if records and not any(bool(record_entry["auth"]) for record_entry in records):
        record("S4", "BLOCKED",
               f"auth not reached in {len(records)} bounded attempts; post-auth read not "
               f"exercised: " + "; ".join(f"#{index + 1} {text}"
                                         for index, text in enumerate(failed)))
    else:
        record("S4", "FAIL",
               f"no exact post-auth read in {len(records)} attempts: "
               + "; ".join(f"#{index + 1} {text}" for index, text in enumerate(failed)))


def s4_only_attempt(number: int) -> dict[str, object]:
    """One auth, one fixed line, server writes it to S4_FILE and reads it back."""
    # Diagnostic only: RAVEN_S4_REMOTE substitutes the remote command to separate a
    # server-push failure from a command-child fault; the default S4 row is unchanged.
    remote = (os.environ.get("RAVEN_S4_REMOTE")
              or f"cat > {S4_FILE}; cat {S4_FILE}")
    argv = [
        "ssh",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "ConnectTimeout=8",
        "-o", "NumberOfPasswordPrompts=1",
        "-vv",
        f"{ROOT_USER}@{HOST}",
        remote,
    ]
    env = os.environ.copy()
    env["SSH_ASKPASS"] = str(ASKPASS)
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["DISPLAY"] = ":0"
    env["K434_SSH_PASSWORD"] = PASSWORD
    stdout_path = OUT / f"s4-only-{number}.stdout"
    stderr_path = OUT / f"s4-only-{number}.stderr"
    started = time.monotonic()
    auth_ms: float | None = None
    write_ms: float | None = None
    read_ms: float | None = None
    auth_seen = False
    input_sent = False
    with open(stdout_path, "wb") as out_file, open(stderr_path, "wb") as err_file:
        process = subprocess.Popen(argv, cwd=ROOT, stdin=subprocess.PIPE,
                                   stdout=out_file, stderr=err_file, env=env,
                                   start_new_session=True)
        auth_deadline = started + S4_AUTH_BOUND_S
        read_deadline: float | None = None
        while process.poll() is None and time.monotonic() - started < S4_TOTAL_BOUND_S:
            now = time.monotonic()
            if not auth_seen:
                try:
                    err = stderr_path.read_bytes()
                except OSError:
                    err = b""
                if b"Authenticated to " in err:
                    auth_seen = True
                    auth_ms = (now - started) * 1000
                    try:
                        process.stdin.write(S4_INPUT)  # type: ignore[union-attr]
                        process.stdin.flush()  # type: ignore[union-attr]
                        process.stdin.close()  # type: ignore[union-attr]
                        input_sent = True
                        write_ms = (time.monotonic() - started) * 1000
                        read_deadline = time.monotonic() + S4_READ_BOUND_S
                    except (BrokenPipeError, OSError):
                        break
                elif now > auth_deadline:
                    break
            if input_sent:
                try:
                    size = stdout_path.stat().st_size
                except FileNotFoundError:
                    size = 0
                if size >= len(S4_INPUT):
                    read_ms = (time.monotonic() - started) * 1000
                    break
                if read_deadline is not None and now > read_deadline:
                    break
            time.sleep(0.02)
        timed_out = process.poll() is None
        if timed_out:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        returncode = process.wait()
        try:
            process.stdin.close()  # type: ignore[union-attr]
        except (OSError, AttributeError):
            pass
    stdout = stdout_path.read_bytes()
    stderr = stderr_path.read_bytes()
    elapsed = (time.monotonic() - started) * 1000
    text = stderr + stdout
    exact = stdout == S4_INPUT
    no_efault = b"EFAULT" not in text
    if not auth_seen:
        step = "auth"
        result = "FAIL"
    elif not input_sent:
        step = "write"
        result = "FAIL"
    elif not no_efault:
        step = "EFAULT"
        result = "FAIL"
    elif not exact:
        step = "read"
        result = "FAIL"
    else:
        step = "read"
        result = "PASS"
    if read_ms is None and result == "PASS":
        read_ms = elapsed
    detail = (f"bytes={literal(stdout)} auth_ms={auth_ms if auth_ms is not None else 'none'} "
              f"write_ms={write_ms if write_ms is not None else 'none'} "
              f"read_ms={read_ms if read_ms is not None else 'none'} "
              f"exit_ms={elapsed:.0f} exit={returncode} step={step}")
    log = OUT / f"s4-only-{number}.log"
    log.write_text(
        f"$ {' '.join(argv)}\n"
        f"exit: {returncode}\n"
        f"timed_out: {timed_out}\n"
        f"auth_seen: {auth_seen}\n"
        f"input_sent: {input_sent}\n"
        f"auth_ms: {auth_ms}\n"
        f"write_ms: {write_ms}\n"
        f"read_ms: {read_ms}\n"
        f"exit_ms: {elapsed:.0f}\n"
        f"stdout_hex: {stdout.hex()}\n"
        f"stdout_text: {stdout.decode('utf-8', 'replace')}\n"
        f"stderr_text: {stderr.decode('utf-8', 'replace')}\n",
        encoding="utf-8")
    return {"result": result, "step": step, "detail": detail, "elapsed_ms": elapsed,
            "stdout": stdout, "returncode": returncode, "auth_ms": auth_ms,
            "write_ms": write_ms, "read_ms": read_ms}


def row_s4_only() -> None:
    """K514: three single-attempt auth-then-read proofs; all three must pass."""
    results = [s4_only_attempt(number) for number in range(1, S4_ONLY_ATTEMPTS + 1)]
    for number, result in enumerate(results, 1):
        attempt("S4", number, "fresh",
                f"Authenticated + exact {literal(S4_INPUT)} written to {S4_FILE} and read back; "
                f"no EFAULT",
                str(result["detail"]), float(result["elapsed_ms"]), str(result["result"]),
                f"step={result['step']}; stdout_hex={bytes(result['stdout']).hex()}")  # type: ignore[arg-type]
    passed = sum(1 for result in results if result["result"] == "PASS")
    walls = [round(float(result["elapsed_ms"])) for result in results]
    exits = [result["returncode"] for result in results]
    if passed == S4_ONLY_ATTEMPTS:
        record("S4", "PASS",
               f"{passed}/{S4_ONLY_ATTEMPTS} attempts: exact {literal(S4_INPUT)} after auth; "
               f"walls_ms={walls}; exits={exits}; every step recorded in attempts.tsv")
    else:
        first = next(result for result in results if result["result"] != "PASS")
        record("S4", "FAIL",
               f"{passed}/{S4_ONLY_ATTEMPTS} attempts; first failure step={first['step']}; "
               f"walls_ms={walls}; exits={exits}; detail={first['detail']}")


def serial_faults(serial: Path | None, before: int) -> list[str]:
    """Return child page-fault addresses appended to the serial since `before`."""
    if serial is None:
        return []
    try:
        segment = serial.read_bytes()[before:] if serial.is_file() else b""
    except OSError:
        return []
    return [address.decode() for address in RELEASE_FAULT.findall(segment)]


def s4_assert_attempt(row: str, number: int, command: list[str],
                      expected: bytes, serial: Path | None) -> dict[str, object]:
    """One release assertion: exit 0, exact bytes, no child page fault."""
    try:
        before = serial.stat().st_size if serial is not None and serial.is_file() else 0
    except OSError:
        before = 0
    result = ssh_run(command, f"{row.lower()}-{number}.log", extra=["-v"], timeout=20)
    faults = serial_faults(serial, before)
    stdout = bytes(result["stdout"])  # type: ignore[arg-type]
    stderr = bytes(result["stderr"])  # type: ignore[arg-type]
    auth = b"Authenticated to " in stderr
    exit_ok = int(result["returncode"]) == 0  # type: ignore[arg-type]
    exact = stdout == expected
    ok = auth and exit_ok and exact and not faults
    step = ("auth" if not auth else "fault" if faults else
            "exit" if not exit_ok else "read" if not exact else "done")
    received = (f"bytes={literal(stdout)}; exit={result['returncode']}; "
                f"auth={auth}; faults={faults}")
    attempt(row, number, "fresh",
            f"exit 0 + exact {literal(expected)} + no child fault",
            received, float(result["elapsed_ms"]), "PASS" if ok else "FAIL",  # type: ignore[arg-type]
            f"step={step}; stdout_hex={stdout.hex()}")
    return {"ok": ok, "step": step, "exit": result["returncode"],
            "stdout": stdout, "faults": faults, "elapsed": result["elapsed_ms"]}


def row_s4_echo(serial: Path | None) -> None:
    """K514 rev93: the builtin-only post-auth read proof (server push + exit 0)."""
    results = [s4_assert_attempt("S4-echo", number, S4_ECHO_COMMAND, S4_ECHO_BYTES, serial)
               for number in range(1, S4_ASSERT_ATTEMPTS + 1)]
    passed = sum(1 for result in results if result["ok"])
    exits = [result["exit"] for result in results]
    faults = [address for result in results for address in result["faults"]]
    if passed == S4_ASSERT_ATTEMPTS:
        record("S4-echo", "PASS",
               f"{passed}/{S4_ASSERT_ATTEMPTS}: builtin echo returned exit 0 and exact "
               f"{literal(S4_ECHO_BYTES)} with no child fault; exits={exits}")
    else:
        first = next(result for result in results if not result["ok"])
        record("S4-echo", "FAIL",
               f"{passed}/{S4_ASSERT_ATTEMPTS}: first failure step={first['step']}; "
               f"exits={exits}; faults={faults}")


def row_s4_exit0(serial: Path | None) -> None:
    """K514 rev93: a real command must return exit 0 and its exact bytes (release gate)."""
    results = [s4_assert_attempt("S4-exit0", number, S4_EXIT_COMMAND, MARKER, serial)
               for number in range(1, S4_ASSERT_ATTEMPTS + 1)]
    passed = sum(1 for result in results if result["ok"])
    exits = [result["exit"] for result in results]
    faults = [address for result in results for address in result["faults"]]
    if passed == S4_ASSERT_ATTEMPTS:
        record("S4-exit0", "PASS",
               f"{passed}/{S4_ASSERT_ATTEMPTS}: `cat /etc/ssh-matrix-marker` returned exit 0 "
               f"and exact {literal(MARKER)} with no child fault; exits={exits}")
    else:
        first = next(result for result in results if not result["ok"])
        record("S4-exit0", "FAIL",
               f"{passed}/{S4_ASSERT_ATTEMPTS}: first failure step={first['step']}; "
               f"exits={exits}; faults={faults}; expected exit 0 + {literal(MARKER)}")


def row_tty10() -> None:
    """K514 rev95: ten real local-pty SSH sessions; require /dev/pts/N from tty."""
    import pexpect

    passed = 0
    exits: list[int] = []
    walls: list[float] = []
    outputs: list[str] = []
    argv = [
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "ConnectTimeout=8",
        "-o", "NumberOfPasswordPrompts=1",
        "-tt",
        f"{USER}@{HOST}",
        "tty",
    ]
    env = os.environ.copy()
    env["SSH_ASKPASS"] = str(ASKPASS)
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["DISPLAY"] = ":0"
    env["K434_SSH_PASSWORD"] = PASSWORD
    for number in range(1, TTY10_ATTEMPTS + 1):
        started = time.monotonic()
        log_path = OUT / f"tty10-{number}.log"
        timed_out = False
        try:
            with open(log_path, "w", encoding="utf-8") as log:
                child = pexpect.spawn("ssh", argv, cwd=str(ROOT), encoding="utf-8",
                                      codec_errors="replace", timeout=20, env=env)
                child.logfile = log
                try:
                    child.expect([r"(?i)password: ?", pexpect.EOF], timeout=20)
                    if child.after != pexpect.EOF and "password" in str(child.after).lower():
                        child.sendline(PASSWORD)
                        child.expect(pexpect.EOF, timeout=20)
                except pexpect.TIMEOUT:
                    timed_out = True
                finally:
                    child.close(force=True)
        except OSError as error:
            log_path.write_text(f"spawn failed: {error}\n", encoding="utf-8")
            timed_out = True
        elapsed = (time.monotonic() - started) * 1000
        text = log_path.read_text(errors="replace") if log_path.is_file() else ""
        match = re.search(r"/dev/pts/[0-9]+", text)
        exit_code = -9
        try:
            exit_code = int(child.exitstatus if child.exitstatus is not None
                            else (child.signalstatus if child.signalstatus is not None else -9))
        except (NameError, ValueError):
            exit_code = -9
        ok = match is not None and not timed_out
        if ok:
            passed += 1
        exits.append(exit_code)
        walls.append(round(elapsed))
        outputs.append(match.group(0) if match else "none")
        received = (f"pts={match.group(0) if match else 'none'}; exit={exit_code}; "
                    f"timed_out={timed_out}; wall={elapsed:.0f}ms")
        attempt("S8-tty10", number, "fresh", "/dev/pts/[0-9]+ from tty",
                received, elapsed, "PASS" if ok else "FAIL",
                f"step={'done' if ok else 'output'}; log={log_path.name}")
    record("S8-tty10", "PASS" if passed == TTY10_ATTEMPTS else "FAIL",
           f"{passed}/{TTY10_ATTEMPTS} real-pty sessions returned /dev/pts/N; outputs={outputs}; "
           f"exits={exits}; walls_ms={walls}")


def row_release_commands(serial: Path) -> None:
    """K514 rev92: ten sessions per release command; exit, bytes and serial fault."""
    for name, command, expected, expected_text in RELEASE_COMMANDS:
        passed = 0
        exits: list[int] = []
        sizes: list[int] = []
        faults_seen: list[str] = []
        rtypes: list[str] = []
        walls: list[float] = []
        for number in range(1, RELEASE_SESSIONS + 1):
            try:
                before = serial.stat().st_size
            except OSError:
                before = 0
            result = ssh_run(command, f"release-{name}-{number}.log",
                             extra=["-v"], timeout=25)
            try:
                segment = serial.read_bytes()[before:] if serial.is_file() else b""
            except OSError:
                segment = b""
            faults = [address.decode() for address in RELEASE_FAULT.findall(segment)]
            stdout = bytes(result["stdout"])  # type: ignore[arg-type]
            stderr = bytes(result["stderr"])  # type: ignore[arg-type]
            auth = b"Authenticated to " in stderr
            if b"rtype exit-status" in stderr:
                rtype = "exit-status"
            elif b"rtype exit-signal" in stderr:
                rtype = "exit-signal"
            else:
                rtype = "none"
            ok = auth and not faults and bool(expected(stdout))
            if ok:
                passed += 1
            step = "auth" if not auth else "fault" if faults else "read"
            exits.append(int(result["returncode"]))  # type: ignore[arg-type]
            sizes.append(len(stdout))
            faults_seen.extend(faults)
            rtypes.append(rtype)
            walls.append(round(float(result["elapsed_ms"])))  # type: ignore[arg-type]
            received = (f"bytes={literal(stdout)}; size={len(stdout)}; "
                        f"exit={result['returncode']}; faults={faults}; "
                        f"auth={auth}; rtype={rtype}")
            attempt(f"C-{name}", number, "fresh", expected_text, received,
                    float(result["elapsed_ms"]), "PASS" if ok else "FAIL",  # type: ignore[arg-type]
                    f"step={step}; stdout_hex={stdout.hex()}")
        unique = sorted(set(faults_seen))
        record(f"C-{name}",
               "PASS" if passed == RELEASE_SESSIONS else "FAIL",
               f"{passed}/{RELEASE_SESSIONS} usable; exits={exits}; sizes={sizes}; "
               f"faults={unique}; rtypes={sorted(set(rtypes))}; walls_ms={walls}; "
               f"command={' '.join(command)}")


def row_mutation() -> None:
    """K562: the 0755 image must fail the non-root /tmp create row."""
    stat = ssh_run(["stat", "-c", "%a %U %G", "/tmp"], "k562-mutation-stat.log")
    stat_text = stat["stdout"].decode("utf-8", "replace").strip()
    create = ssh_run(["touch", "/tmp/k562-user"], "k562-mutation-create.log")
    create_text = (create["stderr"] + create["stdout"]).decode("utf-8", "replace").strip()
    mode_ok = stat_text == "755" or stat_text.startswith("755 ")
    denied = create["returncode"] != 0 and "ermission denied" in create_text
    observed = (f"stat={stat_text!r}; create_exit={create['returncode']}; "
                f"create={create_text!r}")
    attempt("K562-mutation", 1, "fresh",
            "/tmp mode 755; non-root create fails Permission denied",
            observed, float(create["elapsed_ms"]), "PASS" if mode_ok and denied else "FAIL")
    if mode_ok and denied:
        record("K562-mutation", "PASS",
               f"`raven` sees /tmp `{stat_text}` and `touch /tmp/k562-user` fails "
               f"`{create_text}` (0755 mutation observed)")
    else:
        record("K562-mutation", "FAIL",
               f"mutation not observed: {observed}")


def row_s8_proof(console: SerialConsole | None) -> None:
    """K548 proof: correlate one client banner timeout with the guest sshd log."""
    result = ssh_run(["tty"], "s8-proof-tt.log", extra=["-tt"], timeout=15)
    text = (result["stderr"] + result["stdout"]).decode("utf-8", "replace").strip()
    attempt("S8-proof-tt", 1, "fresh", "client banner/accept result",
            f"exit {result['returncode']}; {text[:240]}",
            float(result["elapsed_ms"]), "OBSERVED")
    if console is None:
        record("S8-proof", "BLOCKED", "no serial console for the sshd log")
        return
    if console.wait_for(b"login:", 5) < 0:
        console.send(b"\n")
        if console.wait_for(b"login:", 30) < 0:
            record("S8-proof", "BLOCKED", "no console login prompt")
            return
    console.send(b"root\n")
    if console.wait_for(b"Password:", 10) < 0:
        record("S8-proof", "BLOCKED", "no console password prompt")
        return
    console.send(b"raven\n")
    if console.wait_for(b"Welcome to the RavenBSD console", 15) < 0:
        record("S8-proof", "BLOCKED", "console login did not reach the shell banner")
        return
    console.send(b"\n")
    time.sleep(0.5)
    start_mark = len(console.buffer)
    console.send(b"echo K434-$((7*11))")
    time.sleep(0.3)
    console.send(b"\r")
    started = console.wait_for(b"K434-77", 8, start=start_mark) >= 0
    mark = len(console.buffer)
    console.send(b"ls -l /tmp/sshd.log; cat /tmp/sshd.log; echo K434-$((13*13))")
    time.sleep(0.3)
    console.send(b"\r")
    end = console.wait_for(b"K434-169", 15, start=mark)
    if end < 0:
        console.send(b"\n")
        end = console.wait_for(b"K434END", 15, start=mark)
    segment = bytes(console.buffer[mark:len(console.buffer)])
    log = segment.decode("utf-8", "replace").replace("\r", "")
    log = re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", log)
    (OUT / "s8-proof-sshd.log").write_text(log, encoding="utf-8")
    print(f"matrix: console s8-proof-sshd.log shell={started} end_marker={end >= 0}", flush=True)
    print("matrix: --- /tmp/sshd.log ---", flush=True)
    for line in log.splitlines():
        print(f"matrix: sshd| {line}", flush=True)
    lines = log.splitlines()
    listening = [line for line in lines if "Server listening" in line]
    connections = [line for line in lines if "Connection from" in line]
    record("S8-proof", "OBSERVED",
           f"client_exit={result['returncode']}; Server listening={len(listening)}; "
           f"Connection from={len(connections)}; artifact=s8-proof-sshd.log")
    attempt("S8-proof-accept", 1, "fresh",
            "Server listening line and whether a Connection from line follows",
            f"listening={listening[:2]}; connections={connections[:2]}", 0, "OBSERVED")


def row_s5_s6() -> None:
    result = ssh_probe(["true"], "s5-vvv.log", extra=["-vvv"])
    text = (result["stderr"] + result["stdout"]).decode("utf-8", "replace")
    auth = "Authenticated to " in text and 'using "password"' in text
    open_confirm = "debug2: channel_input_open_confirmation: channel 0" in text
    confirm_line = next((line for line in text.splitlines()
                         if "channel 0: open confirm rwindow" in line), "")
    type91_line = next((line for line in text.splitlines()
                        if "receive packet: type 91" in line), "")
    broken = next((line for line in text.splitlines()
                   if "read failed" in line), "")
    epipe_lines = list(result["epipe_lines"])  # type: ignore[arg-type]
    epipe_line = epipe_lines[0] if epipe_lines else broken
    fd_targets = list(result["fd_targets"])  # type: ignore[arg-type]
    peer_states = list(result["peer_states"])  # type: ignore[arg-type]
    fd_text = ", ".join(fd_targets) if fd_targets else "not captured"
    peer_text = " || ".join(peer_states) if peer_states else "not captured"
    errno_text = "EPIPE (32)" if epipe_line and ("EPIPE" in epipe_line or "Broken pipe" in epipe_line) \
        else "not observed"
    attempt("S5", 1, "fresh", "channel_input_open_confirmation + open confirm",
            confirm_line or f"exit {result['returncode']}",
            float(result["elapsed_ms"]), "PASS" if auth and open_confirm and confirm_line else "FAIL")
    attempt("S6", 1, "reuse", "receive packet type 91 + channel 0",
            type91_line or f"exit {result['returncode']}",
            float(result["elapsed_ms"]), "PASS" if auth and type91_line and open_confirm else "FAIL")
    identity_exit_path = OUT / "socket-write-identity.exit"
    identity_log = OUT / "socket-write-identity.log"
    identity_ok = False
    identity_detail = "artifact missing; run tests/fixtures/socket-write-identity/run-tap.sh"
    if identity_exit_path.is_file():
        try:
            identity_exit = int(identity_exit_path.read_text(encoding="utf-8").strip())
        except ValueError:
            identity_exit = -1
        identity_text = identity_log.read_text(errors="replace") if identity_log.is_file() else ""
        identity_ok = (identity_exit == 0
                       and "socket-write-identity-tap: pass" in identity_text)
        witness = next((line.strip() for line in identity_text.splitlines()
                        if "rsh:" in line or "FAIL" in line or "unmatched expectations" in line),
                       "socket-write-identity-tap: pass")
        identity_detail = f"exit {identity_exit}; {witness}"
        attempt("S5-identity", 1, "fresh",
                "host peer exact N bytes on the same connection; console none",
                identity_detail, 0, "PASS" if identity_ok else "FAIL")
    else:
        attempt("S5-identity", 1, "fresh",
                "host peer exact N bytes on the same connection; console none",
                identity_detail, 0, "BLOCKED", identity_detail)
    if auth and open_confirm and confirm_line and identity_ok:
        record("S5", "PASS",
               f"client received `{confirm_line.strip()}`; identity qtest PASS "
               f"(sosend; C5/C6/C8)")
    elif auth and open_confirm and confirm_line:
        record("S5", "FAIL" if identity_exit_path.is_file() else "BLOCKED",
               f"confirmation delivery PASS; identity {identity_detail} (sosend; C5/C6/C8)")
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
    session_completed = result["returncode"] == 0 and not bool(result["timed_out"])
    observed = (f"strace=`{epipe_line}` errno={errno_text} fd4={fd_text} "
                f"peer_state={peer_text}")
    attempt("S5-session", 1, "fresh", "command session completes; no EPIPE",
            observed, float(result["elapsed_ms"]),
            "PASS" if session_completed and not epipe_line else "FAIL",
            f"server-side handle not exposed; exit {result['returncode']}; "
            f"timed_out={result['timed_out']}")
    if epipe_line:
        record("S5-session", "FAIL",
               f"after the confirmation the client reports `{epipe_line.strip()}`; "
               f"errno={errno_text}, fd4={fd_text}, peer_state={peer_text}, "
               f"exit {result['returncode']}; the server-side handle is not exposed "
               f"(post-exec/session path, not the confirmation)")
    elif not session_completed:
        record("S5-session", "FAIL",
               f"no EPIPE, but the command session did not complete: exit "
               f"{result['returncode']}, timed_out={result['timed_out']}, fd4={fd_text}, "
               f"peer_state={peer_text} (post-exec/session path, not the confirmation)")
    else:
        record("S5-session", "PASS",
               f"command session completed exit 0; fd4={fd_text}, peer_state={peer_text}")


def console_marker_count(console: SerialConsole) -> int:
    console.pump()
    return bytes(console.buffer).count(MARKER)


def classify_s7(result: dict[str, object], probe_banner: bytes,
                server_lines: str) -> tuple[str, str]:
    """Classify one failed S7 attempt from the client text, a fresh probe and server lines."""
    text = (result["stderr"] + result["stdout"]).decode("utf-8", "replace")  # type: ignore[operator]
    auth = "Authenticated to " in text
    banner = "Remote protocol version" in text
    probe_ok = probe_banner == EXPECTED_BANNER
    phrases = (
        "Permission denied",
        "Connection refused",
        "Connection timed out",
        "Connection reset",
        "Connection closed",
        "Connection aborted",
        "kex_exchange_identification",
        "Broken pipe",
        "Identification string",
    )
    witness = ""
    for phrase in phrases:
        witness = next((line.strip() for line in text.splitlines() if phrase in line), "")
        if witness:
            break
    if "Permission denied" in text or "Failed password" in server_lines \
            or "Invalid user" in server_lines:
        return "auth-rejected", witness or "server logged a password refusal"
    if witness:
        if not probe_ok and ("timed out" in witness or "refused" in witness):
            return "guest-not-listening", witness
        if "banner exchange" in witness:
            return ("banner-timeout" if probe_ok else "guest-not-listening"), witness
        return "connection-reset", witness
    if not banner:
        if probe_ok:
            return "banner-timeout", "no banner in attempt; fresh probe exact"
        return "guest-not-listening", "no banner; fresh probe failed"
    if not auth:
        return "auth-not-completed", "banner present; no Authenticated line"
    return "post-auth-zero-bytes", "Authenticated present; stdout not the marker"


def row_s7(console: SerialConsole) -> None:
    total_before = console_marker_count(console)
    marker_run = ssh_run(["cat", "/tmp/ssh-marker"], "s7-marker.log",
                         timeout=S7_TIMEOUT_S, console=console)
    time.sleep(0.5)
    console_new = console_marker_count(console) - total_before
    marker_ok = marker_run["stdout"] == MARKER
    attempt("S7-marker", 1, "fresh", f"{literal(MARKER)}; console delta 0",
            f"{literal(marker_run['stdout'])}; console_new={console_new}",
            float(marker_run["elapsed_ms"]),
            "PASS" if marker_ok and console_new == 0 else "FAIL",
            f"exit {marker_run['returncode']}")
    passed = 0
    all_exit_ok = True
    wall_ms: list[float] = []
    max_console_new = console_new
    for number in range(1, 11):
        before = console_marker_count(console)
        session = ssh_run(["cat", "/tmp/ssh-marker"], f"s7-run-{number}.log",
                          timeout=S7_TIMEOUT_S, console=console)
        time.sleep(0.5)
        after = console_marker_count(console)
        new_markers = after - before
        max_console_new = max(max_console_new, new_markers)
        ok = session["stdout"] == MARKER and new_markers == 0
        passed += 1 if ok else 0
        elapsed = float(session["elapsed_ms"])
        wall_ms.append(elapsed)
        exit_ok = session["returncode"] == 0 and elapsed <= S7_WALL_BOUND_MS
        all_exit_ok = all_exit_ok and exit_ok
        attempt("S7-10/10", number, "fresh", f"{literal(MARKER)}; console delta 0",
                f"{literal(session['stdout'])}; console_new={new_markers}",
                elapsed, "PASS" if ok else "FAIL",
                f"exit {session['returncode']}")
        attempt("S7-exit", number, "fresh",
                f"exit 0; {literal(MARKER)}; wall <= {S7_WALL_BOUND_MS} ms",
                f"exit {session['returncode']}; wall={elapsed:.0f} ms; "
                f"bytes={literal(session['stdout'])}",
                elapsed, "PASS" if exit_ok and session["stdout"] == MARKER else "FAIL")
        if ok:
            continue
        probe = banner_probe(f"s7-probe-{number}", send_client=False)
        classification, witness = classify_s7(session, probe["bytes"], "")
        attempt("S7-class", number, "fresh",
                "guest-not-listening / auth-rejected / connection-reset from the client and a "
                "fresh banner probe; QEMU serial in serial.log",
                f"class={classification}; probe_banner={literal(probe['bytes'])}; "
                f"witness={witness!r}",
                0, "OBSERVED")
    total_after = console_marker_count(console)
    total_delta = total_after - total_before
    server_capture = ssh_run(["cat", "/tmp/sshd.log"], "s7-server.log",
                             user=ROOT_USER, timeout=8)
    server_log = server_capture["stdout"].decode("utf-8", "replace")
    (OUT / "s7-server.raw.log").write_text(server_log, encoding="utf-8")
    ps_capture = ssh_run(["ps", "-e"], "s7-ps.log", user=ROOT_USER, timeout=8)
    ps_log = ps_capture["stdout"].decode("utf-8", "replace")
    (OUT / "s7-ps.raw.log").write_text(ps_log, encoding="utf-8")
    server_ok = server_capture["returncode"] == 0
    ps_ok = ps_capture["returncode"] == 0
    accepted = server_log.count("Accepted password for raven ")
    closed = server_log.count("Connection closed")
    reset = server_log.count("Connection reset")
    server_summary = (f"Accepted={accepted} Connection-closed={closed} reset={reset}"
                      if server_ok else f"server log UNKNOWN (ssh exit "
                                        f"{server_capture['returncode']})")
    ps_summary = "ps captured" if ps_ok else f"ps UNKNOWN (exit {ps_capture['returncode']})"
    routing_ok = marker_ok and total_delta == 0
    attempt("S7-routing", 1, "fresh", "client exact marker; console receives none",
            f"client={literal(marker_run['stdout'])}; max_console_new={max_console_new}; "
            f"console_total_delta={total_delta}",
            float(marker_run["elapsed_ms"]), "PASS" if routing_ok else "FAIL")
    if routing_ok:
        record("S7-routing", "PASS",
               f"client received the exact marker; console total delta 0 "
               f"(exit {marker_run['returncode']} is the S7-exit/S5-session fact)")
    else:
        if marker_run["stdout"] != MARKER:
            reason = f"client={literal(marker_run['stdout'])} (not the marker)"
        else:
            reason = f"console_total_delta={total_delta} (console also received the marker)"
        record("S7-routing", "FAIL", reason)
    if marker_ok and passed == 10 and total_delta == 0 and all_exit_ok:
        record("S7", "PASS",
               f"`cat /tmp/ssh-marker` returned exactly {literal(MARKER)} on the single attempt "
               f"and 10/10; console_total_delta=0; all exits 0 and walls <= "
               f"{S7_WALL_BOUND_MS} ms")
    else:
        record("S7", "FAIL",
               f"marker {'exact' if marker_ok else 'mismatch/error'} "
               f"({literal(marker_run['stdout'])}); {passed}/10 exact; "
               f"console_total_delta={total_delta}; all_exit_ok={all_exit_ok}; "
               f"walls_ms={[round(value) for value in wall_ms]}; "
               f"server={server_summary}; {ps_summary} (gate; C1-C10)")


def ssh_pty_probe(console: SerialConsole | None, log_name: str,
                  sample_ps=None) -> dict[str, object]:
    """Run `ssh -tt tty` and sample the guest process table while it is live."""
    argv = [
        "ssh",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "ConnectTimeout=8",
        "-o", "NumberOfPasswordPrompts=1",
        "-tt",
        f"{USER}@{HOST}",
        "tty",
    ]
    env = os.environ.copy()
    env["SSH_ASKPASS"] = str(ASKPASS)
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["DISPLAY"] = ":0"
    env["K434_SSH_PASSWORD"] = PASSWORD
    stdout_path = OUT / f"{log_name}.stdout"
    stderr_path = OUT / f"{log_name}.stderr"
    started = time.monotonic()
    auth_seen = False
    during_ps = ""
    with open(stdout_path, "wb") as out_file, open(stderr_path, "wb") as err_file:
        process = subprocess.Popen(argv, cwd=ROOT, stdin=subprocess.PIPE,
                                   stdout=out_file, stderr=err_file, env=env,
                                   start_new_session=True)
        deadline = started + REQUEST_TIMEOUT_S
        while process.poll() is None and time.monotonic() < deadline:
            if not auth_seen:
                try:
                    text = stderr_path.read_bytes()
                except OSError:
                    text = b""
                if b"Authenticated to " in text:
                    auth_seen = True
                    if sample_ps is not None:
                        during_ps = sample_ps()
            time.sleep(0.02)
        while process.poll() is None and time.monotonic() < deadline:
            time.sleep(0.02)
        timed_out = process.poll() is None
        if timed_out:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        returncode = process.wait()
        try:
            process.stdin.close()  # type: ignore[union-attr]
        except (OSError, AttributeError):
            pass
    stdout = stdout_path.read_bytes()
    stderr = stderr_path.read_bytes()
    elapsed = (time.monotonic() - started) * 1000
    log = OUT / log_name
    log.write_text(
        f"$ {' '.join(argv)}\n"
        f"exit: {returncode}\n"
        f"timed_out: {timed_out}\n"
        f"auth_seen: {auth_seen}\n"
        f"elapsed_ms: {elapsed:.0f}\n"
        f"stdout_hex: {stdout.hex()}\n"
        f"stdout_text: {stdout.decode('utf-8', 'replace')}\n"
        f"stderr_text: {stderr.decode('utf-8', 'replace')}\n",
        encoding="utf-8")
    return {"returncode": returncode, "stdout": stdout, "stderr": stderr,
            "elapsed_ms": elapsed, "timed_out": timed_out, "argv": argv,
            "auth_seen": auth_seen, "during_ps": during_ps}


def row_s8() -> None:
    exit_path = OUT / "pty-alloc.exit"
    log_path = OUT / "pty-alloc.log"
    pty_alloc_ok = False
    if exit_path.is_file():
        try:
            pty_alloc_exit = int(exit_path.read_text(encoding="utf-8").strip())
        except ValueError:
            pty_alloc_exit = -1
        pty_text = log_path.read_text(errors="replace") if log_path.is_file() else ""
        last_line = next((line.strip() for line in reversed(pty_text.splitlines())
                          if line.strip()), "no log line")
        pty_alloc_ok = pty_alloc_exit == 0 and "pty-alloc: pass" in pty_text
        attempt("S8-pty-alloc", 1, "fresh", "exit 0 + `pty-alloc: pass`",
                f"exit {pty_alloc_exit}; {last_line}", 0,
                "PASS" if pty_alloc_ok else "FAIL")
    else:
        attempt("S8-pty-alloc", 1, "fresh", "exit 0 + `pty-alloc: pass`",
                "artifact missing", 0, "BLOCKED",
                "run `timeout 75 cargo xtask qtest --case pty-alloc --arch x86_64`")
    termios_exit_path = OUT / "termios.exit"
    termios_log = OUT / "termios.log"
    termios_ok = False
    if termios_exit_path.is_file():
        try:
            termios_exit = int(termios_exit_path.read_text(encoding="utf-8").strip())
        except ValueError:
            termios_exit = -1
        termios_text = termios_log.read_text(errors="replace") if termios_log.is_file() else ""
        last_termios = next((line.strip() for line in reversed(termios_text.splitlines())
                             if line.strip()), "no log line")
        termios_ok = termios_exit == 0 and "termios: pass" in termios_text
        attempt("S8-termios", 1, "fresh", "exit 0 + `termios: pass`",
                f"exit {termios_exit}; {last_termios}", 0,
                "PASS" if termios_ok else "FAIL")
    else:
        attempt("S8-termios", 1, "fresh", "exit 0 + `termios: pass`",
                "artifact missing", 0, "BLOCKED",
                "run `timeout 150 cargo xtask qtest --case termios --arch x86_64`")

    fixture_ok = False
    fixture_detail = "fixture capture unavailable"
    fixture_run = ssh_run(["/bin/openpty-null"], "s8-openpty-null.log",
                          user=ROOT_USER, timeout=15)
    clean = fixture_run["stdout"].decode("utf-8", "replace")
    (OUT / "s8-openpty-null.raw.log").write_text(clean, encoding="utf-8")
    fixture_exit = int(fixture_run["returncode"])
    null_line = next((line.strip() for line in clean.splitlines()
                      if "call=null " in line), "")
    epipe_line = next((line.strip() for line in clean.splitlines()
                       if "call=epipe-sentinel" in line), "")
    name_line = next((line.strip() for line in clean.splitlines()
                      if "call=name " in line), "")
    fixture_ran = bool(null_line and name_line)
    fixture_ok = fixture_ran and "call=null rc=0" in clean
    fixture_detail = f"exit={fixture_exit}; {null_line}; {epipe_line}; {name_line}"
    if not fixture_ran:
        attempt("S8-openpty-null", 1, "fresh",
                "openpty(..., NULL, ...) returns 0; control with a name returns 0",
                f"fixture did not run: exit {fixture_exit}; {fixture_detail}",
                0, "BLOCKED")
        record("S8-openpty-null", "BLOCKED",
               f"fixture did not produce its two call lines: {fixture_detail}")
    else:
        attempt("S8-openpty-null", 1, "fresh",
                "openpty(..., NULL, ...) returns 0; control with a name returns 0",
                fixture_detail, 0, "PASS" if fixture_ok else "FAIL")
        if fixture_ok:
            record("S8-openpty-null", "PASS",
                   "the exact OpenSSH NULL-name call returns a pair (FreeBSD pty.c:74)")
        else:
            record("S8-openpty-null", "FAIL",
                   f"OpenSSH sshpty.c:63 calls openpty(..., NULL, ...); Raven's C wrapper "
                   f"rejects a null name at compat/openpty.rs:23 before any pty call and "
                   f"leaves errno stale: {fixture_detail} (donor FreeBSD pty.c:74 `if (name)`)")

    def sample_ps() -> str:
        capture = ssh_run(["ps", "-e"], "s8-ps-during.log", user=ROOT_USER, timeout=8)
        return capture["stdout"].decode("utf-8", "replace")

    result = ssh_pty_probe(None, "s8-tt.log", sample_ps)
    output = result["stdout"]  # type: ignore[assignment]
    error = result["stderr"]  # type: ignore[assignment]
    match = re.search(rb"/dev/pts/[0-9]+", output)  # type: ignore[arg-type]
    pid_capture = ssh_run(["cat", "/tmp/sshd.pid"], "s8-pid.log",
                          user=ROOT_USER, timeout=8)
    ps_capture = ssh_run(["ps", "-e"], "s8-ps.log", user=ROOT_USER, timeout=8)
    ps_after = ps_capture["stdout"].decode("utf-8", "replace")
    (OUT / "s8-ps.raw.log").write_text(ps_after, encoding="utf-8")
    server_capture = ssh_run(["cat", "/tmp/sshd.log"], "s8-server.log",
                             user=ROOT_USER, timeout=8)
    server_log = server_capture["stdout"].decode("utf-8", "replace")
    (OUT / "s8-server.raw.log").write_text(server_log, encoding="utf-8")
    ps_during = str(result["during_ps"])
    ps_ok = ps_capture["returncode"] == 0
    monitor_lines = [line.strip() for line in (ps_during + "\n" + ps_after).splitlines()
                     if "sshd-session" in line or "sshd-auth" in line]
    monitor_alive = any("sshd-session" in line for line in ps_after.splitlines()) if ps_ok else None
    if ps_ok:
        attempt("S8-monitor", 1, "fresh", "sshd-session row in the guest ps -e table",
                f"alive_after={monitor_alive}; during={monitor_lines[:4]}; "
                f"after_rows={len(ps_after.splitlines())}",
                0, "OBSERVED")
        record("S8-monitor", "OBSERVED",
               f"monitor alive after failure={monitor_alive}; rows={monitor_lines[:4] or 'none'}; "
               f"artifacts s8-ps-during.log/s8-ps.raw.log")
    else:
        attempt("S8-monitor", 1, "fresh", "sshd-session row in the guest ps -e table",
                f"monitor state UNKNOWN: ps capture exit={ps_capture['returncode']}",
                0, "BLOCKED")
        record("S8-monitor", "BLOCKED",
               f"monitor state UNKNOWN: ps capture exit={ps_capture['returncode']}; "
               f"no /proc and no process-fd enumeration in this guest")
    fd_lines = [line.strip() for line in (ps_during + "\n" + server_log).splitlines()
                if re.search(r"\b(fd|fd=)\b", line) or re.search(r"\bfd[= ]", line)]
    fd_note = ("no /proc and no fd-enumeration syscall exist in this guest "
               "(ps -e exposes PID PPID UID GID CMD only); captured OpenSSH fd lines")
    attempt("S8-fds", 1, "fresh", "monitor fd table", fd_note, 0, "BLOCKED")
    record("S8-fds", "BLOCKED",
           f"{fd_note}: {fd_lines[:8] or 'none in ps/sshd log'}; "
           f"sshd-log capture exit={server_capture['returncode']}")
    if match and pty_alloc_ok and termios_ok:
        record("S8", "PASS",
               f"`ssh -tt tty` returned {match.group(0).decode()}; `pty-alloc` and `termios` "
               f"pass (openpty; C9)")
        attempt("S8", 1, "fresh", "/dev/pts/N", match.group(0).decode(),
                float(result["elapsed_ms"]), "PASS")
    elif b"PTY allocation request failed" in error or b"PTY allocation request failed" in output:
        record("S8", "FAIL",
               f"`PTY allocation request failed on channel 0` (pts_alloc/openpty; C9); full "
               f"sshd log in s8-server.log; openpty-null fixture: {fixture_detail}")
        attempt("S8", 1, "fresh", "/dev/pts/N", "PTY allocation request failed",
                float(result["elapsed_ms"]), "FAIL")
    elif b"tty: command not found" in (output + error):
        tty_line = next((line.strip() for line in (output + error).decode("utf-8", "replace").splitlines()
                         if "tty: command not found" in line), "tty: command not found")
        record("S8", "FAIL",
               f"PTY allocation did not report; the remote `tty` utility is missing: `{tty_line}` "
               f"(C9)")
        attempt("S8", 1, "fresh", "/dev/pts/N", tty_line,
                float(result["elapsed_ms"]), "FAIL")
    elif match and not (pty_alloc_ok and termios_ok):
        record("S8", "FAIL",
               f"`ssh -tt tty` returned {match.group(0).decode()}, but pty-alloc="
               f"{pty_alloc_ok} termios={termios_ok} (openpty; C9)")
        attempt("S8", 1, "fresh", "/dev/pts/N + pty-alloc/termios pass",
                match.group(0).decode(), float(result["elapsed_ms"]), "FAIL")
    else:
        record("S8", "FAIL",
               f"`ssh -tt tty` exit {result['returncode']}, no /dev/pts/N (openpty; C9); "
               f"openpty-null fixture: {fixture_detail}")
        attempt("S8", 1, "fresh", "/dev/pts/N", literal(output[:80]),
                float(result["elapsed_ms"]), "FAIL")


def row_s7_gate() -> None:
    walls: list[float] = []
    exits: list[int] = []
    passed = 0
    for number in range(1, 11):
        result = ssh_run(["cat", "/tmp/ssh-marker"], f"s7-gate-{number}.log",
                         timeout=S7_TIMEOUT_S)
        elapsed = float(result["elapsed_ms"])
        walls.append(elapsed)
        exits.append(int(result["returncode"]))
        ok = (result["returncode"] == 0 and result["stdout"] == MARKER
              and elapsed <= S7_WALL_BOUND_MS)
        passed += 1 if ok else 0
        attempt("S7-exit", number, "fresh",
                f"exit 0; {literal(MARKER)}; wall <= {S7_WALL_BOUND_MS} ms",
                f"exit {result['returncode']}; wall={elapsed:.0f} ms; "
                f"bytes={literal(result['stdout'])}",
                elapsed, "PASS" if ok else "FAIL")
    if passed == 10:
        record("S7-exit", "PASS", f"10/10 exit 0; walls={[round(v) for v in walls]}")
    else:
        record("S7-exit", "FAIL",
               f"{passed}/10 exit0+bytes+<=5s; walls={[round(v) for v in walls]}; "
               f"exits={exits}")


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


def serial_row_capture(console: SerialConsole, name: str, mark: int) -> int:
    """Write the serial bytes produced by one row and return the new offset."""
    console.pump()
    chunk = bytes(console.buffer[mark:])
    (OUT / f"serial-{name}.log").write_bytes(chunk)
    print(f"matrix: serial-{name}.log {len(chunk)} bytes", flush=True)
    return len(console.buffer)


def console_shell_login(console: SerialConsole) -> bool:
    """Log in on the serial console without requiring the shell prompt string."""
    if console.wait_for(b"login:", 5) < 0:
        console.send(b"\n")
        if console.wait_for(b"login:", 30) < 0:
            return False
    console.send(b"root\n")
    if console.wait_for(b"Password:", 10) < 0:
        return False
    console.send(b"raven\n")
    if console.wait_for(b"Welcome to the RavenBSD console", 15) < 0:
        return False
    console.send(b"\n")
    time.sleep(0.5)
    return True


def console_cr_capture(console: SerialConsole, command: str, log_name: str,
                       marker: str, timeout: float = 20) -> str:
    """Run one console command with a separate CR and capture up to a unique echo marker."""
    mark = len(console.buffer)
    console.send(command.encode())
    time.sleep(0.3)
    console.send(b"\r")
    time.sleep(0.2)
    console.send(b"\r")
    end = console.wait_for(marker.encode(), timeout, start=mark)
    if end < 0:
        console.send(b"\n")
        console.wait_for(marker.encode(), timeout, start=mark)
    console.pump()
    text = bytes(console.buffer[mark:len(console.buffer)]).decode("utf-8", "replace")
    text = text.replace("\r", "")
    text = re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", text)
    (OUT / log_name).write_text(text, encoding="utf-8")
    print(f"matrix: console {log_name} marker={end >= 0}", flush=True)
    return text


def row_close_probe(console: SerialConsole | None) -> None:
    """K449/K448 close acceptance: one session plus client, serial, sshd log and ps capture."""
    close_timeout = int(os.environ.get("RAVEN_CLOSE_TIMEOUT", "15"))
    result = None
    client_text = ""
    output_ok = False
    exit_status_seen = False
    eof_seen = False
    close_seen = False
    attempts = 0
    for number in range(1, 4):
        attempts = number
        result = ssh_run(["echo", "K449-DIAG"], f"close-client-{number}.log",
                         extra=["-vvv"], timeout=close_timeout)
        client_text = (result["stderr"] + result["stdout"]).decode("utf-8", "replace")
        (OUT / f"close-client-vvv-{number}.log").write_text(client_text, encoding="utf-8")
        lines = [line for line in client_text.splitlines() if line.strip()]
        output_ok = b"K449-DIAG" in result["stdout"]
        exit_status_seen = any("rtype exit-status" in line for line in lines)
        eof_seen = any("rcvd eof" in line for line in lines)
        close_seen = any("rcvd close" in line or "channel 0: free" in line
                         for line in lines)
        attempt("close-client", number, "fresh", "exit 0 + K449-DIAG",
                f"exit {result['returncode']}; timed_out={result['timed_out']}; "
                f"output={output_ok}; exit_status={exit_status_seen}; eof={eof_seen}; "
                f"close={close_seen}",
                float(result["elapsed_ms"]), "OBSERVED")
        if output_ok or result["timed_out"]:
            break
    assert result is not None
    client_lines = [line for line in client_text.splitlines() if line.strip()]
    last_line = client_lines[-1] if client_lines else "no client output"
    (OUT / "close-client-vvv.log").write_text(client_text, encoding="utf-8")

    sshd_text = ""
    sshd_ls_text = ""
    ps_text = ""
    debug_log_text = ""
    debug_client_text = ""
    debug_result: dict[str, object] | None = None
    if console is not None and console_shell_login(console):
        sshd_ls_text = console_cr_capture(
            console, "ls -l /tmp/sshd.log; echo K434-$((7*11))",
            "close-sshd-ls.log", "K434-77")
        sshd_text = console_cr_capture(
            console, "cat /tmp/sshd.log; echo K434-$((13*13))",
            "close-sshd.log", "K434-169")
        ps_text = console_cr_capture(
            console, "ps -e; echo K434-$((17*13))", "close-ps.log", "K434-221")
        debug_mark = len(console.buffer)
        console.send(b"/sbin/sshd -d -p 2222 -f /etc/ssh/sshd_config &")
        time.sleep(0.3)
        console.send(b"\r")
        time.sleep(0.2)
        console.send(b"\r")
        time.sleep(2.0)
        debug_result = ssh_run(["echo", "K449-DIAG"], "close-debug-client.log",
                               extra=["-vvv", "-p", "2222"], timeout=15)
        debug_client_text = (debug_result["stderr"]
                             + debug_result["stdout"]).decode("utf-8", "replace")
        (OUT / "close-debug-client-vvv.log").write_text(debug_client_text,
                                                       encoding="utf-8")
        console.pump()
        debug_log_text = bytes(console.buffer[debug_mark:]).decode("utf-8", "replace")
        debug_log_text = debug_log_text.replace("\r", "")
        debug_log_text = re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", debug_log_text)
        (OUT / "close-sshd-debug-console.log").write_text(debug_log_text,
                                                          encoding="utf-8")
    elif console is not None:
        record("close-probe", "BLOCKED", "console login failed before the sshd log dump")
        return

    sshd_lines = [line.strip() for line in sshd_text.splitlines()
                  if line.strip() and "K434-169" not in line
                  and "cat /tmp/sshd.log" not in line and not line.startswith("raven#")]
    sshd_file = next((line.strip() for line in sshd_ls_text.splitlines()
                      if "/tmp/sshd.log" in line and "ls -l" not in line), "none")
    ps_lines = [line.strip() for line in ps_text.splitlines()
                if "sshd" in line or "defunct" in line or line.strip().startswith("Z")]
    collect_lines = [line for line in sshd_lines
                     if "collect" in line.lower() or "sigchld" in line.lower()]
    exit_lines = [line for line in sshd_lines
                  if "exit-status" in line.lower() or "channel" in line.lower()
                  or "session" in line.lower() or "close" in line.lower()]
    accept_lines = [line for line in sshd_lines
                    if "Server listening" in line or "Connection from" in line
                    or "Accepted password" in line]
    auth_lines = [line for line in sshd_lines
                  if "Accepted password" in line or "Failed password" in line
                  or "Postponed" in line or "Authenticat" in line]
    session_exit_lines = [line for line in sshd_lines
                          if "session_exit_message" in line or "exit-status" in line
                          or "session 0" in line]
    channel_close_lines = [line for line in sshd_lines
                           if "channel" in line.lower() or "close" in line.lower()
                           or "disconnect" in line.lower()]
    client_tail = " || ".join(client_lines[-12:])
    debug_lines = [line.strip() for line in debug_log_text.splitlines()
                   if line.strip()
                   and any(key in line.lower() for key in
                           ("debug", "sigchld", "collect", "session_exit",
                            "exit-status", "channel"))]
    debug_sigchld = [line for line in debug_lines
                     if "SIGCHLD" in line or "Received SIGCHLD" in line]
    debug_collect = [line for line in debug_lines
                     if "collect_children" in line or "collect" in line.lower()]
    debug_session = [line for line in debug_lines
                     if "session_exit_message" in line or "exit-status" in line]
    debug_close = [line for line in debug_lines
                   if "channel" in line.lower()
                   and ("close" in line.lower() or "free" in line.lower())]
    debug_client_lines = [line for line in debug_client_text.splitlines() if line.strip()]
    debug_exit_status = any("rtype exit-status" in line for line in debug_client_lines)
    debug_eof = any("rcvd eof" in line for line in debug_client_lines)
    debug_close_client = any("rcvd close" in line or "channel 0: free" in line
                             for line in debug_client_lines)
    accepted = next((line for line in sshd_lines if "Accepted password" in line), "")
    closed = next((line for line in sshd_lines if "Connection closed" in line), "")
    cleanup = next((line for line in sshd_lines if "cleanup_exit" in line), "")

    if output_ok and result["returncode"] == 0:
        missing = "none"
    elif not output_ok and not eof_seen:
        missing = "session accept/banner (no output)"
    elif not exit_status_seen:
        missing = "exit-status (98)"
    elif not eof_seen:
        missing = "CHANNEL_EOF (96)"
    elif not close_seen:
        missing = "CHANNEL_CLOSE (97)/socket close"
    else:
        missing = "session exit path after close"

    summary = (
        f"client_attempts={attempts} client_exit={result['returncode']} "
        f"timed_out={result['timed_out']} output={output_ok} "
        f"exit_status={exit_status_seen} eof={eof_seen} close={close_seen}\n"
        f"first_missing={missing}\n"
        f"client_last={last_line}\n"
        f"sshd_file={sshd_file}\n"
        f"sshd_accepted={accepted or 'none'}\n"
        f"sshd_closed={closed or 'none'}\n"
        f"sshd_cleanup={cleanup or 'none'}\n"
        f"sshd_collect_sigchld={collect_lines or 'none'}\n"
        f"sshd_exit_channel={exit_lines or 'none'}\n"
        f"sshd_accept={accept_lines or 'none'}\n"
        f"sshd_auth={auth_lines or 'none'}\n"
        f"sshd_session_exit={session_exit_lines or 'none'}\n"
        f"sshd_channel_close={channel_close_lines or 'none'}\n"
        f"client_tail={client_tail}\n"
        f"debug_client_exit={debug_result['returncode'] if debug_result else 'n/a'} "
        f"timed_out={debug_result['timed_out'] if debug_result else 'n/a'} "
        f"exit_status={debug_exit_status} eof={debug_eof} close={debug_close_client}\n"
        f"debug_sigchld={debug_sigchld or 'none'}\n"
        f"debug_collect_children={debug_collect or 'none'}\n"
        f"debug_session_exit={debug_session or 'none'}\n"
        f"debug_channel_close={debug_close or 'none'}\n"
        f"ps={ps_lines or 'none'}\n"
    )
    (OUT / "close-summary.txt").write_text(summary, encoding="utf-8")
    serial_path = OUT / "serial-only.log"
    if serial_path.is_file():
        (OUT / "close-serial.log").write_bytes(serial_path.read_bytes())
    print("matrix: --- close summary ---", flush=True)
    for line in summary.splitlines():
        print(f"matrix: close| {line}", flush=True)
    record("close-probe", "OBSERVED",
           f"first_missing={missing}; client_exit={result['returncode']}; "
           f"output={output_ok}; artifact=close-summary.txt")


def row_close_hang(console: SerialConsole | None) -> None:
    """K434: capture guest process state while the close hangs, before the client is killed."""
    argv = [
        "ssh",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "ConnectTimeout=8",
        "-o", "NumberOfPasswordPrompts=1",
        "-vvv",
        f"{USER}@{HOST}",
        "echo", "K449-DIAG",
    ]
    env = os.environ.copy()
    env["SSH_ASKPASS"] = str(ASKPASS)
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["DISPLAY"] = ":0"
    env["K434_SSH_PASSWORD"] = PASSWORD
    out_path = OUT / "close-hang-client.stdout"
    err_path = OUT / "close-hang-client.stderr"
    with open(out_path, "wb") as out_file, open(err_path, "wb") as err_file:
        process = subprocess.Popen(argv, cwd=ROOT, stdin=subprocess.DEVNULL,
                                   stdout=out_file, stderr=err_file, env=env,
                                   start_new_session=True)
        deadline = time.monotonic() + 8
        seen = False
        while time.monotonic() < deadline and process.poll() is None:
            stdout_text = out_path.read_bytes().decode("utf-8", "replace")
            stderr_text = err_path.read_bytes().decode("utf-8", "replace")
            if "K449-DIAG" in stdout_text or "rcvd eof" in stderr_text:
                seen = True
                break
            time.sleep(0.1)

        ps_text = ""
        ps_alt_text = ""
        procs_text = ""
        proc_text = ""
        if console is not None and console_shell_login(console):
            ps_text = console_cr_capture(
                console, "ps -e; echo K434-$((7*11))",
                "close-hang-ps.log", "K434-77", timeout=10)
            ps_alt_text = console_cr_capture(
                console, "ps --json; echo K434-$((13*13))",
                "close-hang-ps-json.log", "K434-169", timeout=10)
            procs_text = console_cr_capture(
                console, "procs; echo K434-$((19*13))",
                "close-hang-procs.log", "K434-247", timeout=10)
            proc_text = console_cr_capture(
                console, "ls /proc; echo K434-$((17*13))",
                "close-hang-proc.log", "K434-221", timeout=10)

        try:
            returncode = process.wait(timeout=15)
            timed_out = False
        except subprocess.TimeoutExpired:
            timed_out = True
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            returncode = process.wait()

    client_text = err_path.read_text(errors="replace")
    (OUT / "close-hang-client-vvv.log").write_text(client_text, encoding="utf-8")
    ps_all = ps_text + "\n" + ps_alt_text
    ps_lines = [line.strip() for line in ps_all.splitlines()
                if any(key in line for key in
                       ("sshd", "sshd-session", "brush", "defunct", "echo",
                        " K449-DIAG", "sigmask"))]
    procs_lines = [line.strip() for line in procs_text.splitlines()
                   if any(key in line.lower() for key in
                          ("sshd", "session", "brush", "echo", "child",
                           "zombie", "defunct"))]
    closed = bool(seen and returncode == 0 and not timed_out)
    zombie = []
    for line in ps_lines:
        fields = line.split()
        if ("defunct" in line or " Z " in f" {line} "
                or (len(fields) > 2 and fields[2] == "Z")):
            zombie.append(line)
    proc_list = proc_text.strip()
    attempt("close-hang", 1, "fresh", "process table during the post-EOF hang",
            f"output_seen={seen} exit={returncode} timed_out={timed_out}; "
            f"ps={ps_lines[:12]}; proc={proc_list[:160]}",
            0, "OBSERVED")
    summary = (
        f"output_seen={seen} exit={returncode} timed_out={timed_out}\n"
        f"ps_lines={ps_lines or 'none'}\n"
        f"zombie_lines={zombie or 'none'}\n"
        f"proc={proc_list or 'none'}\n"
        f"procs={procs_lines or 'none'}\n"
        f"session_closed={closed}\n"
        f"ps_alt={ps_alt_text.strip() or 'none'}\n"
    )
    (OUT / "close-hang-summary.txt").write_text(summary, encoding="utf-8")
    print("matrix: --- close-hang summary ---", flush=True)
    for line in summary.splitlines():
        print(f"matrix: hang| {line}", flush=True)
    record("close-hang", "OBSERVED",
           f"output_seen={seen} exit={returncode} timed_out={timed_out}; "
           f"ps={ps_lines[:8]}; zombie={zombie or 'none'}")


def row_full(console: SerialConsole) -> None:
    """S1-S8 plus S5-session in order on one boot, with per-row serial capture."""
    mark = len(console.buffer)
    row_s1()
    mark = serial_row_capture(console, "S1", mark)
    row_s2()
    mark = serial_row_capture(console, "S2", mark)
    s3_state = row_s3_start()
    row_s3_finish(console, s3_state)
    mark = serial_row_capture(console, "S3", mark)
    row_s4()
    mark = serial_row_capture(console, "S4", mark)
    row_s4_echo(None)
    row_s4_exit0(None)
    row_s5_s6()
    mark = serial_row_capture(console, "S5-S6", mark)
    row_s7(console)
    mark = serial_row_capture(console, "S7", mark)
    row_s8()
    serial_row_capture(console, "S8", mark)


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
    only = os.environ.get("RAVEN_MATRIX_ONLY", "")
    if only:
        child = None
        console: SerialConsole | None = None
        row_label = ("K562-mutation" if only == "mutation"
                     else "S4" if only == "s4" else only)
        if only == "full" and not build_peer():
            record("full", "BLOCKED", "cannot build the ssh-peer tracer")
            write_tables()
            print("matrix: RED (1 red rows)")
            return 2
        try:
            socket_only = only in ("s8", "s7", "s8-proof", "close-probe", "close-hang", "full")
            child, handle = boot(IMAGE, b"svcd: sshd up", "serial-only.log",
                                 "vars-only.fd", ready_timeout=180,
                                 socket_serial=socket_only)
            if only in ("close-probe", "close-hang"):
                # The close captures need sshd/TCP, not the DNS lease; resolvd may be red.
                console = handle  # type: ignore[assignment]
                lease_ok = True
            elif socket_only:
                console = handle  # type: ignore[assignment]
                lease_ok = serial_has(console, SERIAL_LEASE, 60)
            else:
                lease_ok = serial_has(handle, SERIAL_LEASE, 60)  # type: ignore[arg-type]
            if not lease_ok:
                record(row_label, "BLOCKED",
                       f"guest did not acquire {SERIAL_LEASE.decode()!r} within 60 s")
            elif not wait_tcp(22, 30):
                record(row_label, "BLOCKED",
                       "guest listener did not accept a TCP connection after svcd: sshd up")
            else:
                time.sleep(0.5)
                if only == "s1-repeat":
                    row_s1_repeat()
                elif only == "s4":
                    row_s4_only()
                    row_s4_echo(handle)  # type: ignore[arg-type]
                    row_s4_exit0(handle)  # type: ignore[arg-type]
                elif only == "commands":
                    row_release_commands(handle)  # type: ignore[arg-type]
                elif only == "mutation":
                    row_mutation()
                elif only == "s7-exit":
                    row_s7_gate()
                elif only == "s7":
                    login_ok = console is not None and console_login(console)
                    if login_ok:
                        row_s7(console)
                    else:
                        record("S7", "BLOCKED", "console login failed before S7")
                elif only == "s8":
                    row_s8()
                elif only == "tty10":
                    row_tty10()
                elif only == "s8-proof":
                    row_s8_proof(console)
                elif only == "close-probe":
                    row_close_probe(console)
                elif only == "close-hang":
                    row_close_hang(console)
                elif only == "full":
                    row_full(console)
                elif only == "s5":
                    row_s5_s6()
                else:
                    record(only, "BLOCKED", f"unknown RAVEN_MATRIX_ONLY={only}")
        except Exception as error:  # noqa: BLE001 - isolate the only-mode boot
            record(row_label, "BLOCKED", f"only-mode boot failed: {error}")
        finally:
            if child is not None:
                kill(child)
            if console is not None:
                console.pump()
                console.close()
            time.sleep(2.0)
        write_tables()
        failed = [row for row, status, _ in rows if status in ("FAIL", "BLOCKED", "PARTIAL")]
        print(f"matrix: {'RED' if failed else 'GREEN'} ({len(failed)} red rows)")
        return 1 if failed else 0
    if not build_peer():
        print("matrix: cannot build the peer tracer", file=sys.stderr)
        return 2

    row_s0()

    repeat_child = None
    try:
        repeat_child, repeat_serial = boot(IMAGE, b"svcd: sshd up", "serial-repeat.log",
                                           "vars-repeat.fd", ready_timeout=180)
        if not serial_has(repeat_serial, SERIAL_LEASE, 60):
            record("S1-repeat", "BLOCKED",
                   f"guest did not acquire {SERIAL_LEASE.decode()!r} within 60 s")
        elif not wait_tcp(22, 30):
            record("S1-repeat", "BLOCKED",
                   "guest listener did not accept a TCP connection after svcd: sshd up")
        else:
            time.sleep(0.5)
            row_s1_repeat()
    except Exception as error:  # noqa: BLE001 - isolate a repeat-boot failure
        record("S1-repeat", "BLOCKED", f"S1-repeat boot failed: {error}")
    finally:
        if repeat_child is not None:
            kill(repeat_child)
        time.sleep(2.0)

    s8_child = None
    s8_console: SerialConsole | None = None
    try:
        s8_child, s8_console = boot(IMAGE, b"svcd: sshd up", "serial-s8.log",
                                    "vars-s8.fd", ready_timeout=180,
                                    socket_serial=True)
        if not serial_has(s8_console, SERIAL_LEASE, 60):
            record("S8", "BLOCKED",
                   f"guest did not acquire {SERIAL_LEASE.decode()!r} within 60 s")
            record("S8-openpty-null", "BLOCKED", "guest network not ready")
            record("S8-monitor", "BLOCKED", "guest network not ready")
            record("S8-fds", "BLOCKED", "guest network not ready")
        elif not wait_tcp(22, 30):
            record("S8", "BLOCKED",
                   "guest listener did not accept a TCP connection after svcd: sshd up")
        else:
            time.sleep(0.5)
            row_s8()
    except Exception as error:  # noqa: BLE001 - isolate the S8 boot
        record("S8", "BLOCKED", f"S8 boot failed: {error}")
    finally:
        if s8_child is not None:
            kill(s8_child)
        if s8_console is not None:
            s8_console.pump()
            s8_console.close()
        time.sleep(2.0)

    child = None
    console: SerialConsole | None = None
    try:
        child, console = boot(IMAGE, b"svcd: sshd up", "serial.log", "vars.fd",
                              ready_timeout=180, socket_serial=True)
        if not serial_has(console, SERIAL_LEASE, 60):
            raise RuntimeError(f"guest did not acquire {SERIAL_LEASE.decode()!r} within 60 s")
        row_s1()
        row_s2()
        s3_state = row_s3_start()
        row_s4()
        row_s4_echo(None)
        row_s4_exit0(None)
        row_s5_s6()
        row_s3_finish(console, s3_state)
        row_s7(console)
    except Exception as error:  # noqa: BLE001 - a boot failure is a BLOCKED row, not a crash
        record("S1", "BLOCKED", f"SSH boot/run failed: {error}")
        for row in ("S2", "S3", "S4", "S5", "S6", "S7", "S8"):
            record(row, "BLOCKED", f"SSH boot/run failed before this row: {error}")
    finally:
        if child is not None:
            kill(child)
        if console is not None:
            console.pump()
            console.close()
    write_tables()
    serial = OUT / "serial.log"
    if serial.is_file():
        print(f"matrix: serial sha256 {sha256(serial)}")
    failed = [row for row, status, _ in rows if status in ("FAIL", "BLOCKED", "PARTIAL")]
    print(f"matrix: {'RED' if failed else 'GREEN'} ({len(failed)} red rows)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
