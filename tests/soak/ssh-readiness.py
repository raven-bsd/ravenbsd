#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K566: sshd must be ready before svcd reports it up.

Boot the release image with a QEMU user-net host forward to guest port 22. As soon
as the console prints `svcd: sshd up`, connect to the forwarded port and require
the SSH banner within two seconds. A `readiness = "none"` image prints the line at
spawn, so the first connection can race the listener bind and time out.

Usage:
  python3 tests/soak/ssh-readiness.py --image build/qtest/sshd-ready.img --boots 10
"""

from __future__ import annotations

import argparse
import os
import shutil
import socket
import sys
import time
from pathlib import Path

import pexpect

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))
import run as soak_run  # noqa: E402  (same-directory runner owns the QEMU argv)

BANNER = b"SSH-2.0-OpenSSH"
BUDGET_S = 2.0


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.bind(("127.0.0.1", 0))
        return int(probe.getsockname()[1])


def boot_once(image: Path, mode: str, timeout: float, raw: Path) -> tuple[bool, str]:
    port = free_port()
    vars_source = os.environ.get("RAVEN_OVMF_VARS", soak_run.DEFAULT_OVMF_VARS)
    vars_path = raw.with_suffix(".vars.fd")
    shutil.copyfile(vars_source, vars_path)
    command = soak_run.qemu_argv("x86_64", soak_run.MODES[mode], image, vars_path)
    command = [
        f"user,id=n0,hostfwd=tcp:127.0.0.1:{port}-:22" if item == "user,id=n0" else item
        for item in command
    ]
    child = pexpect.spawn(
        command[0],
        command[1:],
        encoding="utf-8",
        codec_errors="replace",
        timeout=timeout,
    )
    banner = b""
    elapsed = 0.0
    line = ""
    try:
        with raw.open("w", encoding="utf-8") as log:
            child.logfile = log
            try:
                child.expect("svcd: sshd up", timeout=timeout)
                started = time.monotonic()
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=BUDGET_S) as conn:
                        conn.settimeout(BUDGET_S)
                        banner = conn.recv(256)
                except OSError as error:
                    line = type(error).__name__
                elapsed = time.monotonic() - started
            except (pexpect.EOF, pexpect.TIMEOUT) as error:
                line = type(error).__name__
    finally:
        try:
            child.close(force=True)
        except Exception:  # noqa: BLE001 - QEMU may ignore TERM on a busy host
            try:
                child.kill(9)
            except Exception:  # noqa: BLE001 - the process is already gone
                pass
        vars_path.unlink(missing_ok=True)
    ok = banner.startswith(BANNER) and elapsed <= BUDGET_S
    detail = (
        f"banner={'ok' if banner.startswith(BANNER) else 'missing'} "
        f"elapsed_ms={int(elapsed * 1000)} "
        f"status={'ok' if ok else 'fail'}"
    )
    if line:
        detail += f" error={line}"
    return ok, detail


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path, required=True)
    parser.add_argument("--boots", type=int, default=4)
    parser.add_argument("--mode", choices=("tcg", "kvm2", "kvm4"), default="tcg")
    parser.add_argument("--timeout", type=float, default=120.0)
    parser.add_argument("--out", type=Path, default=ROOT / "build" / "k566-ssh-readiness")
    args = parser.parse_args(argv)
    if args.boots < 1:
        parser.error("--boots must be at least 1")
    image = args.image if args.image.is_absolute() else ROOT / args.image
    if not image.is_file():
        parser.error(f"image not found: {image}")
    args.out.mkdir(parents=True, exist_ok=True)
    passed = 0
    failed = 0
    for boot in range(1, args.boots + 1):
        raw = args.out / f"boot-{boot:02d}.log"
        ok, detail = boot_once(image, args.mode, args.timeout, raw)
        if ok:
            passed += 1
        else:
            failed += 1
        print(f"ssh-readiness: boot={boot} {detail}", flush=True)
    print(f"ssh-readiness: boots={args.boots} pass={passed} fail={failed}", flush=True)
    if failed:
        print("ssh-readiness: FAIL", flush=True)
        return 1
    print("ssh-readiness: PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
