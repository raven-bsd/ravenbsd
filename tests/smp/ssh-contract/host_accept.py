#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K435 K6 host peer: connect twice through the QEMU hostfwd to the guest listener, verify the
exact echo and send the acknowledgement that makes the guest print `CONTRACT K6 PASS`."""

import argparse
import socket
import sys
import time


def exchange(host, port, payload, ack, deadline):
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=2.0) as stream:
                stream.settimeout(5.0)
                stream.sendall(payload)
                echoed = b""
                while len(echoed) < len(payload):
                    chunk = stream.recv(len(payload) - len(echoed))
                    if not chunk:
                        raise OSError("eof before echo")
                    echoed += chunk
                if echoed != payload:
                    raise OSError(f"echo mismatch {echoed!r}")
                stream.sendall(ack)
                return True
        except OSError:
            time.sleep(0.2)
    return False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=23097)
    parser.add_argument("--result", required=True)
    parser.add_argument("--log", default="")
    parser.add_argument("--timeout", type=float, default=120.0)
    args = parser.parse_args()

    deadline = time.monotonic() + args.timeout
    steps = [(b"ACCEPT1", b"K6ACK1"), (b"ACCEPT2", b"K6ACK2")]
    for payload, ack in steps:
        if not exchange(args.host, args.port, payload, ack, deadline):
            with open(args.result, "w", encoding="utf-8") as handle:
                handle.write(f"fail {payload!r}\n")
            if args.log:
                with open(args.log, "a", encoding="utf-8") as handle:
                    handle.write(f"K6 host fail {payload!r}\n")
            return 1
        if args.log:
            with open(args.log, "a", encoding="utf-8") as handle:
                handle.write(f"K6 host ok {payload!r}\n")
    with open(args.result, "w", encoding="utf-8") as handle:
        handle.write("ok\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
