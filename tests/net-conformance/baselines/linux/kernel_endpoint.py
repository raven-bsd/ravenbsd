#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Linux kernel TCP endpoint for the K263 baselines.

This runs in the host namespace and binds 10.263.0.1 on the veth; the raw peer is
10.263.0.2. Modes are deliberately small so the scenario controls the peer side.
"""

import argparse
import socket
import sys
import time

HOST_IP = "10.26.3.1"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    parser.add_argument("--mode", default="listener",
                        choices=["listener", "hold", "write", "read", "close"])
    parser.add_argument("--bytes", type=int, default=4096)
    parser.add_argument("--chunk", type=int, default=1024)
    parser.add_argument("--interval-ms", type=int, default=40)
    parser.add_argument("--backlog", type=int, default=4)
    parser.add_argument("--hold-ms", type=int, default=5000)
    args = parser.parse_args()

    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind((HOST_IP, args.port))
    listener.listen(args.backlog)
    print("READY", flush=True)

    if args.mode == "hold":
        time.sleep(args.hold_ms / 1000.0)
        listener.close()
        return 0

    if args.mode == "listener":
        while True:
            try:
                conn, peer = listener.accept()
                print(f"ACCEPTED {peer}", flush=True)
                conn.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
                conn.close()
            except OSError:
                break
        return 0

    try:
        conn, peer = listener.accept()
    except OSError as error:
        print(f"ACCEPT-FAIL {error}", flush=True)
        return 1
    conn.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
    print(f"ACCEPTED {peer}", flush=True)

    if args.mode == "write":
        payload = bytes((index % 251 for index in range(args.bytes)))
        sent = 0
        while sent < len(payload):
            chunk = payload[sent:sent + args.chunk]
            try:
                conn.sendall(chunk)
            except OSError as error:
                print(f"WRITE-FAIL {error}", flush=True)
                break
            sent += len(chunk)
            print(f"WROTE {sent}", flush=True)
            time.sleep(args.interval_ms / 1000.0)
        time.sleep(args.hold_ms / 1000.0)
    elif args.mode == "read":
        try:
            while True:
                data = conn.recv(4096)
                if not data:
                    print("EOF", flush=True)
                    break
                print(f"READ {len(data)}", flush=True)
        except OSError as error:
            print(f"READ-FAIL {error}", flush=True)
    elif args.mode == "close":
        conn.close()
        print("CLOSED", flush=True)
    conn.close()
    listener.close()
    time.sleep(0.2)
    return 0


if __name__ == "__main__":
    sys.exit(main())
