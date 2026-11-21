#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K408 host peer. In `listen` mode (default) it waits for the connected-socket guest. In
# `connect` mode it connects to the guest's accepted listener over the K194 tap (10.0.2.15:23096).
# Both modes verify the five write markers in order and send the READ/ACK2/DONE triggers only
# after the earlier markers arrive.

import os
import socket
import sys
import time

PORT = 23096
ACCEPT_SECONDS = 160
CONNECT_SECONDS = 120
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


def verify(connection, log_path, result_path):
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


def connect_guest(log_path):
    host = os.environ.get("SOCKET_WRITE_EXEC_HOST", "10.0.2.15")
    deadline = time.monotonic() + CONNECT_SECONDS
    while time.monotonic() < deadline:
        try:
            return socket.create_connection((host, PORT), timeout=3)
        except OSError as error:
            log(log_path, f"connect retry: {error}")
            time.sleep(0.2)
    return None


def main():
    log_path = "build/fixtures/socket-write-exec/host.log"
    result_path = "build/fixtures/socket-write-exec/host.result"
    mode = os.environ.get("SOCKET_WRITE_EXEC_HOST_MODE", "listen")
    open(log_path, "w", encoding="utf-8").close()

    if mode == "connect":
        log(log_path, "connecting to the guest listener")
        connection = connect_guest(log_path)
        if connection is None:
            log(log_path, "connect timeout")
            with open(result_path, "w", encoding="utf-8") as handle:
                handle.write("1\n")
            return 1
        try:
            return verify(connection, log_path, result_path)
        finally:
            connection.close()

    listener = socket.socket()
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("0.0.0.0", PORT))
    listener.listen(1)
    listener.settimeout(ACCEPT_SECONDS)
    log(log_path, f"listening 0.0.0.0:{PORT}")
    try:
        connection, address = listener.accept()
    except OSError as error:
        log(log_path, f"accept error: {error}")
        with open(result_path, "w", encoding="utf-8") as handle:
            handle.write("1\n")
        return 1
    log(log_path, f"accepted {address}")
    try:
        return verify(connection, log_path, result_path)
    finally:
        connection.close()
        listener.close()


if __name__ == "__main__":
    sys.exit(main())
