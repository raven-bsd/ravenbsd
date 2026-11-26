#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""S85p 0.1.0 USB keyboard console login gate.

Boot the release profile image under QEMU with `qemu-xhci` and `usb-kbd`, wait
for the brush login prompt on the serial console, and type the login dialogue
only through the QMP monitor (`sendkey`). The gate then asserts the password
prompt, the root shell prompt and a marker command echoed by the guest.

The serial console is output-only here (`-serial file:`), so a pass cannot come
from serial input. A missing prompt, rejected input, failed login or panic names
the exact stage and preserves the serial.

Usage:
  sh tests/boot/0.1.0-usb-keyboard/run.sh --arch x86_64 --accel kvm
  sh tests/boot/0.1.0-usb-keyboard/run.sh --arch x86_64 --accel tcg
  sh tests/boot/0.1.0-usb-keyboard/run.sh --mutation no-keyboard
  sh tests/boot/0.1.0-usb-keyboard/run.sh --mutation wrong-password

Exit codes: 0 pass; 2 setup; 3 image; 10 boot; 11 input; 12 login;
13 marker; 14 panic.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent

SETUP = "setup"
IMAGE = "image"
BOOT = "boot"
INPUT = "input"
LOGIN = "login"
MARKER = "marker"
PANIC = "panic"

STAGES = (IMAGE, BOOT, INPUT, LOGIN, MARKER, PANIC)
MUTATIONS = ("none", "no-keyboard", "wrong-password")

EXIT_STAGE = {
    SETUP: 2,
    IMAGE: 3,
    BOOT: 10,
    INPUT: 11,
    LOGIN: 12,
    MARKER: 13,
    PANIC: 14,
}

LOGIN_PROMPT = r"(?m)^login: ?"
PASSWORD_PROMPT = r"(?i)password: ?"
SHELL_PROMPT = r"raven# "
MARKER_TEXT = "s85p-usb-ok"
MARKER_COMMAND = "echo " + MARKER_TEXT
MARKER_LINE = r"(?m)^" + re.escape(MARKER_TEXT) + r"$"
REJECTED = r"(?i)login incorrect"

PANIC_PATTERN = r"(?i)kernel panic|\bpanic:|EXCEPTION"

IMAGE_TIMEOUT = 300
TOTAL_TIMEOUT = 240
BOOT_TIMEOUT = 120
STEP_TIMEOUT = 20
SHELL_TIMEOUT = 30
QMP_TIMEOUT = 30
KEY_DELAY = 0.05

DEFAULT_OVMF_CODE = "/usr/share/OVMF/OVMF_CODE_4M.fd"
DEFAULT_OVMF_VARS = "/usr/share/OVMF/OVMF_VARS_4M.fd"


class GateFailure(Exception):
    """A gate stage did not pass."""

    def __init__(self, stage: str, detail: str) -> None:
        super().__init__(f"{stage}: {detail}")
        self.stage = stage
        self.detail = detail


def log(message: str) -> None:
    print(f"s85p: {message}", flush=True)


def utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


KEY_NAMES = {
    " ": "spc",
    "-": "minus",
    ".": "dot",
    "/": "slash",
    "_": "shift-minus",
}


def key_name(character: str) -> str:
    """Return the QEMU `sendkey` name for one ASCII character."""
    if len(character) != 1:
        raise ValueError(f"expected one character, got {character!r}")
    if "a" <= character <= "z" or "0" <= character <= "9":
        return character
    if "A" <= character <= "Z":
        return "shift-" + character.lower()
    try:
        return KEY_NAMES[character]
    except KeyError:
        raise ValueError(f"no QEMU key name for {character!r}") from None


def key_sequence(text: str) -> list[str]:
    """Return the QEMU key names for `text` followed by Enter."""
    return [key_name(character) for character in text] + ["ret"]


def sendkey_command(key: str) -> dict:
    """Return the QMP request that injects one key into the guest keyboard."""
    return {
        "execute": "human-monitor-command",
        "arguments": {"command-line": f"sendkey {key}"},
    }


def password_for(password: str, mutation: str) -> str:
    """Return the password the mutation cell types."""
    return "wrong" if mutation == "wrong-password" else password


def artifact_suffix(arch: str, accel: str, mutation: str) -> str:
    """Return the evidence file suffix for one gate cell."""
    if mutation == "none":
        return f"{arch}-{accel}"
    return f"{arch}-{accel}-{mutation}"


def qemu_argv(
    arch: str,
    accel: str,
    image: Path,
    vars_path: Path,
    code_path: Path,
    serial_path: Path,
    qmp_path: Path,
    mutation: str,
) -> list[str]:
    """Return the QEMU argv for one gate cell."""
    argv = [
        f"qemu-system-{arch}",
        "-machine",
        "q35",
        "-m",
        "512M",
        "-smp",
        "2",
        "-serial",
        f"file:{serial_path}",
        "-display",
        "none",
        "-monitor",
        "none",
        "-no-reboot",
        "-drive",
        f"if=pflash,format=raw,readonly=on,file={code_path}",
        "-drive",
        f"if=pflash,format=raw,file={vars_path}",
        "-drive",
        f"format=raw,file={image}",
        "-device",
        "qemu-xhci",
        "-qmp",
        f"unix:{qmp_path},server=on,wait=off",
    ]
    if mutation != "no-keyboard":
        argv.extend(["-device", "usb-kbd"])
    if accel == "kvm":
        argv.extend(["-enable-kvm", "-cpu", "host"])
    else:
        argv.extend(["-accel", "tcg"])
    return argv


def console_stage(text: str) -> str:
    """Return the furthest stage evidenced by a guest serial transcript."""
    if re.search(PANIC_PATTERN, text):
        return PANIC
    if re.search(MARKER_LINE, text):
        return MARKER
    if re.search(SHELL_PROMPT, text):
        return LOGIN
    if re.search(PASSWORD_PROMPT, text):
        return INPUT
    return BOOT


def strip_ansi(data: bytes) -> str:
    """Return serial bytes as text without terminal control sequences."""
    text = data.decode("utf-8", "replace")
    return re.sub(r"\x1b\[[0-9;?]*[A-Za-z]", "", text)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


class Serial:
    """Serial output file reader with stage-tagged waits."""

    def __init__(self, path: Path, total_timeout: int) -> None:
        self.path = path
        self.offset = 0
        self.raw = bytearray()
        self.text = ""
        self.deadline = time.monotonic() + total_timeout
        self.process: subprocess.Popen | None = None

    def _drain(self) -> None:
        with self.path.open("rb") as handle:
            handle.seek(self.offset)
            chunk = handle.read()
            self.offset = handle.tell()
        self.raw.extend(chunk)
        self.text = strip_ansi(bytes(self.raw))

    def mark(self) -> int:
        """Return the current transcript offset for a later `since` check."""
        return len(self.text)

    def expect(
        self,
        pattern: str,
        timeout: int,
        stage: str,
        since: int = 0,
        reject: str | None = None,
        reject_detail: str = "guest rejected the input",
    ) -> None:
        regex = re.compile(pattern)
        reject_regex = re.compile(reject) if reject else None
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            self._drain()
            if regex.search(self.text):
                return
            if reject_regex is not None and reject_regex.search(self.text[since:]):
                raise GateFailure(stage, reject_detail)
            if re.search(PANIC_PATTERN, self.text):
                raise GateFailure(PANIC, "guest panicked")
            if time.monotonic() > self.deadline:
                raise GateFailure(stage, "gate total timeout expired")
            if self.process is not None and self.process.poll() is not None:
                raise GateFailure(
                    stage,
                    f"QEMU exited with {self.process.returncode} "
                    f"before `{pattern}`",
                )
            time.sleep(0.05)
        self._drain()
        raise GateFailure(stage, f"timeout {timeout}s waiting for `{pattern}`")


class Qmp:
    """QMP client that injects keys through QEMU's monitor."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self.sock: socket.socket | None = None
        self.buffer = b""

    def _read_message(self) -> dict:
        assert self.sock is not None
        while b"\n" not in self.buffer:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise OSError("QMP connection closed")
            self.buffer += chunk
        line, self.buffer = self.buffer.split(b"\n", 1)
        return json.loads(line.decode("utf-8"))

    def connect(self) -> None:
        end = time.monotonic() + QMP_TIMEOUT
        while time.monotonic() < end:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            try:
                sock.connect(str(self.path))
            except OSError:
                sock.close()
                time.sleep(0.1)
                continue
            sock.settimeout(5)
            self.sock = sock
            self._read_message()
            self.command({"execute": "qmp_capabilities"})
            return
        raise OSError(f"QMP socket did not appear: {self.path}")

    def command(self, message: dict) -> None:
        assert self.sock is not None
        self.sock.sendall((json.dumps(message) + "\n").encode("utf-8"))
        reply = self._read_message()
        if "error" in reply:
            raise OSError(f"QMP error: {reply['error']}")

    def type_text(self, text: str) -> None:
        for key in key_sequence(text):
            self.command(sendkey_command(key))
            time.sleep(KEY_DELAY)

    def close(self) -> None:
        if self.sock is not None:
            try:
                self.sock.close()
            finally:
                self.sock = None


def run_command(
    argv: list[str], timeout: int, log_path: Path
) -> subprocess.CompletedProcess:
    try:
        result = subprocess.run(
            argv,
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as expired:
        output = (expired.stdout or "") + (expired.stderr or "")
        log_path.write_text(
            f"$ {' '.join(argv)}\n{output}\ntimeout after {timeout}s\n",
            encoding="utf-8",
        )
        raise GateFailure(IMAGE, f"command timed out: {' '.join(argv)}")
    log_path.write_text(
        f"$ {' '.join(argv)}\n{result.stdout}{result.stderr}\nexit={result.returncode}\n",
        encoding="utf-8",
    )
    return result


def git_head() -> str:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stdout.strip()


def prepare_image(evidence: Path, force: bool) -> Path:
    """Build the release image once per commit and reuse it."""
    image = ROOT / "build" / "s85p" / "release.img"
    stamp = image.with_suffix(".stamp")
    cmdline = "console=ttyS0 loglevel=info"
    commit = git_head()
    key = f"{commit}\nrelease\n{cmdline}\n"
    if not force and image.is_file() and stamp.is_file():
        if stamp.read_text(encoding="utf-8") == key:
            log(f"image: reusing {image} (commit {commit[:8]})")
            return image
    image.parent.mkdir(parents=True, exist_ok=True)
    log(f"image: building {image} (commit {commit[:8]})")
    argv = [
        "cargo",
        "xtask",
        "image",
        "--arch",
        "x86_64",
        "--profile",
        "release",
        "--cmdline",
        cmdline,
        "--output",
        str(image.relative_to(ROOT)),
    ]
    try:
        result = run_command(argv, IMAGE_TIMEOUT, evidence / "image-build.log")
    except GateFailure:
        raise
    if result.returncode != 0:
        raise GateFailure(IMAGE, "release image build failed; see image-build.log")
    if not image.is_file():
        raise GateFailure(IMAGE, f"release image missing: {image}")
    stamp.write_text(key, encoding="utf-8")
    return image


def host_record() -> dict:
    try:
        load1 = os.getloadavg()[0]
    except OSError:
        load1 = None
    try:
        qemu = subprocess.run(
            ["qemu-system-x86_64", "--version"],
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        ).stdout.splitlines()[0]
    except (OSError, subprocess.TimeoutExpired, IndexError):
        qemu = "unavailable"
    return {
        "load1": round(load1, 2) if load1 is not None else None,
        "nproc": os.cpu_count(),
        "qemu": qemu,
        "kvm": os.access("/dev/kvm", os.R_OK | os.W_OK),
    }


def write_sums(evidence: Path) -> None:
    lines = []
    for path in sorted(evidence.iterdir()):
        if path.is_file() and path.name != "SHA256SUMS":
            lines.append(f"{sha256_file(path)}  {path.name}")
    (evidence / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def run(args: argparse.Namespace) -> int:
    if args.arch != "x86_64":
        log(f"unsupported arch `{args.arch}` (x86_64 is the gate cell)")
        return 2
    code = Path(os.environ.get("RAVEN_OVMF_CODE", DEFAULT_OVMF_CODE))
    vars_src = Path(os.environ.get("RAVEN_OVMF_VARS", DEFAULT_OVMF_VARS))
    if not code.is_file() or not vars_src.is_file():
        log(f"OVMF firmware missing: {code} / {vars_src}")
        return 2
    if args.accel == "kvm" and not os.access("/dev/kvm", os.R_OK | os.W_OK):
        log("/dev/kvm is not accessible")
        return 2

    evidence = Path(args.evidence_dir)
    evidence.mkdir(parents=True, exist_ok=True)
    suffix = artifact_suffix(args.arch, args.accel, args.mutation)
    serial_path = evidence / f"serial-{suffix}.log"
    summary_path = evidence / f"summary-{suffix}.json"
    qemu_log = evidence / f"qemu-{suffix}.log"
    for path in (serial_path, qemu_log):
        path.unlink(missing_ok=True)
    serial_path.touch()

    work = ROOT / "build" / "s85p"
    work.mkdir(parents=True, exist_ok=True)
    vars_path = work / f"ovmf-vars-{suffix}.fd"
    qmp_path = work / f"qmp-{suffix}.sock"
    qmp_path.unlink(missing_ok=True)
    shutil.copyfile(vars_src, vars_path)

    started = time.monotonic()
    stage = SETUP
    failure: GateFailure | None = None
    serial = Serial(serial_path, TOTAL_TIMEOUT)
    image: Path | None = None
    process = None
    qmp = None
    try:
        image = prepare_image(evidence, args.force_build)
        argv = qemu_argv(
            args.arch,
            args.accel,
            image,
            vars_path,
            code,
            serial_path,
            qmp_path,
            args.mutation,
        )
        log(f"boot: arch={args.arch} accel={args.accel} mutation={args.mutation}")
        with qemu_log.open("w", encoding="utf-8") as qemu_out:
            process = subprocess.Popen(
                argv,
                cwd=ROOT,
                stdin=subprocess.DEVNULL,
                stdout=qemu_out,
                stderr=subprocess.STDOUT,
            )
            serial.process = process

            stage = BOOT
            serial.expect(LOGIN_PROMPT, BOOT_TIMEOUT, BOOT)
            qmp = Qmp(qmp_path)
            try:
                qmp.connect()
            except OSError as error:
                raise GateFailure(INPUT, f"QMP unavailable: {error}") from None

            stage = INPUT
            qmp.type_text("root")
            serial.expect(PASSWORD_PROMPT, STEP_TIMEOUT, INPUT)

            stage = LOGIN
            password_at = serial.mark()
            qmp.type_text(password_for("raven", args.mutation))
            serial.expect(
                SHELL_PROMPT,
                SHELL_TIMEOUT,
                LOGIN,
                since=password_at,
                reject=REJECTED,
                reject_detail="credentials rejected by login",
            )

            stage = MARKER
            qmp.type_text(MARKER_COMMAND)
            serial.expect(MARKER_LINE, STEP_TIMEOUT, MARKER)
    except GateFailure as error:
        failure = error
        stage = error.stage
    finally:
        if qmp is not None:
            qmp.close()
        if process is not None:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
        qmp_path.unlink(missing_ok=True)
        vars_path.unlink(missing_ok=True)

    serial._drain()
    serial_path.write_bytes(serial.raw)

    elapsed = round(time.monotonic() - started, 1)
    serial_stage = console_stage(serial.text)
    qemu_exit = process.returncode if process is not None else None
    record = {
        "ticket": "S85p",
        "arch": args.arch,
        "accel": args.accel,
        "mutation": args.mutation,
        "started_utc": utc_now(),
        "elapsed_seconds": elapsed,
        "commit": git_head(),
        "image": str(image.relative_to(ROOT)) if image is not None else None,
        "image_sha256": sha256_file(image) if image is not None else None,
        "serial": serial_path.name,
        "serial_bytes": serial_path.stat().st_size,
        "serial_sha256": sha256_file(serial_path),
        "serial_stage": serial_stage,
        "failure_stage": failure.stage if failure else None,
        "failure_detail": failure.detail if failure else None,
        "passed": failure is None,
        "marker": MARKER_TEXT,
        "qemu_exit": qemu_exit,
        "host": host_record(),
    }
    summary_path.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
    write_sums(evidence)

    if failure is None:
        log(f"PASS stage={MARKER} elapsed={elapsed}s serial={serial_path.name}")
        return 0
    tail = serial.text.splitlines()[-20:]
    for line in tail:
        print(f"serial: {line}")
    log(
        f"FAIL stage={failure.stage} detail={failure.detail} "
        f"elapsed={elapsed}s serial={serial_path.name}"
    )
    return EXIT_STAGE[failure.stage]


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(prog="0.1.0-usb-keyboard-gate")
    parser.add_argument("--arch", required=True, choices=("x86_64",))
    parser.add_argument("--accel", required=True, choices=("kvm", "tcg"))
    parser.add_argument("--mutation", default="none", choices=MUTATIONS)
    parser.add_argument("--evidence-dir", required=True)
    parser.add_argument("--force-build", action="store_true")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    try:
        return run(args)
    except GateFailure as error:
        log(f"FAIL stage={error.stage} detail={error.detail}")
        return EXIT_STAGE[error.stage]


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
