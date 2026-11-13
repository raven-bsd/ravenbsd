#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Host side of the B70 TCP echo: connect through QEMU's hostfwd, require the
exact payload, echo it back, then send the guest's acknowledgement.

The guest sends the payload back only after it has printed the socket-table
row for the established connection. The guest then reads the ack, which proves
the host received the exact echo without a second guest boot.
"""

import argparse
import socket
import sys
import time

PAYLOAD = b"b70-host-echo-payload"
ACK = b"b70-host-ack"


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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=23070)
    parser.add_argument("--payload", default=PAYLOAD.decode())
    parser.add_argument("--ack", default=ACK.decode())
    parser.add_argument("--result", required=True)
    parser.add_argument("--log", default="")
    parser.add_argument("--timeout", type=float, default=240.0)
    args = parser.parse_args()

    payload = args.payload.encode()
    ack = args.ack.encode()
    deadline = time.monotonic() + args.timeout
    attempt = 0
    while time.monotonic() < deadline:
        attempt += 1
        try:
            with socket.create_connection((args.host, args.port), timeout=2.0) as stream:
                stream.settimeout(5.0)
                stream.sendall(payload)
                received = recv_exact(stream, len(payload), time.monotonic() + 30.0)
                if received != payload:
                    log(
                        f"b70: host echo mismatch attempt={attempt} got={received!r}",
                        args.log,
                    )
                    continue
                stream.sendall(ack)
                log(f"b70: host echo ok attempt={attempt} bytes={len(payload)}", args.log)
                write_result(
                    args.result,
                    f"ok payload={args.payload} ack={args.ack}",
                )
                return 0
        except OSError as error:
            if attempt == 1:
                log(f"b70: host echo waiting for hostfwd: {error}", args.log)
            time.sleep(0.1)

    log("b70: host echo timed out", args.log)
    write_result(args.result, "failed timeout")
    return 1


if __name__ == "__main__":
    sys.exit(main())
