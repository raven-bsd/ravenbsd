#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K294 host peer for the listener-backlog qtest: open four connections before the
# guest accepts any, then send one byte on each and read the 44-byte payload. The
# qtest asserts four accepted fd/closed pairs, so the listen backlog must hold the
# queued connections.

set -eu
port=${1:-23090}
python3 - "$port" <<'PY'
import socket
import sys
import time

port = int(sys.argv[1])
deadline = time.time() + 60
connections = []
for index in range(4):
    while time.time() < deadline:
        try:
            connection = socket.create_connection(("127.0.0.1", port), timeout=2)
            connection.settimeout(20)
            connections.append(connection)
            print(f"tcp-api-backlog-peer: connected {index + 1}", flush=True)
            break
        except OSError:
            time.sleep(0.2)
    else:
        print("tcp-api-backlog-peer: connect failed", flush=True)
        sys.exit(1)

for connection in connections:
    connection.sendall(b"x")

for connection in connections:
    payload = b""
    while len(payload) < 44:
        chunk = connection.recv(44 - len(payload))
        if not chunk:
            break
        payload += chunk
    print(f"tcp-api-backlog-peer: read {len(payload)}", flush=True)
    connection.close()

print("tcp-api-backlog-peer: done", flush=True)
PY
