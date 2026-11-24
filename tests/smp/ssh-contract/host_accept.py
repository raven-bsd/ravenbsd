#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K435 K6 host peer: connect twice through the QEMU hostfwd to the guest listener, verify the
exact echo and send the acknowledgement that makes the guest print `CONTRACT K6 PASS`."""

import argparse
import re
import select
import socket
import sys
import time

_LOG = ""
_ORIGIN = time.monotonic()


def note(message):
    """Append a timed diagnostic line to the host peer log, if one was configured."""
    if not _LOG:
        return
    elapsed = time.monotonic() - _ORIGIN
    with open(_LOG, "a", encoding="utf-8") as handle:
        handle.write(f"{elapsed:8.3f}s {message}\n")


def parse_usernet(text):
    """Map guest ports 23096-23098 to the ephemeral host ports QEMU chose."""
    ports = {}
    pattern = re.compile(
        r"TCP\[HOST_FORWARD\]\s+\d+\s+\S+\s+(\d+)\s+\S+\s+(\d+)"
    )
    for line in text.splitlines():
        match = pattern.search(line)
        if match is None:
            continue
        host_port = int(match.group(1))
        guest_port = int(match.group(2))
        if guest_port in (23096, 23097, 23098):
            ports[guest_port] = host_port
    return ports


def discover_ports(path, deadline):
    """Read QEMU's `info usernet` from the monitor socket until the K6 forwards are known."""
    last = "monitor socket did not appear"
    while time.monotonic() < deadline:
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as monitor:
                remaining = max(0.1, deadline - time.monotonic())
                monitor.settimeout(min(2.0, remaining))
                monitor.connect(path)
                monitor.sendall(b"info usernet\n")
                data = b""
                read_deadline = min(deadline, time.monotonic() + 2.0)
                while time.monotonic() < read_deadline:
                    try:
                        chunk = monitor.recv(4096)
                    except socket.timeout:
                        break
                    if not chunk:
                        break
                    data += chunk
                    ports = parse_usernet(data.decode(errors="replace"))
                    if 23097 in ports and 23098 in ports:
                        return ports
            last = f"incomplete forwarding table: {parse_usernet(data.decode(errors='replace'))!r}"
        except OSError as error:
            last = str(error)
        time.sleep(0.1)
    raise RuntimeError(f"QEMU monitor {path}: {last}")


def wait_ready(host, port, deadline):
    """Wait for the guest K6-ready marker before opening the first K6 connection."""
    started = time.monotonic()
    attempts = 0
    last = "no attempt"
    while time.monotonic() < deadline:
        attempts += 1
        try:
            with socket.create_connection((host, port), timeout=2.0) as stream:
                # A connect issued before the guest has its IP can sit unanswered in slirp
                # until the guest stack comes up. Bound the greeting so this attempt closes
                # and retries promptly instead of consuming the guest's 8 s ready window.
                stream.settimeout(2.0)
                if attempts <= 5:
                    note(
                        f"K6-ready connected port={port} attempt={attempts} "
                        f"elapsed={time.monotonic() - started:.3f}s"
                    )
                if read_line(stream) == b"READY":
                    stream.sendall(b"A")
                    note(
                        f"K6-ready ok port={port} attempt={attempts} "
                        f"elapsed={time.monotonic() - started:.3f}s"
                    )
                    return True
        except OSError as error:
            last = str(error)
            if attempts <= 5 or attempts % 100 == 0:
                note(
                    f"K6-ready retry port={port} attempt={attempts} "
                    f"elapsed={time.monotonic() - started:.3f}s error={last}"
                )
            time.sleep(0.2)
    note(
        f"K6-ready timeout port={port} attempts={attempts} "
        f"elapsed={time.monotonic() - started:.3f}s last={last}"
    )
    return False


def exchange(host, port, payload, ack, deadline):
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=5.0) as stream:
                stream.settimeout(30.0)
                stream.sendall(payload)
                echoed = b""
                while len(echoed) < len(payload):
                    chunk = stream.recv(len(payload) - len(echoed))
                    if not chunk:
                        raise OSError("eof before echo")
                    echoed += chunk
                if echoed != payload:
                    raise OSError(f"echo mismatch {echoed!r}")
                stream.sendall(ack)
                return True
        except OSError:
            time.sleep(0.2)
    return False


def read_line(stream, limit=64):
    data = bytearray()
    while len(data) < limit:
        chunk = stream.recv(1)
        if not chunk:
            raise OSError("eof in greeting")
        if chunk == b"\n":
            return bytes(data)
        data.extend(chunk)
    raise OSError(f"greeting too long {bytes(data)!r}")


def eof_fin(host, port, deadline):
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=5.0) as stream:
                stream.settimeout(30.0)
                greeting = read_line(stream)
                if greeting != b"EOF":
                    raise OSError(f"eof greeting {greeting!r}")
                return True
        except OSError:
            time.sleep(0.2)
    return False


def k6_exchange(host, port, deadline):
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=5.0) as stream:
                stream.settimeout(30.0)
                stream.sendall(b"PING")
                echoed = b""
                while len(echoed) < 4:
                    chunk = stream.recv(4 - len(echoed))
                    if not chunk:
                        raise OSError("eof before PING echo")
                    echoed += chunk
                if echoed == b"PING":
                    return True
        except OSError:
            time.sleep(0.2)
    return False


def k6_loop(host, port, loops, deadline):
    for _ in range(loops):
        if not k6_exchange(host, port, deadline):
            return False
    return True


def own_shutdown(host, port, deadline):
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=5.0) as stream:
                stream.settimeout(30.0)
                greeting = read_line(stream)
                if greeting != b"SHUT":
                    raise OSError(f"shutdown greeting {greeting!r}")
                while stream.recv(64):
                    pass
                return True
        except OSError:
            time.sleep(0.2)
    return False


def half_close(host, port, deadline):
    """Guest-initiated half-close: FIN after HALFACK drains, peer EOF/HUP, no RST."""
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=5.0) as stream:
                stream.settimeout(30.0)
                greeting = read_line(stream)
                if greeting != b"HALF":
                    raise OSError(f"half greeting {greeting!r}")
                stream.sendall(b"DATA")
                stream.shutdown(socket.SHUT_WR)
                ack = b""
                while len(ack) < 7:
                    chunk = stream.recv(7 - len(ack))
                    if not chunk:
                        raise OSError("eof before HALFACK")
                    ack += chunk
                if ack != b"HALFACK":
                    raise OSError(f"half ack {ack!r}")
                # The guest now sends its FIN after the queued HALFACK drains. A graceful FIN
                # reads as EOF; a RST raises ConnectionResetError and fails the row. The host
                # has shut down its write side, so both directions are closed: poll must report
                # HUP. No SO_LINGER is set, so the close after this block cannot inject a RST.
                try:
                    tail = stream.recv(1)
                except ConnectionResetError:
                    return False
                if tail != b"":
                    raise OSError(f"half tail {tail!r}")
                poller = select.poll()
                poller.register(stream, select.POLLIN | select.POLLHUP)
                events = poller.poll(5000)
                if not events or not (events[0][1] & select.POLLHUP):
                    return False
                return True
        except ConnectionResetError:
            return False
        except socket.timeout:
            return False
        except OSError:
            time.sleep(0.2)
    return False


def main():
    global _LOG
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=23097)
    parser.add_argument("--eof-port", type=int, default=23098)
    parser.add_argument("--ready-port", type=int, default=23096)
    parser.add_argument("--k6-ready", action="store_true")
    parser.add_argument("--k1k8", action="store_true")
    parser.add_argument("--monitor", default="")
    parser.add_argument("--k6-loop", type=int, default=0)
    parser.add_argument("--result", required=True)
    parser.add_argument("--log", default="")
    parser.add_argument("--timeout", type=float, default=600.0)
    args = parser.parse_args()
    _LOG = args.log

    deadline = time.monotonic() + args.timeout
    if args.monitor:
        try:
            ports = discover_ports(args.monitor, deadline)
            note(f"monitor ports {sorted(ports.items())}")
            args.port = ports[23097]
            args.eof_port = ports[23098]
            if 23096 in ports:
                args.ready_port = ports[23096]
        except RuntimeError as error:
            with open(args.result, "w", encoding="utf-8") as handle:
                handle.write(f"fail monitor: {error}\n")
            if args.log:
                with open(args.log, "a", encoding="utf-8") as handle:
                    handle.write(f"monitor fail: {error}\n")
            print(f"host peer: {error}", file=sys.stderr)
            return 1
    if args.k6_ready:
        if not wait_ready(args.host, args.ready_port, deadline):
            with open(args.result, "w", encoding="utf-8") as handle:
                handle.write("fail k6-ready\n")
            if args.log:
                with open(args.log, "a", encoding="utf-8") as handle:
                    handle.write("K6-ready fail\n")
            return 1
        if args.log:
            with open(args.log, "a", encoding="utf-8") as handle:
                handle.write("K6-ready ok\n")
    if args.k6_loop:
        if not k6_loop(args.host, args.port, args.k6_loop, deadline):
            with open(args.result, "w", encoding="utf-8") as handle:
                handle.write("fail k6-loop\n")
            if args.log:
                with open(args.log, "a", encoding="utf-8") as handle:
                    handle.write("K6-loop fail\n")
            return 1
        with open(args.result, "w", encoding="utf-8") as handle:
            handle.write("ok\n")
        if args.log:
            with open(args.log, "a", encoding="utf-8") as handle:
                handle.write(f"K6-loop ok {args.k6_loop}\n")
        return 0
    steps = [(b"ACCEPT1", b"K6ACK1"), (b"ACCEPT2", b"K6ACK2")]
    for payload, ack in steps:
        if not exchange(args.host, args.port, payload, ack, deadline):
            with open(args.result, "w", encoding="utf-8") as handle:
                handle.write(f"fail {payload!r}\n")
            if args.log:
                with open(args.log, "a", encoding="utf-8") as handle:
                    handle.write(f"K6 host fail {payload!r}\n")
            return 1
        if args.log:
            with open(args.log, "a", encoding="utf-8") as handle:
                handle.write(f"K6 host ok {payload!r}\n")
    roles = [("K8", eof_fin), ("K8-shutdown", own_shutdown)]
    if not args.k1k8:
        roles.append(("K9", half_close))
    for name, role in roles:
        if not role(args.host, args.eof_port, deadline):
            with open(args.result, "w", encoding="utf-8") as handle:
                handle.write(f"fail {name}\n")
            if args.log:
                with open(args.log, "a", encoding="utf-8") as handle:
                    handle.write(f"{name} host fail\n")
            return 1
        if args.log:
            with open(args.log, "a", encoding="utf-8") as handle:
                handle.write(f"{name} host ok\n")
    with open(args.result, "w", encoding="utf-8") as handle:
        handle.write("ok\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
