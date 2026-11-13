#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Host-only self-test for the B70 echo client: loopback server, exact payload,
echo and acknowledgment, without QEMU."""

import argparse
import os
import socket
import subprocess
import sys
import tempfile
import threading

PAYLOAD = b"b70-host-echo-payload"
ACK = b"b70-host-ack"


def recv_exact(connection, wanted):
    received = b""
    while len(received) < wanted:
        chunk = connection.recv(wanted - len(received))
        if not chunk:
            return None
        received += chunk
    return received


def serve(listener, ready):
    listener.listen(1)
    ready.set()
    connection, _ = listener.accept()
    with connection:
        received = recv_exact(connection, len(PAYLOAD))
        if received != PAYLOAD:
            return
        connection.sendall(PAYLOAD)
        ack = recv_exact(connection, len(ACK))
        if ack != ACK:
            return


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--client", default="tests/fixtures/b70-net/host-echo.py")
    args = parser.parse_args()

    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind(("127.0.0.1", 0))
    port = listener.getsockname()[1]
    ready = threading.Event()
    worker = threading.Thread(target=serve, args=(listener, ready), daemon=True)
    worker.start()
    ready.wait(5.0)
    try:
        with tempfile.TemporaryDirectory() as directory:
            result = os.path.join(directory, "result")
            completed = subprocess.run(
                [
                    sys.executable,
                    args.client,
                    "--host",
                    "127.0.0.1",
                    "--port",
                    str(port),
                    "--result",
                    result,
                    "--timeout",
                    "10",
                ],
                check=False,
            )
            if completed.returncode != 0:
                print("b70: host echo self-test failed", file=sys.stderr)
                return 1
            with open(result, encoding="utf-8") as handle:
                line = handle.read().strip()
            if line != "ok payload=b70-host-echo-payload ack=b70-host-ack":
                print(f"b70: host echo self-test bad result: {line}", file=sys.stderr)
                return 1
    finally:
        listener.close()
    print("b70: host echo self-test ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
