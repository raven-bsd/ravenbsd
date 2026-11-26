#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Host side of the K438 SCM_RIGHTS TCP case.

Listen on the host loopback; QEMU user networking exposes it to the guest as 10.0.2.2. The guest
connects, receives the connected socket fd, passes it to a forked child with `SCM_RIGHTS`, and the
child writes the payload through that received fd. The host requires the exact payload and sends
the acknowledgement the child reads back through the same received descriptor.
"""

import argparse
import socket
import sys
import time

PAYLOAD = b"fdpass-tcp-child"
ACK = b"fdpass-tcp-ack"


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
    parser.add_argument("--port", type=int, default=23084)
    parser.add_argument("--result", required=True)
    parser.add_argument("--log", default="")
    parser.add_argument("--timeout", type=float, default=240.0)
    args = parser.parse_args()

    log(f"fdpass-tcp: host listen {args.host}:{args.port}", args.log)
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind((args.host, args.port))
        listener.listen(1)
        listener.settimeout(args.timeout)
        try:
            stream, peer = listener.accept()
        except OSError as error:
            log(f"fdpass-tcp: host accept error={error}", args.log)
            write_result(args.result, f"accept-error {error}")
            return 1
        with stream:
            stream.settimeout(30.0)
            log(f"fdpass-tcp: host accepted peer={peer}", args.log)
            received = recv_exact(stream, len(PAYLOAD), time.monotonic() + 30.0)
            if received != PAYLOAD:
                log(f"fdpass-tcp: host payload mismatch got={received!r}", args.log)
                write_result(args.result, f"mismatch {received!r}")
                return 1
            stream.sendall(ACK)
            log(f"fdpass-tcp: host payload ok bytes={len(PAYLOAD)}", args.log)
            write_result(args.result, f"ok payload={PAYLOAD!r} ack={ACK!r}")
            return 0


if __name__ == "__main__":
    sys.exit(main())
