#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Host side of the B70 TCP echo: connect to QEMU's hostfwd and require the exact payload."""

import argparse
import socket
import sys
import time

PAYLOAD = b"b70-host-echo-payload"


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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=23070)
    parser.add_argument("--payload", default=PAYLOAD.decode())
    parser.add_argument("--result", required=True)
    parser.add_argument("--log", default="")
    parser.add_argument("--timeout", type=float, default=240.0)
    args = parser.parse_args()

    payload = args.payload.encode()
    deadline = time.monotonic() + args.timeout
    attempt = 0
    while time.monotonic() < deadline:
        attempt += 1
        try:
            with socket.create_connection((args.host, args.port), timeout=2.0) as stream:
                stream.settimeout(5.0)
                stream.sendall(payload)
                received = b""
                while len(received) < len(payload):
                    chunk = stream.recv(len(payload) - len(received))
                    if not chunk:
                        break
                    received += chunk
                if received == payload:
                    log(f"b70: host echo ok attempt={attempt} bytes={len(payload)}", args.log)
                    write_result(args.result, f"ok payload={args.payload}")
                    return 0
                log(
                    f"b70: host echo mismatch attempt={attempt} got={received!r}",
                    args.log,
                )
        except OSError as error:
            if attempt == 1:
                log(f"b70: host echo waiting for hostfwd: {error}", args.log)
            time.sleep(0.1)

    log("b70: host echo timed out", args.log)
    write_result(args.result, "failed timeout")
    return 1


if __name__ == "__main__":
    sys.exit(main())
