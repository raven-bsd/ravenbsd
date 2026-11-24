#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Deterministic local HTTP content server for the E-Ink fetch proof (K201).

Serves the fixed insult body on /insult and /insult-final, a 302 redirect on /redirect
(Location: /insult-final), and 404 elsewhere. It binds 127.0.0.1 only, never the external
network, and exits after --max-requests or the idle timeout.
"""

import argparse
import pathlib
import socket
import sys

BODY = pathlib.Path(__file__).with_name("insult.txt").read_bytes()
NOT_FOUND = b"not found\n"
HOST = "127.0.0.1"


def response(status: str, body: bytes, headers: list[tuple[str, str]]) -> bytes:
    lines = [f"HTTP/1.1 {status}"]
    lines.extend(f"{name}: {value}" for name, value in headers)
    lines.extend(["Connection: close", f"Content-Length: {len(body)}", "", ""])
    return "\r\n".join(lines).encode("ascii") + body


def route(path: str) -> bytes:
    if path in ("/insult", "/insult-final"):
        return response(
            "200 OK",
            BODY,
            [("Content-Type", "text/plain; charset=utf-8")],
        )
    if path == "/redirect":
        return response(
            "302 Found",
            b"",
            [("Location", "/insult-final"), ("Content-Type", "text/plain")],
        )
    return response("404 Not Found", NOT_FOUND, [("Content-Type", "text/plain")])


def serve(client: socket.socket) -> str:
    client.settimeout(10)
    request = b""
    while b"\r\n\r\n" not in request and len(request) < 8192:
        chunk = client.recv(1024)
        if not chunk:
            break
        request += chunk
    line = request.split(b"\r\n", 1)[0].decode("ascii", "replace")
    parts = line.split()
    if len(parts) < 2 or parts[0] != "GET":
        client.sendall(response("405 Method Not Allowed", b"method\n", [("Content-Type", "text/plain")]))
        return line
    target = parts[1]
    path = target.split("?", 1)[0]
    client.sendall(route(path))
    return line


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=18083)
    parser.add_argument("--max-requests", type=int, default=8)
    parser.add_argument("--idle-timeout", type=float, default=300.0)
    options = parser.parse_args()

    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.settimeout(options.idle_timeout)
    try:
        server.bind((HOST, options.port))
        server.listen(8)
        print(f"insult-server: listening {HOST}:{options.port}", flush=True)
        for _ in range(options.max_requests):
            try:
                client, address = server.accept()
            except socket.timeout:
                print("insult-server: idle timeout", flush=True)
                return 0
            with client:
                line = serve(client)
            print(f"insult-server: {address[0]} {line}", flush=True)
        print("insult-server: served", flush=True)
        return 0
    except OSError as error:
        print(f"insult-server: {error}", flush=True)
        return 1
    finally:
        server.close()


if __name__ == "__main__":
    sys.exit(main())
