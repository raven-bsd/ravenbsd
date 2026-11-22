#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K566 round 2: prove netd is up before sshd starts listening.

Boot the release image, log in on the serial console, and read `/tmp/sshd.log`.
The row is green only when the console shows `svcd: netd up` before
`svcd: sshd up` and sshd's own log says `Server listening on 0.0.0.0 port 22.`
instead of `listen on [0.0.0.0]:22: Permission denied`.

Usage:
  python3 tests/soak/ssh-order.py --image build/qtest/ssh-order.img
"""

from __future__ import annotations

import argparse
import os
import shutil
import sys
from pathlib import Path

import pexpect

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))
import run as soak_run  # noqa: E402  (same-directory runner owns the QEMU argv)

LOGIN = "login:"
PASSWORD = "Password:"
PROMPT = "# "

# The marker is computed by the shell so the typed command echo cannot match it.
COMMAND = "cat /tmp/sshd.log 2>/dev/null; echo DONE-$((6*7))"
MARKER = "DONE-42"


def fail(message: str) -> int:
    print(f"ssh-order: FAIL {message}", flush=True)
    return 1


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", type=Path, required=True)
    parser.add_argument("--out", type=Path, default=None)
    parser.add_argument("--mode", choices=("tcg", "kvm2", "kvm4"), default="tcg")
    parser.add_argument("--timeout", type=float, default=180.0)
    parser.add_argument("--root-password", default=os.environ.get("RAVEN_ROOT_PASSWORD", "raven"))
    args = parser.parse_args(argv)

    image = args.image if args.image.is_absolute() else ROOT / args.image
    if not image.is_file():
        return fail(f"missing image {image}")
    out = args.out or ROOT / "build" / "k566-ssh-order"
    out.mkdir(parents=True, exist_ok=True)
    raw = out / "serial.log"

    vars_source = os.environ.get("RAVEN_OVMF_VARS", soak_run.DEFAULT_OVMF_VARS)
    vars_path = out / ".vars.fd"
    shutil.copyfile(vars_source, vars_path)
    try:
        command = soak_run.qemu_argv("x86_64", soak_run.MODES[args.mode], image, vars_path)
        child = pexpect.spawn(
            command[0],
            command[1:],
            encoding="utf-8",
            codec_errors="replace",
            timeout=args.timeout,
        )
        with raw.open("w", encoding="utf-8") as log:
            child.logfile = log
            try:
                child.expect(LOGIN, timeout=args.timeout)
                child.sendline("root")
                child.expect(PASSWORD, timeout=30)
                child.sendline(args.root_password)
                child.expect(PROMPT, timeout=30)
                child.sendline(COMMAND)
                child.expect(MARKER, timeout=30)
            except (pexpect.EOF, pexpect.TIMEOUT) as error:
                tail = (child.before or "")[-1200:]
                print(tail, file=sys.stderr)
                return fail(f"console wait: {type(error).__name__}")
            finally:
                try:
                    child.close(force=True)
                except Exception:  # noqa: BLE001 - QEMU may ignore TERM on a busy host
                    try:
                        child.kill(9)
                    except Exception:  # noqa: BLE001 - the process is already gone
                        pass
    finally:
        vars_path.unlink(missing_ok=True)

    text = raw.read_text(encoding="utf-8", errors="replace")
    lines = text.splitlines()

    def line_index(needle: str) -> int | None:
        for index, line in enumerate(lines):
            if needle in line:
                return index
        return None

    netd_up = line_index("svcd: netd up")
    sshd_up = line_index("svcd: sshd up")
    denied = "Permission denied" in text or "listen on [0.0.0.0]:22" in text
    listening = "Server listening on 0.0.0.0 port 22." in text
    ordering = netd_up is not None and sshd_up is not None and netd_up < sshd_up
    print(
        "ssh-order: "
        f"netd_up_line={netd_up} sshd_up_line={sshd_up} "
        f"ordering={'ok' if ordering else 'bad'} "
        f"listening={'ok' if listening else 'missing'} "
        f"denied={'yes' if denied else 'no'}",
        flush=True,
    )
    if netd_up is None:
        return fail("no 'svcd: netd up' console line")
    if sshd_up is None:
        return fail("no 'svcd: sshd up' console line")
    if not ordering:
        return fail(f"netd_up_line={netd_up} after sshd_up_line={sshd_up}")
    if denied:
        return fail("sshd logged Permission denied instead of listening")
    if not listening:
        return fail("sshd log has no 'Server listening on 0.0.0.0 port 22.' row")
    print("ssh-order: PASS", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
