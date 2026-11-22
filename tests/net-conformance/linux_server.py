#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K253 Linux baseline server: a host-kernel TCP endpoint inside a netns.

It accepts one raw-client connection and writes exactly the 44-byte fixture.
The client controls the advertised window, so the server's normal TCP stack is
the reference for persist probes, window updates and retransmission.
"""

import argparse
import socket

PAYLOAD = b"K253-zero-window-44-byte-payload-0123456789!"
assert len(PAYLOAD) == 44


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="10.99.0.2")
    parser.add_argument("--port", type=int, default=23090)
    args = parser.parse_args()
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind((args.host, args.port))
    listener.listen(4)
    listener.settimeout(0.5)
    print("ready", flush=True)
    while True:
        try:
            connection, _ = listener.accept()
        except socket.timeout:
            continue
        try:
            connection.sendall(PAYLOAD)
            # Hold the connection open for persist/window/retransmit tests, but
            # close immediately when the peer sends FIN so the wire sees a FIN.
            connection.settimeout(8.0)
            try:
                while connection.recv(1):
                    pass
            except socket.timeout:
                pass
        finally:
            connection.close()


if __name__ == "__main__":
    raise SystemExit(main())
