#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K435 K6 host peer: connect twice through the QEMU hostfwd to the guest listener, verify the
exact echo and send the acknowledgement that makes the guest print `CONTRACT K6 PASS`."""

import argparse
import socket
import struct
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


def read_line(stream, limit=64):
    data = bytearray()
    while len(data) < limit:
        chunk = stream.recv(1)
        if not chunk:
            raise OSError("eof in greeting")
        if chunk == b"\n":
            return bytes(data)
        data.extend(chunk)
    raise OSError(f"greeting too long {bytes(data)!r}")


def eof_fin(host, port, deadline):
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=2.0) as stream:
                stream.settimeout(5.0)
                greeting = read_line(stream)
                if greeting != b"EOF":
                    raise OSError(f"eof greeting {greeting!r}")
                return True
        except OSError:
            time.sleep(0.2)
    return False


def half_close(host, port, deadline):
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=2.0) as stream:
                stream.settimeout(5.0)
                greeting = read_line(stream)
                if greeting != b"HALF":
                    raise OSError(f"half greeting {greeting!r}")
                stream.sendall(b"DATA")
                stream.shutdown(socket.SHUT_WR)
                ack = b""
                while len(ack) < 7:
                    chunk = stream.recv(7 - len(ack))
                    if not chunk:
                        raise OSError("eof before HALFACK")
                    ack += chunk
                if ack != b"HALFACK":
                    raise OSError(f"half ack {ack!r}")
                # Full close with RST: the peer has closed its read side, so the guest's next
                # write must observe EPIPE and POLLHUP rather than a graceful half-close.
                stream.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER,
                                  struct.pack("ii", 1, 0))
                return True
        except OSError:
            time.sleep(0.2)
    return False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=23197)
    parser.add_argument("--eof-port", type=int, default=23198)
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
    for name, role in (("K8", eof_fin), ("K9", half_close)):
        if not role(args.host, args.eof_port, deadline):
            with open(args.result, "w", encoding="utf-8") as handle:
                handle.write(f"fail {name}\n")
            if args.log:
                with open(args.log, "a", encoding="utf-8") as handle:
                    handle.write(f"{name} host fail\n")
            return 1
        if args.log:
            with open(args.log, "a", encoding="utf-8") as handle:
                handle.write(f"{name} host ok\n")
    with open(args.result, "w", encoding="utf-8") as handle:
        handle.write("ok\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
