#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Boot-gate measurement core.

Spawns one QEMU process, reads its serial stream as bytes from a Unix socket, and measures
host wall time from immediately before the spawn (QEMU start, firmware included) to the first
`login:` bytes. The guest cannot influence the clock: only the host monotonic timer is used.

The QEMU argv is passed after `--`; the literal `@SERIAL@` is replaced with a Unix-socket serial
backend. The full serial stream is written to --log so a failed run keeps its evidence.

Exit codes: 0 valid run under the budget; 1 invalid or over budget; 2 usage/infrastructure.
"""

import argparse
import os
import re
import socket
import subprocess
import sys
import time

MARKER = b"login:"
KERNEL_BANNER = b"RavenBSD kernel 0.1 ("
KERNEL_ID = re.compile(rb"RavenBSD kernel 0\.1 \([^)]*\) ([0-9A-Za-z_-]+) ")
BUDGET_MS = 1000
FORBIDDEN = re.compile(
    rb"(?i)panic|EXCEPTION|SegmentOutsideUserRange|BadElf|Traceback"
)
CONNECT_SECS = 5.0


def fail(message):
    print(f"boot-gate: FAIL: {message}", file=sys.stderr)
    return 1


def parse_args(argv):
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--log", required=True, help="serial transcript path")
    parser.add_argument("--timeout", type=float, default=90.0, help="per-run wall-clock cap")
    parser.add_argument("qemu", nargs=argparse.REMAINDER, help="QEMU argv after --")
    args = parser.parse_args(argv)
    if args.qemu and args.qemu[0] == "--":
        args.qemu = args.qemu[1:]
    if not args.qemu:
        parser.error("missing QEMU argv after --")
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    return args


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def main(argv):
    args = parse_args(argv)
    log_dir = os.path.dirname(os.path.abspath(args.log))
    os.makedirs(log_dir, exist_ok=True)
    sock_path = args.log + ".sock"
    try:
        os.unlink(sock_path)
    except FileNotFoundError:
        pass
    argv = [
        value.replace("@SERIAL@", f"unix:{sock_path},server=on,wait=off")
        for value in args.qemu
    ]

    data = bytearray()
    qemu_stderr = open(args.log + ".qemu", "wb")
    start = time.monotonic_ns()
    try:
        process = subprocess.Popen(
            argv,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=qemu_stderr,
        )
    except OSError as error:
        qemu_stderr.close()
        return fail(f"cannot start QEMU: {error}")

    sock = None
    connect_deadline = time.monotonic() + CONNECT_SECS
    while time.monotonic() < connect_deadline:
        try:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.connect(sock_path)
            break
        except OSError:
            sock = None
            if process.poll() is not None:
                break
            time.sleep(0.005)
    log = open(args.log, "wb")
    if sock is None:
        stop(process)
        log.close()
        qemu_stderr.close()
        return fail("cannot connect to the QEMU serial socket (see .qemu log)")

    kernel_ns = None
    login_ns = None
    reason = "timeout before the login marker"
    sock.settimeout(0.02)
    deadline = start + int(args.timeout * 1_000_000_000)
    try:
        while True:
            now = time.monotonic_ns()
            if now >= deadline:
                break
            try:
                chunk = sock.recv(65536)
            except socket.timeout:
                if process.poll() is not None:
                    reason = "QEMU exited before the login marker"
                    break
                continue
            except OSError:
                reason = "QEMU serial closed before the login marker"
                break
            if not chunk:
                reason = "QEMU serial closed before the login marker"
                break
            log.write(chunk)
            log.flush()
            data.extend(chunk)
            if kernel_ns is None and KERNEL_BANNER in data:
                kernel_ns = time.monotonic_ns()
            if MARKER in data:
                login_ns = time.monotonic_ns()
                break
    finally:
        try:
            sock.close()
        except OSError:
            pass
        stop(process)
        log.close()
        qemu_stderr.close()
        try:
            os.unlink(sock_path)
        except FileNotFoundError:
            pass

    if kernel_ns is None:
        return fail("no RavenBSD kernel banner before the login marker")
    if login_ns is None:
        return fail(reason)
    forbidden = FORBIDDEN.search(bytes(data))
    if forbidden is not None:
        return fail(f"forbidden serial output: {forbidden.group(0).decode('ascii', 'replace')}")

    boot_ms = (login_ns - start) // 1_000_000
    kernel_ms = (kernel_ns - start) // 1_000_000
    match = KERNEL_ID.search(bytes(data))
    if match is None:
        return fail("kernel banner carries no build id")
    print(f"boot_ms={boot_ms}")
    print(f"kernel_ms={kernel_ms}")
    print(f"kernel_id={match.group(1).decode('ascii')}")
    if boot_ms >= BUDGET_MS:
        return fail(f"boot {boot_ms} ms >= {BUDGET_MS} ms")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
