#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K431 S5 host peer: one accepted connection, exact ordered bytes on the same socket.

The guest accepts, duplicates the socket onto fd 7/1/2, fork+execs the child, and writes
ID7-CHAN / ID1-STDOUT / ID2-STDERR. This peer requires those exact bytes in order on the same
TCP connection, sends PING, requires ID7-PONG, then requires the parent's ID-PARENT and sends
DONE. A sentinel that reached the guest console instead of this socket is a failure.
"""

import argparse
import os
import socket
import sys
import time

PORT = 23097
CONNECT_SECONDS = 120
IO_SECONDS = 30


def log(path, text):
    print(text, flush=True)
    if path:
        with open(path, "a", encoding="utf-8") as handle:
            handle.write(text + "\n")


def write_result(path, text):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(text + "\n")


def read_exact(connection, length):
    data = bytearray()
    while len(data) < length:
        chunk = connection.recv(length - len(data))
        if not chunk:
            raise OSError(f"eof after {len(data)} of {length}")
        data.extend(chunk)
    return bytes(data)


def read_line(connection, limit=64):
    data = bytearray()
    while len(data) < limit:
        chunk = connection.recv(1)
        if not chunk:
            raise OSError(f"eof in line after {len(data)} bytes")
        if chunk == b"\n":
            return bytes(data)
        data.extend(chunk)
    raise OSError(f"line too long: {bytes(data)!r}")


def verify(connection, log_path, result_path):
    connection.settimeout(IO_SECONDS)
    for name, expected in [("shell-stdout", b"ID1-STDOUT"), ("shell-stderr", b"ID2-STDERR")]:
        got = read_line(connection)
        log(log_path, f"{name}={got!r}")
        if got != expected:
            raise OSError(f"{name} mismatch {got!r}")
    got = read_exact(connection, 8)
    log(log_path, f"fd7-chan={got!r}")
    if got != b"ID7-CHAN":
        raise OSError(f"chan mismatch {got!r}")
    connection.sendall(b"PING")
    got = read_exact(connection, 8)
    log(log_path, f"fd7-pong={got!r}")
    if got != b"ID7-PONG":
        raise OSError(f"pong mismatch {got!r}")
    got = read_exact(connection, 9)
    log(log_path, f"parent={got!r}")
    if got != b"ID-PARENT":
        raise OSError(f"parent mismatch {got!r}")
    connection.sendall(b"DONE")
    write_result(result_path, "0")
    log(log_path, "verified S5 identity and stdio routing")
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default=os.environ.get("SOCKET_WRITE_IDENTITY_HOST", "10.43.0.15"))
    parser.add_argument("--port", type=int, default=PORT)
    parser.add_argument("--mode", default="connect")
    parser.add_argument("--log", default="build/fixtures/socket-write-identity/host.log")
    parser.add_argument("--result", default="build/fixtures/socket-write-identity/host.result")
    args = parser.parse_args()
    log_path = args.log
    result_path = args.result
    open(log_path, "w", encoding="utf-8").close()
    write_result(result_path, "1")

    if args.mode != "connect":
        log(log_path, f"unsupported mode {args.mode}")
        return 1

    deadline = time.monotonic() + CONNECT_SECONDS
    connection = None
    while time.monotonic() < deadline:
        try:
            connection = socket.create_connection((args.host, args.port), timeout=3)
            break
        except OSError as error:
            log(log_path, f"connect retry: {error}")
            time.sleep(0.2)
    if connection is None:
        log(log_path, "connect timeout")
        return 1

    log(log_path, f"connected {args.host}:{args.port}")
    try:
        return verify(connection, log_path, result_path)
    except OSError as error:
        log(log_path, f"failed: {error}")
        return 1
    finally:
        connection.close()


if __name__ == "__main__":
    sys.exit(main())
