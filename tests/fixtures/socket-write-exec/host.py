#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K408 host peer: listen for the guest connection and verify the five write variants in order.
# The READ/ACK2/DONE triggers are sent only after the earlier markers arrive, so a missing
# variant leaves the guest blocked at the corresponding serial marker.

import socket
import sys

HOST = "0.0.0.0"
PORT = 23096
ACCEPT_SECONDS = 160
IO_SECONDS = 30


def log(path, text):
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(text + "\n")


def read_exact(connection, length):
    data = bytearray()
    while len(data) < length:
        chunk = connection.recv(length - len(data))
        if not chunk:
            raise OSError(f"eof after {len(data)} of {length}")
        data.extend(chunk)
    return bytes(data)


def main():
    log_path = "build/fixtures/socket-write-exec/host.log"
    result_path = "build/fixtures/socket-write-exec/host.result"
    open(log_path, "w", encoding="utf-8").close()

    listener = socket.socket()
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind((HOST, PORT))
    listener.listen(1)
    listener.settimeout(ACCEPT_SECONDS)
    log(log_path, f"listening {HOST}:{PORT}")
    try:
        connection, address = listener.accept()
    except OSError as error:
        log(log_path, f"accept error: {error}")
        with open(result_path, "w", encoding="utf-8") as handle:
            handle.write("1\n")
        return 1
    log(log_path, f"accepted {address}")
    connection.settimeout(IO_SECONDS)

    steps = [
        ("same", 4, b"SAME"),
        ("fork", 4, b"FORK"),
        ("exec", 4, b"EXEC"),
    ]
    try:
        for name, length, expected in steps:
            got = read_exact(connection, length)
            log(log_path, f"{name}={got!r}")
            if got != expected:
                raise OSError(f"{name} mismatch {got!r}")
        connection.sendall(b"READ")
        got = read_exact(connection, 4)
        log(log_path, f"read={got!r}")
        if got != b"READ":
            raise OSError(f"read mismatch {got!r}")
        connection.sendall(b"ACK2")
        got = read_exact(connection, 6)
        log(log_path, f"second={got!r}")
        if got != b"SECOND":
            raise OSError(f"second mismatch {got!r}")
        connection.sendall(b"DONE")
        log(log_path, "verified all variants")
        with open(result_path, "w", encoding="utf-8") as handle:
            handle.write("0\n")
        return 0
    except OSError as error:
        log(log_path, f"failed: {error}")
        with open(result_path, "w", encoding="utf-8") as handle:
            handle.write("1\n")
        return 1
    finally:
        connection.close()
        listener.close()


if __name__ == "__main__":
    sys.exit(main())
