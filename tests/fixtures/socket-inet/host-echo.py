#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Host side of the K230 AF_INET echo.

Connect through QEMU hostfwd to the guest listener, send the payload, require the exact echo,
then send the acknowledgement. The guest prints `socket-inet: host ack` only after it reads the
acknowledgement, so the serial transcript proves the whole host-to-guest path.
"""

import argparse
import socket
import sys
import time

PAYLOAD = b"socket-inet-payload"
ACK = b"socket-inet-ack"


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
    parser.add_argument("--port", type=int, default=23080)
    parser.add_argument("--result", required=True)
    parser.add_argument("--log", default="")
    parser.add_argument("--timeout", type=float, default=240.0)
    args = parser.parse_args()

    deadline = time.monotonic() + args.timeout
    attempt = 0
    while time.monotonic() < deadline:
        attempt += 1
        try:
            with socket.create_connection((args.host, args.port), timeout=2.0) as stream:
                stream.settimeout(5.0)
                stream.sendall(PAYLOAD)
                received = recv_exact(stream, len(PAYLOAD), time.monotonic() + 30.0)
                if received != PAYLOAD:
                    log(
                        f"socket-inet: host echo mismatch attempt={attempt} got={received!r}",
                        args.log,
                    )
                    continue
                stream.sendall(ACK)
                log(
                    f"socket-inet: host echo ok attempt={attempt} bytes={len(PAYLOAD)}",
                    args.log,
                )
                write_result(args.result, f"ok payload={PAYLOAD!r} ack={ACK!r}")
                return 0
        except OSError as error:
            if attempt % 20 == 0:
                log(f"socket-inet: host retry attempt={attempt} error={error}", args.log)
            time.sleep(0.5)

    log(f"socket-inet: host timeout after {attempt} attempts", args.log)
    write_result(args.result, "timeout")
    return 1


if __name__ == "__main__":
    sys.exit(main())
