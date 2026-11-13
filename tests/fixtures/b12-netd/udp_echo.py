#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""One-shot UDP echo server for the netd-virtio qtest."""

import socket
import sys

port = int(sys.argv[1])
server = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server.bind(("127.0.0.1", port))
server.settimeout(120)
try:
    # One datagram for each of the two netd lifecycle probes.
    for _ in range(2):
        data, peer = server.recvfrom(2048)
        server.sendto(data, peer)
finally:
    server.close()
