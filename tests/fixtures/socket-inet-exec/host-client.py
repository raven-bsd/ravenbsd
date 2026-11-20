#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Host side of the K231 exec'd accepted-socket exchange (an `sshd`-shaped session).

Connect through QEMU hostfwd, read the server's first record (a 19-byte banner followed by a
960-byte KEXINIT-sized blob), answer with a 2912-byte record, and require the 9-byte `k231-done`
reply. The server speaks first and then waits for this record with `poll`, so a client whose
record is never read, or a reply that never leaves the guest, fails here and in the guest log.

Before the first server byte arrives the guest may simply not be listening yet (hostfwd then
resets the connection), so those attempts are retried. After the banner there is no retry: a
session that started must finish.
"""

import argparse
import socket
import sys
import time

BANNER = b"SSH-2.0-RavenK231\r\n"
KEXINIT_BYTES = 960
CLIENT_RECORD = bytes(index % 251 for index in range(2912))
DONE = b"k231-done"


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


def recv_exact(stream, wanted, deadline):
    received = b""
    while len(received) < wanted:
        if time.monotonic() >= deadline:
            return received
        chunk = stream.recv(wanted - len(received))
        if not chunk:
            return received
        received += chunk
    return received


def session(host, port, args):
    """One connection. Returns None to retry, or (ok, message) once a session ran."""
    with socket.create_connection((host, port), timeout=2.0) as stream:
        stream.settimeout(10.0)
        try:
            first = recv_exact(stream, len(BANNER) + KEXINIT_BYTES, time.monotonic() + 8.0)
        except OSError:
            return None
        if not first:
            return None  # reset or closed before any byte: the guest was not listening yet
        if len(first) != len(BANNER) + KEXINIT_BYTES or not first.startswith(BANNER):
            return False, f"short or wrong server record bytes={len(first)}"
        tail = first[len(BANNER):]
        if tail != bytes(index % 251 for index in range(KEXINIT_BYTES)):
            return False, "server KEXINIT blob differs"
        stream.sendall(CLIENT_RECORD)
        try:
            done = recv_exact(stream, len(DONE), time.monotonic() + 15.0)
        except OSError as error:
            return False, f"no reply after client record: {error}"
        if done != DONE:
            return False, f"reply mismatch got={done!r}"
        return True, f"server={len(first)} client={len(CLIENT_RECORD)} done={done!r}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=23081)
    parser.add_argument("--result", required=True)
    parser.add_argument("--log", default="")
    parser.add_argument("--timeout", type=float, default=240.0)
    args = parser.parse_args()

    deadline = time.monotonic() + args.timeout
    attempt = 0
    while time.monotonic() < deadline:
        attempt += 1
        try:
            outcome = session(args.host, args.port, args)
        except OSError as error:
            if attempt % 20 == 0:
                log(f"socket-inet-exec host: retry attempt={attempt} error={error}", args.log)
            time.sleep(0.5)
            continue
        if outcome is None:
            time.sleep(0.5)
            continue
        ok, message = outcome
        log(f"socket-inet-exec host: {'ok' if ok else 'FAILED'} attempt={attempt} {message}", args.log)
        write_result(args.result, ("ok " if ok else "failed ") + message)
        return 0 if ok else 1

    log(f"socket-inet-exec host: timeout after {attempt} attempts", args.log)
    write_result(args.result, "timeout")
    return 1


if __name__ == "__main__":
    sys.exit(main())
