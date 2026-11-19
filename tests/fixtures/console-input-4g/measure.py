#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""Isolate the 4 GiB console-input NUL bytes.

Boots the S157 `smp-4g` release image with a chosen RAM size and shell, logs in,
sends controlled lines with a chosen spacing, captures the raw serial bytes and
records whether the line editor received the bytes or a same-length run of NULs.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import signal
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
DEFAULT_IMAGE = ROOT / "build" / "qtest" / "smp-4g.img"
DEFAULT_OVMF_CODE = "/usr/share/OVMF/OVMF_CODE_4M.fd"
DEFAULT_OVMF_VARS = "/usr/share/OVMF/OVMF_VARS_4M.fd"

COMMANDS = [
    ("echo hi", rb"^hi\s*$"),
    ("pwd", rb"^/\S*\s*$"),
    ("uname -a", rb"^RavenBSD raven 0\.1 .* x86_64\s*$"),
]


class HarnessError(Exception):
    """A configuration or host problem that stops the run."""


def utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def utc_stamp() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git_commit() -> str:
    import subprocess

    result = subprocess.run(
        ["git", "rev-parse", "--short=8", "HEAD"],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    )
    return result.stdout.strip()


def load1() -> float:
    return round(os.getloadavg()[0], 2)


def size_mib(size: str) -> int:
    return {"512M": 512, "4G": 4096}[size]


def qemu_argv(size: str, accel: str, image: Path, vars_path: Path) -> list[str]:
    code = Path(os.environ.get("RAVEN_OVMF_CODE", DEFAULT_OVMF_CODE))
    if not code.is_file():
        raise HarnessError(f"OVMF code not found: {code}")
    argv = [
        "qemu-system-x86_64",
        "-machine",
        "q35",
        "-m",
        str(size_mib(size)),
        "-smp",
        "4",
        "-serial",
        "stdio",
        "-display",
        "none",
        "-monitor",
        "none",
        "-no-reboot",
        "-snapshot",
        "-drive",
        f"if=pflash,format=raw,readonly=on,file={code}",
        "-drive",
        f"if=pflash,format=raw,file={vars_path}",
        "-drive",
        f"format=raw,file={image}",
        "-device",
        "isa-debug-exit,iobase=0xf4,iosize=0x04",
    ]
    if accel == "kvm":
        argv.extend(["-enable-kvm", "-cpu", "host"])
    else:
        argv.extend(["-accel", "tcg"])
    return argv


def kill_child(child) -> None:
    try:
        if child.isalive():
            child.kill(signal.SIGKILL)
    except Exception:
        pass
    try:
        child.close(force=True)
    except Exception:
        pass


def hexdump(data: bytes, width: int = 16) -> str:
    lines = []
    for offset in range(0, len(data), width):
        chunk = data[offset : offset + width]
        hexed = " ".join(f"{byte:02x}" for byte in chunk)
        text = "".join(chr(byte) if 32 <= byte < 127 else "." for byte in chunk)
        lines.append(f"{offset:06x}  {hexed:<{width * 3}}  |{text}|")
    return "\n".join(lines)


def response_after_echo(raw: bytes, command: str) -> bytes:
    needle = command.encode() + b"\r\n"
    index = raw.rfind(needle)
    if index < 0:
        needle = command.encode() + b"\n"
        index = raw.rfind(needle)
    if index < 0:
        return raw
    start = index + len(needle)
    end = raw.find(b"raven# ", start)
    return raw[start:] if end < 0 else raw[start:end]


def classify(command: str, pattern: bytes | None, response: bytes) -> dict:
    import re

    nul_count = response.count(b"\x00")
    text = response.decode("utf-8", "replace")
    found = pattern is None or re.search(pattern.decode(), text, re.MULTILINE) is not None
    if nul_count:
        state = "nul"
    elif b"command not found" in response:
        state = "not-found"
    elif found:
        state = "clean"
    else:
        state = "unexpected"
    return {
        "command": command,
        "state": state,
        "nul_count": nul_count,
        "expected_match": found,
        "response": text,
        "response_hex": hexdump(response),
    }


def run(args) -> int:
    try:
        import pexpect
    except ImportError as error:  # pragma: no cover - host dependency
        raise HarnessError("python3 pexpect is required") from error

    image = args.image if args.image.is_absolute() else ROOT / args.image
    if not image.is_file():
        raise HarnessError(f"smp-4g image not found (read-only input): {image}")
    vars_source = Path(os.environ.get("RAVEN_OVMF_VARS", DEFAULT_OVMF_VARS))
    if not vars_source.is_file():
        raise HarnessError(f"OVMF vars not found: {vars_source}")

    stamp = utc_stamp()
    run_dir = args.out / f"{stamp}-{args.size}-{args.shell}-{args.spacing}ms-{args.accel}"
    run_dir.mkdir(parents=True, exist_ok=True)
    vars_path = run_dir / "vars.fd"
    shutil.copyfile(vars_source, vars_path)
    log_path = run_dir / "serial.log"
    hex_path = run_dir / "responses.hex"
    result_path = run_dir / "result.json"

    argv = qemu_argv(args.size, args.accel, image, vars_path)
    commit = git_commit()
    image_hash = sha256_file(image)
    log_file = log_path.open("wb")
    child = None
    result = {
        "generated_utc": utc_now(),
        "size": args.size,
        "shell": args.shell,
        "spacing_ms": args.spacing,
        "accel": args.accel,
        "commit": commit,
        "image": str(image),
        "image_sha256": image_hash,
        "load1": load1(),
        "host_nproc": os.cpu_count(),
        "boot": "unknown",
        "rsh_start": None,
        "commands": [],
        "serial_log": str(log_path),
    }
    try:
        child = pexpect.spawn(
            argv[0],
            argv[1:],
            cwd=str(ROOT),
            encoding=None,
            echo=False,
            timeout=args.step_timeout,
            logfile=log_file,
        )
        child.expect(rb"(?i)login:\s*", timeout=args.boot_timeout)
        child.sendline(b"root")
        child.expect(rb"(?i)password:\s*", timeout=args.step_timeout)
        child.sendline(b"raven")
        child.expect(rb"raven#\s", timeout=args.boot_timeout)
        result["boot"] = "login"
        result["load1_login"] = load1()

        if args.shell == "rsh":
            time.sleep(args.spacing / 1000.0)
            child.sendline(b"rsh")
            try:
                child.expect(rb"raven#\s", timeout=args.step_timeout)
                result["rsh_start"] = "ok"
            except Exception:
                result["rsh_start"] = "failed"
                result["rsh_start_hex"] = hexdump(child.before or b"")

        probes = list(COMMANDS) + [(command, None) for command in args.extra_command]
        for command, pattern in probes:
            time.sleep(args.spacing / 1000.0)
            child.sendline(command.encode())
            try:
                child.expect(rb"raven#\s", timeout=args.step_timeout)
                response = response_after_echo(child.before or b"", command)
            except Exception as error:
                response = (child.before or b"") + f"\n<{error}>\n".encode()
            result["commands"].append(classify(command, pattern, response))

        time.sleep(args.spacing / 1000.0)
        child.sendline(b"exit 0")
        try:
            child.expect(rb"init exited \(0\)", timeout=15)
            result["exit"] = "clean"
        except Exception:
            result["exit"] = "killed"
    finally:
        if child is not None:
            kill_child(child)
        log_file.flush()
        log_file.close()
        vars_path.unlink(missing_ok=True)

    states = [entry["state"] for entry in result["commands"]]
    result["result"] = (
        "clean" if states and all(state == "clean" for state in states) else "nul-or-fail"
    )
    hex_path.write_text(
        "\n\n".join(
            f"# {entry['command']}\n{entry['response_hex']}"
            for entry in result["commands"]
        )
        + "\n",
        encoding="utf-8",
    )
    result_path.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")

    print(
        "size\tshell\tspacing\taccel\tcommit\tload1\tcommand\tstate\tnuls\tresponse",
        flush=True,
    )
    for entry in result["commands"]:
        response = " ".join(entry["response"].split())[:100]
        print(
            f"{args.size}\t{args.shell}\t{args.spacing}ms\t{args.accel}\t{commit}\t"
            f"{result['load1']}\t{entry['command']}\t{entry['state']}\t"
            f"{entry['nul_count']}\t{response}",
            flush=True,
        )
    print(f"boot: {result['boot']} rsh_start: {result['rsh_start']}", flush=True)
    print(f"raw serial: {log_path}", flush=True)
    print(f"response hex: {hex_path}", flush=True)
    print(f"result: {result['result']}", flush=True)
    return 0 if result["result"] == "clean" else 1


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--size", choices=("512M", "4G"), default="4G")
    parser.add_argument("--shell", choices=("brush", "rsh"), default="brush")
    parser.add_argument("--spacing", type=int, choices=(100, 1000), default=100)
    parser.add_argument("--accel", choices=("tcg", "kvm"), default="tcg")
    parser.add_argument("--image", type=Path, default=DEFAULT_IMAGE)
    parser.add_argument(
        "--extra-command",
        action="append",
        default=[],
        help="additional controlled line (no expected marker; NUL/not-found still fails)",
    )
    parser.add_argument("--boot-timeout", type=float, default=90.0)
    parser.add_argument("--step-timeout", type=float, default=15.0)
    parser.add_argument("--out", type=Path, default=ROOT / "build" / "console-input-4g")
    args = parser.parse_args(argv)
    try:
        return run(args)
    except HarnessError as error:
        print(f"console-input-4g: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
