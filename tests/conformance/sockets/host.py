#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""S174 host peer: drive the guest socket conformance rows."""

from __future__ import annotations

import argparse
import socket
import struct
import sys
import time


def log(message: str) -> None:
    print(f"socket-conformance: {message}", flush=True)


def connect(host: str, port: int) -> socket.socket:
    deadline = time.monotonic() + 10.0
    while time.monotonic() < deadline:
        try:
            return socket.create_connection((host, port), timeout=2.0)
        except OSError:
            time.sleep(0.1)
    raise RuntimeError("guest listener did not appear")


def read_eof(stream: socket.socket) -> bytes:
    data = bytearray()
    while True:
        chunk = stream.recv(64)
        if not chunk:
            return bytes(data)
        data += chunk


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=23100)
    parser.add_argument("--result", required=True)
    parser.add_argument("--log", default="")
    args = parser.parse_args()
    rows: list[str] = []
    try:
        # The one-shot guest service starts before netd finishes the 10.0.2.15 lease;
        # wait for the boot to settle so the first hostfwd connection reaches the listener.
        time.sleep(8.0)
        # Row 1: accept/getpeername. The guest acks after reading the row, so a
        # connection made before the service/listener is ready is retried instead
        # of being mistaken for the next row.
        accepted = False
        for _ in range(300):
            try:
                stream = connect(args.host, args.port)
                stream.settimeout(1.0)
                local = stream.getsockname()[1]
                stream.sendall(struct.pack("!BHH", 1, local, 0))
                ack = stream.recv(1)
                stream.close()
                if ack == b"\x5a":
                    rows.append(f"accept peer_port={local}")
                    accepted = True
                    break
            except OSError:
                pass
            time.sleep(0.1)
        if not accepted:
            raise RuntimeError("guest listener did not ack the accept row")
        # Row 3: the guest shuts down its write side; we send PONG after its FIN.
        with connect(args.host, args.port) as stream:
            stream.settimeout(10.0)
            stream.sendall(b"\x03PING")
            if read_eof(stream) != b"":
                raise RuntimeError("guest FIN was not clean")
            stream.sendall(b"PONG")
            read_eof(stream)
            rows.append("shutdown FIN then PONG")
        # Row 6: half-close; the guest sends HALFACK then expects a peer RST.
        stream = connect(args.host, args.port)
        stream.settimeout(10.0)
        stream.sendall(b"\x06DATA")
        stream.shutdown(socket.SHUT_WR)
        ack = bytearray()
        while len(ack) < 7:
            chunk = stream.recv(7 - len(ack))
            if not chunk:
                break
            ack += chunk
        rows.append(f"half ack={bytes(ack)!r}")
        stream.setsockopt(socket.SOL_SOCKET, socket.SO_LINGER,
                          struct.pack("ii", 1, 0))
        stream.close()
        with open(args.result, "w", encoding="utf-8") as handle:
            handle.write("ok\n")
        if args.log:
            with open(args.log, "a", encoding="utf-8") as handle:
                handle.write("\n".join(rows) + "\n")
        for row in rows:
            log(row)
        return 0
    except Exception as error:  # noqa: BLE001 - diagnostic fixture
        with open(args.result, "w", encoding="utf-8") as handle:
            handle.write(f"fail {error}\n")
        if args.log:
            with open(args.log, "a", encoding="utf-8") as handle:
                handle.write(f"fail {error}\n")
        print(f"socket-conformance: fail {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
