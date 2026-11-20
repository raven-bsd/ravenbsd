#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Host selftest for the K201 insult server fixture. No external network is used."""

import http.client
import pathlib
import socket
import subprocess
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
BODY = (HERE / "insult.txt").read_bytes()


def free_port() -> int:
    probe = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    probe.bind(("127.0.0.1", 0))
    port = probe.getsockname()[1]
    probe.close()
    return port


def request(port: int, path: str) -> tuple[int, dict[str, str], bytes]:
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    connection.request("GET", path)
    response = connection.getresponse()
    body = response.read()
    headers = {name.lower(): value for name, value in response.getheaders()}
    connection.close()
    return response.status, headers, body


def main() -> int:
    port = free_port()
    server = subprocess.Popen(
        [
            sys.executable,
            str(HERE / "server.py"),
            "--port",
            str(port),
            "--max-requests",
            "8",
            "--idle-timeout",
            "20",
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    try:
        deadline = time.time() + 5
        while time.time() < deadline:
            try:
                request(port, "/missing")
                break
            except OSError:
                time.sleep(0.05)
        else:
            print("insult-server selftest: server did not start")
            return 1

        status, headers, body = request(port, "/insult")
        assert status == 200, status
        assert body == BODY, body
        assert headers.get("content-type", "").startswith("text/plain"), headers

        status, _, body = request(port, "/missing")
        assert status == 404, status

        status, headers, _ = request(port, "/redirect")
        assert status == 302, status
        assert headers.get("location") == "/insult-final", headers

        status, _, body = request(port, "/insult-final")
        assert status == 200, status
        assert body == BODY, body

        print(f"insult-server selftest: PASS body={BODY.decode().strip()!r}")
        return 0
    finally:
        server.terminate()
        try:
            server.wait(timeout=5)
        except subprocess.TimeoutExpired:
            server.kill()


if __name__ == "__main__":
    sys.exit(main())
