#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K382 host peer: listen on 0.0.0.0:23095 for the guest's connection, read the 32 KiB TX burst,
# verify it and echo the same bytes back. The guest logs `done` only after the echo arrives.
#
# K433 adds the SSH confirmation shape: after the echo, send a 4-byte request, verify the four
# 44-byte replies byte-for-byte, then acknowledge.

import socket
import sys

HOST = "0.0.0.0"
PORT = 23095
FRAMES = 32
FRAME_BYTES = 1024
TOTAL = FRAMES * FRAME_BYTES
CONFIRM_FRAMES = 4
CONFIRM_BYTES = 44
CONFIRM_BASE = 0x40
CONFIRM_TOTAL = CONFIRM_FRAMES * CONFIRM_BYTES


def log(path, text):
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(text + "\n")


def expected():
    payload = bytearray()
    for offset in range(TOTAL):
        payload.append((offset // FRAME_BYTES + offset % FRAME_BYTES) & 0xFF)
    return payload


def main():
    log_path = "build/fixtures/virtio-tx/host.log"
    result_path = "build/fixtures/virtio-tx/host.result"
    open(log_path, "w", encoding="utf-8").close()

    listener = socket.socket()
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind((HOST, PORT))
    listener.listen(1)
    listener.settimeout(160)
    log(log_path, f"listening {HOST}:{PORT}")
    try:
        connection, address = listener.accept()
    except OSError as error:
        log(log_path, f"accept error: {error}")
        with open(result_path, "w", encoding="utf-8") as handle:
            handle.write("1\n")
        return 1
    log(log_path, f"accepted {address}")
    connection.settimeout(30)
    try:
        received = bytearray()
        while len(received) < TOTAL:
            chunk = connection.recv(TOTAL - len(received))
            if not chunk:
                break
            received.extend(chunk)
        log(log_path, f"received={len(received)}")
        if received != expected():
            log(log_path, "mismatch")
            with open(result_path, "w", encoding="utf-8") as handle:
                handle.write("1\n")
            return 1
        connection.sendall(received)
        log(log_path, "echoed")

        # K433 confirmation shape: request, four 44-byte replies, exact-byte verification.
        connection.sendall(b"REQ\n")
        log(log_path, "sent request=4")
        confirmation = bytearray()
        while len(confirmation) < CONFIRM_TOTAL:
            chunk = connection.recv(CONFIRM_TOTAL - len(confirmation))
            if not chunk:
                break
            confirmation.extend(chunk)
        log(log_path, f"confirmation={len(confirmation)} bytes")
        expected_confirmation = bytes(
            (CONFIRM_BASE + frame * CONFIRM_BYTES + index) & 0xFF
            for frame in range(CONFIRM_FRAMES)
            for index in range(CONFIRM_BYTES)
        )
        if confirmation != expected_confirmation:
            log(log_path, "confirmation mismatch")
            with open(result_path, "w", encoding="utf-8") as handle:
                handle.write("1\n")
            return 1
        connection.sendall(b"ACK\n")
        log(log_path, f"confirmation={len(confirmation)} frames={CONFIRM_FRAMES} ack")
        with open(result_path, "w", encoding="utf-8") as handle:
            handle.write("0\n")
        return 0
    except OSError as error:
        log(log_path, f"io error: {error}")
        with open(result_path, "w", encoding="utf-8") as handle:
            handle.write("1\n")
        return 1
    finally:
        connection.close()
        listener.close()


if __name__ == "__main__":
    sys.exit(main())
