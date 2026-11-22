#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K449 SSH reconnect soak.

Boots the release gate image once and runs fresh ``ssh raven@... echo <marker>`` sessions, each
bounded, recording the exit code, wall time, exact marker bytes and the client-observed server
lines (Authenticated, exit-status, channel free). A non-zero exit, a hang or a missing marker is a
FAIL with the session number, its raw bytes and the client log; the default stops the batch on the
first failure (``--keep-going`` exists only to expose the failure pattern in a bounded
measurement).

The harness is reusable unchanged for the 100/100 gate:
  python3 tests/ssh/reconnect-soak.py --sessions 100 --batch 10
and optionally adds the TTY rows:
  python3 tests/ssh/reconnect-soak.py --sessions 100 --tty-sessions 10

Evidence is written under build/ssh-reconnect-soak/<UTC>/ (copy it to managed evidence after the
run; the frozen product evidence tree is never touched). No QEMU-free shortcut, no retry.

Usage: python3 tests/ssh/reconnect-soak.py [--sessions N] [--batch N] [--session-timeout S]
                                           [--tty-sessions N] [--keep-going] [--image PATH]
                                           [--net tap|user]
Exit: 0 when every session passes, 1 when a session fails, 2 for setup problems.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import signal
import socket
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
IMAGE = Path(os.environ.get("RAVEN_SSH_IMAGE", ROOT / "build" / "qtest" / "ssh-login.img"))
OVMF_CODE = Path(os.environ.get("RAVEN_OVMF_CODE", "/usr/share/OVMF/OVMF_CODE_4M.fd"))
OVMF_VARS = Path(os.environ.get("RAVEN_OVMF_VARS", "/usr/share/OVMF/OVMF_VARS_4M.fd"))
TAP = os.environ.get("RAVEN_QEMU_TAP", "ravendemo0")
HOST = os.environ.get("RAVEN_SSH_HOST", "10.0.2.15")
USER = os.environ.get("RAVEN_SSH_USER", "raven")
PASSWORD = os.environ.get("RAVEN_SSH_PASSWORD", "raven")
ASKPASS = ROOT / "tests" / "ssh" / "askpass.sh"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_args(argv: list[str]) -> dict:
    options = {
        "sessions": 100,
        "batch": 10,
        "session_timeout": 20.0,
        "tty_sessions": 0,
        "keep_going": False,
        "image": IMAGE,
        "net": "tap",
    }
    index = 0
    while index < len(argv):
        arg = argv[index]
        if arg == "--sessions" and index + 1 < len(argv):
            options["sessions"] = int(argv[index + 1])
            index += 2
        elif arg == "--batch" and index + 1 < len(argv):
            options["batch"] = int(argv[index + 1])
            index += 2
        elif arg == "--session-timeout" and index + 1 < len(argv):
            options["session_timeout"] = float(argv[index + 1])
            index += 2
        elif arg == "--tty-sessions" and index + 1 < len(argv):
            options["tty_sessions"] = int(argv[index + 1])
            index += 2
        elif arg == "--keep-going":
            options["keep_going"] = True
            index += 1
        elif arg == "--image" and index + 1 < len(argv):
            options["image"] = Path(argv[index + 1])
            index += 2
        elif arg == "--net" and index + 1 < len(argv):
            options["net"] = argv[index + 1]
            index += 2
        elif arg in ("-h", "--help"):
            print(__doc__)
            raise SystemExit(0)
        else:
            print(f"reconnect-soak: unknown argument {arg}", file=sys.stderr)
            raise SystemExit(2)
    if options["sessions"] < 1 or options["batch"] < 1:
        print("reconnect-soak: --sessions and --batch must be positive", file=sys.stderr)
        raise SystemExit(2)
    return options


def hygiene() -> str:
    try:
        load = Path("/proc/loadavg").read_text(encoding="ascii").split()[0]
    except OSError:
        load = "unknown"
    count = subprocess.run(
        ["sh", "-c", "pgrep -c qemu-system-x86_64 || true"],
        cwd=ROOT, capture_output=True, text=True, check=False,
    ).stdout.strip() or "0"
    return f"load1={load} qemu={count}"


def ensure_tap() -> None:
    subprocess.run(
        ["sudo", "-n", "sh", "scripts/qemu-net.sh", "down"],
        cwd=ROOT, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    subprocess.run(
        ["sudo", "-n", "sh", "scripts/qemu-net.sh", "up"],
        cwd=ROOT, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )


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


def kill(child: subprocess.Popen) -> None:
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    child.wait()


def boot(run: Path, image: Path, net: str) -> tuple[subprocess.Popen, Path, str, int]:
    host, port = HOST, 22
    if net == "tap":
        ensure_tap()
        netdev = f"tap,id=n0,ifname={TAP},script=no,downscript=no"
    elif net == "user":
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        host = "127.0.0.1"
        netdev = f"user,id=n0,hostfwd=tcp:127.0.0.1:{port}-:22"
    else:
        raise RuntimeError(f"unknown --net {net} (tap or user)")
    boot_dir = run / "boot"
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
        "-drive", f"format=raw,file={image}",
        "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-nic", "none",
        "-netdev", netdev,
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
        raise RuntimeError(f"sshd never started; serial {serial}")
    if not wait_serial(serial, b"netd: interface eth0 10.0.2.15/24", 30):
        kill(child)
        raise RuntimeError("netd never published 10.0.2.15")
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=1):
                return child, serial, host, port
        except OSError:
            time.sleep(0.5)
    kill(child)
    raise RuntimeError("guest TCP 22 never listened")


def ssh_argv(marker: str, tty: bool, host: str, port: int) -> list[str]:
    command = "tty" if tty else f"echo {marker}"
    argv = ["ssh"]
    if tty:
        argv.append("-tt")
    argv += [
        "-v",
        "-p", str(port),
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "KbdInteractiveAuthentication=no",
        "-o", "NumberOfPasswordPrompts=1",
        "-o", "ConnectTimeout=8",
        f"{USER}@{host}",
        command,
    ]
    return argv


def run_session(run: Path, number: int, marker: str, timeout: float, tty: bool,
                host: str, port: int) -> dict:
    log = run / f"session-{number}.ssh.log"
    out_path = run / f"session-{number}.stdout"
    env = dict(os.environ)
    env["SSH_ASKPASS"] = str(ASKPASS)
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["K434_SSH_PASSWORD"] = PASSWORD
    started = time.monotonic()
    with open(log, "wb") as log_file, open(out_path, "wb") as out_file:
        child = subprocess.Popen(
            ssh_argv(marker, tty, host, port), cwd=ROOT, stdin=subprocess.DEVNULL,
            stdout=out_file, stderr=log_file, env=env, start_new_session=True,
        )
        timed_out = False
        try:
            child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()
    elapsed = (time.monotonic() - started) * 1000.0
    text = log.read_text(encoding="utf-8", errors="replace")
    stdout = out_path.read_bytes()
    auth = next((line for line in text.splitlines() if 'Authenticated to ' in line), "")
    exit_line = next(
        (line for line in text.splitlines() if "rtype exit-status" in line), ""
    )
    free_line = next(
        (line for line in text.splitlines() if "channel 0: free:" in line), ""
    )
    expected = (f"/dev/pts/" if tty else marker + "\n").encode()
    marker_ok = (expected in stdout) if tty else (stdout == expected)
    return {
        "session": number,
        "batch": (number - 1) // 10 + 1,
        "exit": None if timed_out else child.returncode,
        "timed_out": timed_out,
        "wall_ms": round(elapsed, 1),
        "marker_expected": expected.decode(errors="replace").rstrip("\n"),
        "marker_ok": marker_ok,
        "stdout_hex": stdout.hex(),
        "auth_line": auth,
        "exit_status_line": exit_line,
        "free_line": free_line,
        "result": (
            "PASS"
            if not timed_out and child.returncode == 0 and marker_ok
            else "FAIL"
        ),
    }


def main(argv: list[str]) -> int:
    options = parse_args(argv)
    image = Path(options["image"])
    if not image.is_file():
        print(f"reconnect-soak: missing image {image}", file=sys.stderr)
        return 2
    if not ASKPASS.is_file():
        print(f"reconnect-soak: missing {ASKPASS}", file=sys.stderr)
        return 2
    run = ROOT / "build" / "ssh-reconnect-soak" / datetime.now(timezone.utc).strftime(
        "%Y%m%dT%H%M%SZ"
    )
    run.mkdir(parents=True, exist_ok=True)
    hygiene_before = hygiene()
    print(f"reconnect-soak: run={run} image_sha256={sha256(image)} commit={git_head()}")
    print(f"reconnect-soak: hygiene before boot: {hygiene_before}")
    rows: list[dict] = []
    failures = 0
    child = None
    try:
        child, serial, host, port = boot(run, image, options["net"])
        print(
            f"reconnect-soak: booted net={options['net']} target={host}:{port}; "
            f"hygiene after boot: {hygiene()}"
        )
        total = options["sessions"]
        for number in range(1, total + 1):
            marker = f"K449-OK-{number}"
            row = run_session(
                run, number, marker, options["session_timeout"], tty=False, host=host, port=port
            )
            rows.append(row)
            print(
                f"session {number}/{total}: exit={row['exit']} timed_out={row['timed_out']} "
                f"wall_ms={row['wall_ms']} marker_ok={row['marker_ok']} "
                f"auth={bool(row['auth_line'])} exit_status={bool(row['exit_status_line'])} "
                f"free={bool(row['free_line'])} {row['result']}"
            )
            if row["result"] != "PASS":
                failures += 1
                if not options["keep_going"]:
                    break
        if options["tty_sessions"] > 0 and failures == 0:
            for number in range(1, options["tty_sessions"] + 1):
                row = run_session(
                    run, 1000 + number, f"K449-TTY-{number}",
                    options["session_timeout"], tty=True, host=host, port=port,
                )
                rows.append(row)
                print(
                    f"tty {number}/{options['tty_sessions']}: exit={row['exit']} "
                    f"timed_out={row['timed_out']} wall_ms={row['wall_ms']} "
                    f"marker_ok={row['marker_ok']} {row['result']}"
                )
                if row["result"] != "PASS":
                    failures += 1
                    if not options["keep_going"]:
                        break
    finally:
        if child is not None:
            kill(child)
        if options["net"] == "tap":
            subprocess.run(
                ["sudo", "-n", "sh", "scripts/qemu-net.sh", "down"],
                cwd=ROOT, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
    columns = [
        "session", "batch", "exit", "timed_out", "wall_ms", "marker_expected",
        "marker_ok", "stdout_hex", "auth_line", "exit_status_line", "free_line",
        "result",
    ]
    with open(run / "sessions.tsv", "w", encoding="utf-8") as tsv:
        tsv.write("\t".join(columns) + "\n")
        for row in rows:
            tsv.write("\t".join(str(row[column]) for column in columns) + "\n")
    summary = {
        "sessions": len([row for row in rows if row["session"] < 1000]),
        "tty_sessions": len([row for row in rows if row["session"] >= 1000]),
        "failed": failures,
        "hangs": len([row for row in rows if row["timed_out"]]),
        "image": str(image),
        "image_sha256": sha256(image),
        "commit": git_head(),
        "hygiene_before": hygiene_before,
        "hygiene_after": hygiene(),
        "session_timeout_s": options["session_timeout"],
        "net": options["net"],
        "keep_going": options["keep_going"],
    }
    (run / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    with open(run / "SHA256SUMS", "w", encoding="utf-8") as sums:
        for path in sorted(run.rglob("*")):
            if path.is_file() and path.name != "SHA256SUMS":
                sums.write(f"{sha256(path)}  {path.relative_to(run)}\n")
    print(f"reconnect-soak: done sessions={summary['sessions']} failures={failures} run={run}")
    return 0 if failures == 0 else 1


def git_head() -> str:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True, check=False
    )
    return result.stdout.strip()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
