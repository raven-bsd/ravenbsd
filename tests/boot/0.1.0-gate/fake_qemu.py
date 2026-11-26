#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Stand-in QEMU for the boot-gate mutation check.

Binds the Unix serial socket passed by measure.py and writes one crafted serial stream. It
never boots anything. `run.sh --mutation` uses it to prove the gate rejects a fake login marker
and a forged timestamp instead of certifying itself.
"""

import argparse
import os
import socket
import sys
import time


def parse_args(argv):
    parser = argparse.ArgumentParser()
    parser.add_argument("--scenario", required=True, choices=("marker-only", "fixed-timestamp"))
    parser.add_argument("--serial", required=True, help="QEMU serial value from @SERIAL@")
    return parser.parse_args(argv)


def main(argv):
    args = parse_args(argv)
    if not args.serial.startswith("unix:"):
        print(f"fake-qemu: unexpected serial backend: {args.serial}", file=sys.stderr)
        return 2
    path = args.serial[len("unix:") :].split(",", 1)[0]
    try:
        os.unlink(path)
    except FileNotFoundError:
        pass

    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(path)
    server.listen(1)
    connection, _ = server.accept()
    try:
        if args.scenario == "marker-only":
            connection.sendall(b"Welcome to RavenBSD\nlogin: ")
        else:
            connection.sendall(b"loader: RavenBSD loader 0.1 (x86_64)\n")
            connection.sendall(b"RavenBSD kernel 0.1 (x86_64) deadbeef 2026-11-26\n")
            connection.sendall(b"boot_ms=1\n")
            time.sleep(1.1)
            connection.sendall(b"login: ")
        time.sleep(0.2)
    finally:
        connection.close()
        server.close()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
