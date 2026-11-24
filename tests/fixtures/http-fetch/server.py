#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""One-shot local HTTP server for the E3a live fetch gate."""

import socket
import sys

HOST = "127.0.0.1"
PORT = 18080
BODY = b"known-http-body"
RESPONSE = (
    b"HTTP/1.1 200 OK\r\n"
    b"Content-Length: 15\r\n"
    b"Content-Type: text/plain\r\n"
    b"Connection: close\r\n"
    b"\r\n" + BODY
)


def main() -> int:
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.settimeout(300)
    try:
        server.bind((HOST, PORT))
        server.listen(1)
        print(f"server: listening {HOST}:{PORT}", flush=True)
        client, address = server.accept()
        print(f"server: accepted {address}", flush=True)
        client.settimeout(10)
        request = b""
        while b"\r\n\r\n" not in request and len(request) < 8192:
            chunk = client.recv(1024)
            if not chunk:
                break
            request += chunk
        line = request.split(b"\r\n", 1)[0].decode("ascii", "replace")
        print(f"server: request {line}", flush=True)
        client.sendall(RESPONSE)
        client.close()
        print("server: served", flush=True)
        return 0
    except socket.timeout:
        print("server: timeout", flush=True)
        return 1
    except OSError as error:
        print(f"server: {error}", flush=True)
        return 1
    finally:
        server.close()


if __name__ == "__main__":
    sys.exit(main())
