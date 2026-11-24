#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Run one QEMU boot behind a pty and stop as soon as the login prompt arrives.

This is a foreground supervisor: it starts QEMU, reads the serial pty, timestamps lines, and
terminates QEMU at login or timeout. It never leaves a background process behind. The shell
runner owns the matrix, log paths and p50/p90 aggregation.
"""

import argparse
import os
import pty
import select
import subprocess
import sys
import time


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--timeout", type=float, required=True)
    parser.add_argument("--raw", required=True)
    parser.add_argument("--stamped", required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command
    if command and command[0] == "--":
        command = command[1:]
    if not command:
        parser.error("missing QEMU command")

    master, slave = pty.openpty()
    proc = subprocess.Popen(
        command,
        stdin=slave,
        stdout=slave,
        stderr=slave,
        close_fds=True,
    )
    os.close(slave)

    start = time.monotonic()
    login_at = None
    timed_out = False
    buffer = b""
    with open(args.raw, "wb") as raw, open(args.stamped, "w", encoding="utf-8") as stamped:
        while True:
            elapsed = time.monotonic() - start
            remaining = args.timeout - elapsed
            if remaining <= 0:
                timed_out = True
                break
            ready, _, _ = select.select([master], [], [], min(remaining, 0.2))
            if ready:
                try:
                    chunk = os.read(master, 4096)
                except OSError:
                    chunk = b""
                if not chunk:
                    break
                raw.write(chunk)
                raw.flush()
                now = time.monotonic() - start
                buffer += chunk
                while b"\n" in buffer:
                    line, buffer = buffer.split(b"\n", 1)
                    stamped.write(f"{now:.6f} {line.decode(errors='replace')}\n")
                if b"login:" in buffer:
                    stamped.write(f"{now:.6f} {buffer.decode(errors='replace')}\n")
                    login_at = now
                    break
            if proc.poll() is not None and not ready:
                break

        if buffer and login_at is None:
            stamped.write(f"{time.monotonic() - start:.6f} {buffer.decode(errors='replace')}\n")
        stamped.flush()
        raw.flush()

    if proc.poll() is None:
        proc.terminate()
        try:
            proc.wait(timeout=2)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()

    if login_at is None:
        reason = "timeout" if timed_out else "exit-before-login"
        print(f"login-failed reason={reason}", file=sys.stderr)
        return 1
    print(f"login_s={login_at:.3f}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
