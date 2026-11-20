#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K225 host side: connect twice through QEMU hostfwd, read the exact accepted-socket
banner, and acknowledge it. The first connection exercises the fork/exec child, the
second is the parent-write control."""

import argparse
import socket
import sys
import time

EXEC_BANNER = b"SSH-2.0-exec-pass\r\n"
NOEXEC_BANNER = b"SSH-2.0-noexec-pass\r\n"
EXEC_ACK = b"exec-ack"
NOEXEC_ACK = b"noexec-ack"


def log(line, path):
    print(line, flush=True)
    if path:
        try:
            with open(path, "a", encoding="utf-8") as handle:
                handle.write(line + "\n")
        except OSError:
            pass


def write_result(path, text):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(text + "\n")


def recv_exact(stream, wanted, deadline):
    received = b""
    while len(received) < wanted:
        if time.monotonic() >= deadline:
            return None
        chunk = stream.recv(wanted - len(received))
        if not chunk:
            return None
        received += chunk
    return received


def pass_once(host, port, banner, ack, deadline):
    with socket.create_connection((host, port), timeout=2.0) as stream:
        stream.settimeout(60.0)
        received = recv_exact(stream, len(banner), deadline)
        if received != banner:
            return f"banner mismatch got={received!r}"
        stream.sendall(ack)
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=23125)
    parser.add_argument("--result", required=True)
    parser.add_argument("--log", default="")
    parser.add_argument("--timeout", type=float, default=240.0)
    args = parser.parse_args()

    deadline = time.monotonic() + args.timeout
    attempt = 0
    while time.monotonic() < deadline:
        attempt += 1
        try:
            error = pass_once(args.host, args.port, EXEC_BANNER, EXEC_ACK, deadline)
            if error is not None:
                log(f"tcp-accept-exec host: exec {error} attempt={attempt}", args.log)
                time.sleep(0.1)
                continue
            time.sleep(0.2)
            error = pass_once(args.host, args.port, NOEXEC_BANNER, NOEXEC_ACK, deadline)
            if error is not None:
                log(f"tcp-accept-exec host: noexec {error} attempt={attempt}", args.log)
                time.sleep(0.1)
                continue
            log(f"tcp-accept-exec host: both banners ok attempt={attempt}", args.log)
            write_result(args.result, "ok exec+noexec")
            return 0
        except OSError as error:
            if attempt == 1:
                log(f"tcp-accept-exec host: waiting for hostfwd: {error}", args.log)
            time.sleep(0.1)

    log("tcp-accept-exec host: timed out", args.log)
    write_result(args.result, "failed timeout")
    return 1


if __name__ == "__main__":
    sys.exit(main())
