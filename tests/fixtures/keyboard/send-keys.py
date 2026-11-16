#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Inject keystrokes into the QEMU guest through the QMP monitor.

The qtest harness has no keyboard hook, so the case starts this helper before QEMU. It waits for
the QMP socket, enables QMP, waits for the guest to reach the fixture and sends h, i and Enter.
"""

import json
import socket
import sys
import time


def connect(path: str) -> socket.socket:
    deadline = time.time() + 60
    while time.time() < deadline:
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            sock.connect(path)
            return sock
        except OSError:
            sock.close()
            time.sleep(0.1)
    raise SystemExit("send-keys: QMP socket did not appear")


def send(sock: socket.socket, message: dict) -> None:
    sock.sendall((json.dumps(message) + "\n").encode("utf-8"))
    try:
        sock.recv(65536)
    except OSError:
        pass


def main() -> None:
    sock = connect(sys.argv[1])
    sock.settimeout(5)
    try:
        sock.recv(65536)  # QMP greeting
    except OSError:
        pass
    send(sock, {"execute": "qmp_capabilities"})
    # The guest boots the kernel and the fixture in a few seconds; this margin also covers a
    # cold TCG boot.
    time.sleep(8)
    for key in ("h", "i", "ret"):
        send(
            sock,
            {
                "execute": "human-monitor-command",
                "arguments": {"command-line": f"sendkey {key}"},
            },
        )
        time.sleep(0.3)
    sock.close()


if __name__ == "__main__":
    main()
