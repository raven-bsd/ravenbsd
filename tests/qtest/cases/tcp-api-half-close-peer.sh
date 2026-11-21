#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K294 host peer for the half-close qtest: connect to the guest netconf listener,
# receive its 44-byte payload, shut down the write direction (FIN), drain until
# EOF, and close. The qtest asserts the guest's peer-event read=0 and closed lines.

set -eu
port=${1:-23090}
python3 - "$port" <<'PY'
import socket
import sys
import time

port = int(sys.argv[1])
deadline = time.time() + 60
connection = None
while time.time() < deadline:
    try:
        connection = socket.create_connection(("127.0.0.1", port), timeout=2)
        break
    except OSError:
        time.sleep(0.2)
if connection is None:
    print("tcp-api-half-close-peer: connect failed", flush=True)
    sys.exit(1)

connection.settimeout(20)
payload = b""
while len(payload) < 44:
    chunk = connection.recv(44 - len(payload))
    if not chunk:
        break
    payload += chunk
print(f"tcp-api-half-close-peer: read {len(payload)}", flush=True)
connection.shutdown(socket.SHUT_WR)
while connection.recv(64):
    pass
connection.close()
print("tcp-api-half-close-peer: eof", flush=True)
PY
