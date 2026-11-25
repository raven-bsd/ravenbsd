#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Host peer for the MPTCP2c two-tap fixture.

The peer listens on the primary bridge address with an MPTCP socket when the host kernel
supports it (Linux falls back to plain TCP for a non-MP_CAPABLE client, which is exactly the
negative control). It reads the guest's deterministic 64 KiB stream, then unplugs the primary
QEMU netdev through QMP while the stream is idle. The guest must notice the dead path within its
bounded poll; a fixture that cannot unplug fails the negative control.

The backup listener is opened too, so the positive case has both addresses ready once MPTCP2b
lands. No data is sent after the unplug, so the guest's next read has no false liveness signal.
"""

import argparse
import json
import socket
import sys
import threading
import time

PRIMARY = "10.77.1.2"
BACKUP = "10.77.2.2"
PORT = 25021
READY = b"NEG-READY\n"
GO = b"GO\n"
STREAM_BYTES = 65536


def log(line, path):
    print(line, flush=True)
    if path:
        try:
            with open(path, "a", encoding="utf-8") as handle:
                handle.write(line + "\n")
        except OSError:
            pass


def write_result(path, text):
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(text + "\n")


def recv_exact(stream, wanted, timeout):
    stream.settimeout(timeout)
    received = b""
    while len(received) < wanted:
        chunk = stream.recv(wanted - len(received))
        if not chunk:
            return None
        received += chunk
    return received


def qmp_open(path):
    """Open a QMP monitor, negotiate capabilities, and return (socket, line stream)."""
    monitor = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    monitor.settimeout(20.0)
    try:
        monitor.connect(path)
        stream = monitor.makefile("rwb")
        if not stream.readline():
            raise RuntimeError("qmp closed before greeting")
        stream.write(b'{"execute":"qmp_capabilities"}\n')
        stream.flush()
        while True:
            line = stream.readline()
            if not line:
                raise RuntimeError("qmp closed before capabilities")
            if b'"return"' in line:
                break
            if b'"error"' in line:
                raise RuntimeError(
                    f"qmp capabilities error: {line.decode(errors='replace').strip()}"
                )
        return monitor, stream
    except Exception:
        monitor.close()
        raise


def qmp_send(stream, command):
    """Send one QMP command over an open monitor and return its response object."""
    stream.write((json.dumps(command) + "\n").encode("utf-8"))
    stream.flush()
    while True:
        line = stream.readline()
        if not line:
            raise RuntimeError("qmp closed before response")
        if b'"error"' in line:
            raise RuntimeError(f"qmp error: {line.decode(errors='replace').strip()}")
        if b'"return"' in line:
            return json.loads(line.decode("utf-8"))


def opener(protocol, host, port):
    listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM, protocol)
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    listener.bind((host, port))
    listener.listen(4)
    return listener


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("negative", "listen"), default="negative")
    parser.add_argument("--qmp", default="")
    parser.add_argument("--result", required=True)
    parser.add_argument("--log", default="")
    parser.add_argument("--ready", default="")
    parser.add_argument("--timeout", type=float, default=300.0)
    args = parser.parse_args()

    backups = []
    primary = None
    stream = None
    qmp_holder = {}
    qmp_ready = threading.Event()
    try:
        try:
            primary = opener(262, PRIMARY, PORT)
            protocol = "mptcp"
        except OSError as error:
            log(f"host: mptcp listener unavailable ({error}); plain tcp fallback", args.log)
            primary = opener(0, PRIMARY, PORT)
            protocol = "tcp-fallback"
        try:
            backups.append(opener(262 if protocol == "mptcp" else 0, BACKUP, PORT))
        except OSError as error:
            log(f"host: backup listener unavailable: {error}", args.log)
        log(f"host: listening primary={PRIMARY}:{PORT} backup={BACKUP}:{PORT} protocol={protocol}", args.log)
        if args.ready:
            write_result(args.ready, f"protocol={protocol} listeners={1 + len(backups)}")

        if args.mode == "listen":
            deadline = time.monotonic() + args.timeout
            accepted = 0
            while time.monotonic() < deadline:
                primary.settimeout(1.0)
                try:
                    connection, peer = primary.accept()
                except socket.timeout:
                    continue
                accepted += 1
                log(f"host: peer connected peer={peer[0]}:{peer[1]}", args.log)
                try:
                    connection.settimeout(10.0)
                    connection.recv(4096)
                except OSError:
                    pass
                connection.close()
                break
            write_result(
                args.result,
                f"ok=1 mode=listen protocol={protocol} listeners={1 + len(backups)} accepted={accepted}",
            )
            return 0

        if args.mode == "negative" and args.qmp:
            def connect_qmp():
                deadline = time.monotonic() + args.timeout
                last = None
                while time.monotonic() < deadline:
                    try:
                        monitor, stream = qmp_open(args.qmp)
                    except (OSError, RuntimeError) as error:
                        last = error
                        log(f"host: qmp retry: {error}", args.log)
                        time.sleep(0.5)
                        continue
                    except Exception as error:  # noqa: BLE001 - a dead thread must be visible
                        last = error
                        log(f"host: qmp unexpected: {type(error).__name__}: {error}", args.log)
                        time.sleep(0.5)
                        continue
                    qmp_holder["monitor"] = monitor
                    qmp_holder["stream"] = stream
                    qmp_ready.set()
                    log("host: qmp connected", args.log)
                    return
                qmp_holder["error"] = last
                qmp_ready.set()

            threading.Thread(target=connect_qmp, daemon=True).start()

        if not args.qmp:
            raise RuntimeError("negative mode requires --qmp")
        stream, peer = primary.accept()
        accepted_at = time.monotonic()
        log(f"host: accepted peer={peer[0]}:{peer[1]}", args.log)

        if recv_exact(stream, len(READY), 60.0) != READY:
            raise RuntimeError("missing NEG-READY")

        stream.sendall(GO)
        log("host: go sent", args.log)

        received = recv_exact(stream, STREAM_BYTES, 60.0)
        if received is None:
            raise RuntimeError("short stream")
        for index, byte in enumerate(received):
            if byte != (index & 0xFF):
                raise RuntimeError(f"stream pattern mismatch at {index}")
        log(f"host: stream ok bytes={STREAM_BYTES} t={(time.monotonic() - accepted_at):.3f}", args.log)

        def ticker(duration):
            """Send a byte every 200 ms for `duration`; the unplug must stop delivery."""
            deadline = time.monotonic() + duration
            sent = 0
            while time.monotonic() < deadline:
                try:
                    stream.sendall(b"T")
                except OSError:
                    break
                sent += 1
                time.sleep(0.2)
            log(f"host: ticks sent={sent}", args.log)

        ticks = threading.Thread(target=ticker, args=(3.0,), daemon=True)
        ticks.start()
        # One immediate tick, then the unplug. The guest starts reading ticks right after its
        # stream; 1 s is enough for it to observe a live path before the link drops.
        try:
            stream.sendall(b"T")
        except OSError:
            pass
        time.sleep(1.0)

        unplug_started = time.monotonic()
        if not qmp_ready.wait(30.0):
            raise RuntimeError("qmp connection did not become ready")
        if "stream" not in qmp_holder:
            raise RuntimeError(f"qmp unavailable: {qmp_holder.get('error')}")
        qmp_send(
            qmp_holder["stream"],
            {"execute": "set_link", "arguments": {"name": "n0", "up": False}},
        )
        unplug_ms = int((time.monotonic() - unplug_started) * 1000)
        log(
            f"host: unplug netdev=n0 ok=1 unplug_ms={unplug_ms} "
            f"t={(time.monotonic() - accepted_at):.3f}",
            args.log,
        )

        stream.settimeout(15.0)
        try:
            trailing = stream.recv(1)
        except socket.timeout:
            trailing = None
        except ConnectionResetError:
            trailing = b""
        write_result(
            args.result,
            f"ok=1 protocol={protocol} bytes={STREAM_BYTES} peer={peer[0]} "
            f"unplug=1 unplug_ms={unplug_ms} client_closed={int(trailing == b'')}",
        )
        log("host: negative peer result ok=1", args.log)
        return 0
    except (OSError, RuntimeError, ValueError) as error:
        log(f"host: failed: {error}", args.log)
        try:
            write_result(args.result, f"ok=0 reason={error}")
        except OSError:
            pass
        return 1
    finally:
        for key in ("stream", "monitor"):
            handle = qmp_holder.get(key)
            if handle is not None:
                try:
                    handle.close()
                except OSError:
                    pass
        if stream is not None:
            try:
                stream.close()
            except OSError:
                pass
        if primary is not None:
            try:
                primary.close()
            except OSError:
                pass
        for listener in backups:
            try:
                listener.close()
            except OSError:
                pass


if __name__ == "__main__":
    sys.exit(main())
