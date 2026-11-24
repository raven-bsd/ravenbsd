#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K448 SSH session termination bytes.

A stub SSH server (paramiko, host test tooling — not OpenSSH) completes the transport and auth,
opens a session, and sends a controlled termination sequence to the host OpenSSH client:

  * command output (CHANNEL_DATA);
  * exit-status (SSH_MSG_CHANNEL_REQUEST 98, request "exit-status", status 98);
  * SSH_MSG_CHANNEL_EOF (96);
  * SSH_MSG_CHANNEL_CLOSE (97);
  * socket close.

The client must print the output and exit 98 within 5 s. Negative controls omit exit-status,
omit CLOSE, send EOF without CLOSE, and send CLOSE before the output. Every channel message the
stub puts on the wire is recorded from the transport's own plaintext message buffer (the bytes are
then encrypted by the transport), so the msg id, recipient channel and exit status are parsed
byte-exactly at the message layer; the client's exit/output proves the wire delivery.

Usage: python3 tests/ssh/session-termination.py
Exit: 0 when every case behaves as asserted, 1 otherwise, 2 for a setup problem.
"""

from __future__ import annotations

import os
import signal
import socket
import subprocess
import sys
import threading
import time

try:
    import paramiko
except ImportError:
    print("session-termination: python3-paramiko is required (host test tooling)", file=sys.stderr)
    sys.exit(2)

HERE = os.path.dirname(os.path.abspath(__file__))
ASKPASS = os.path.join(HERE, "askpass.sh")
PASSWORD = "raven"
CLIENT_LIMIT = 5.0
OUTPUT = b"hello\n"
EXIT_CODE = 98


class StubServer:
    """One stub server run: a listener, one transport, one session channel."""

    def __init__(self, mode: str) -> None:
        self.mode = mode
        self.sent: list[dict] = []
        self.listener = None
        self.port = 0
        self.ready = threading.Event()
        self.stop = threading.Event()
        self.error: str | None = None
        self.thread = threading.Thread(target=self._serve, daemon=True)

    def start(self) -> int:
        self.thread.start()
        if not self.ready.wait(5):
            raise RuntimeError("stub server did not start")
        return self.port

    def _record(self, transport: paramiko.Transport) -> None:
        original = transport._send_message

        def record(message):
            raw = message.asbytes()
            if raw and raw[0] in (96, 97, 98):
                entry = {
                    "type": raw[0],
                    "channel": int.from_bytes(raw[1:5], "big"),
                }
                if raw[0] == 98:
                    at = 5
                    length = int.from_bytes(raw[at : at + 4], "big")
                    at += 4
                    entry["request"] = raw[at : at + length].decode("ascii", "replace")
                    at += length
                    entry["want_reply"] = raw[at]
                    at += 1
                    entry["status"] = int.from_bytes(raw[at : at + 4], "big")
                self.sent.append(entry)
            return original(message)

        transport._send_message = record

    def _serve(self) -> None:
        try:
            key = paramiko.RSAKey.generate(2048)
            listener = socket.socket()
            listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            listener.bind(("127.0.0.1", 0))
            listener.listen(1)
            listener.settimeout(10)
            self.listener = listener
            self.port = listener.getsockname()[1]
            self.ready.set()
            client, _ = listener.accept()
            transport = paramiko.Transport(client)
            transport.add_server_key(key)
            self._record(transport)

            stub = self

            class Server(paramiko.ServerInterface):
                def check_auth_password(self, username, password):
                    return paramiko.AUTH_SUCCESSFUL

                def get_allowed_auths(self, username):
                    return "password"

                def check_channel_request(self, kind, chanid):
                    if kind == "session":
                        return paramiko.OPEN_SUCCEEDED
                    return paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED

                def check_channel_exec_request(self, channel, command):
                    threading.Thread(
                        target=stub._terminate, args=(channel, transport), daemon=True
                    ).start()
                    return True

            transport.start_server(server=Server())
            transport.join(CLIENT_LIMIT + 5)
            transport.close()
        except Exception as error:  # noqa: BLE001 - reported, not swallowed
            self.error = f"{type(error).__name__}: {error}"
            self.ready.set()

    def _terminate(self, channel: paramiko.Channel, transport: paramiko.Transport) -> None:
        def eof() -> None:
            # paramiko 2.9: shutdown_write() is SSH_MSG_CHANNEL_EOF (96).
            channel.shutdown_write()

        if self.mode == "positive":
            channel.send(OUTPUT)
            channel.send_exit_status(EXIT_CODE)
            eof()
            time.sleep(0.05)
            channel.close()
            time.sleep(0.05)
            transport.close()
        elif self.mode == "no-exit-status":
            channel.send(OUTPUT)
            eof()
            time.sleep(0.05)
            channel.close()
            time.sleep(0.05)
            transport.close()
        elif self.mode == "no-close":
            channel.send(OUTPUT)
            channel.send_exit_status(EXIT_CODE)
            eof()
            self.stop.wait(CLIENT_LIMIT + 5)
            channel.close()
            transport.close()
        elif self.mode == "eof-no-close":
            channel.send(OUTPUT)
            eof()
            self.stop.wait(CLIENT_LIMIT + 5)
            channel.close()
            transport.close()
        elif self.mode == "close-before-data":
            channel.close()
            try:
                channel.send(b"late\n")
            except Exception:  # noqa: BLE001 - data after CLOSE must never reach the client
                pass
            time.sleep(0.05)
            transport.close()
        else:
            self.error = f"unknown mode {self.mode}"

    def shutdown(self) -> None:
        self.stop.set()
        if self.listener is not None:
            try:
                self.listener.close()
            except OSError:
                pass
        self.thread.join(10)


def run_client(port: int) -> tuple[int | None, bytes, bytes, float, bool]:
    env = dict(os.environ)
    env["SSH_ASKPASS"] = ASKPASS
    env["SSH_ASKPASS_REQUIRE"] = "force"
    env["K434_SSH_PASSWORD"] = PASSWORD
    command = [
        "ssh",
        "-p",
        str(port),
        "-o",
        "StrictHostKeyChecking=no",
        "-o",
        "UserKnownHostsFile=/dev/null",
        "-o",
        "PreferredAuthentications=password",
        "-o",
        "PubkeyAuthentication=no",
        "-o",
        "KbdInteractiveAuthentication=no",
        "-o",
        "NumberOfPasswordPrompts=1",
        "-o",
        "ConnectTimeout=5",
        "-o",
        "LogLevel=ERROR",
        f"raven@127.0.0.1",
        "true",
    ]
    started = time.monotonic()
    process = subprocess.Popen(
        command,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        start_new_session=True,
    )
    timed_out = False
    try:
        stdout, stderr = process.communicate(timeout=CLIENT_LIMIT)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(process.pid, signal.SIGKILL)
        stdout, stderr = process.communicate()
    elapsed = time.monotonic() - started
    exit_code = None if timed_out else process.returncode
    return exit_code, stdout, stderr, elapsed, timed_out


def check(mode: str, exit_code, stdout, stderr, elapsed, timed_out, sent, error) -> tuple[bool, str]:
    ids = [(entry["type"], entry["channel"]) for entry in sent]
    statuses = [entry for entry in sent if entry["type"] == 98]
    channel_values = {entry["channel"] for entry in sent}
    summary = (
        f"mode={mode} exit={exit_code} timed_out={timed_out} wall_ms={elapsed * 1000:.0f} "
        f"stdout={stdout!r} sent={ids}"
    )
    if error:
        return False, summary + f" server_error={error}"
    if mode == "positive":
        ok = (
            not timed_out
            and exit_code == EXIT_CODE
            and stdout == OUTPUT
            and elapsed <= CLIENT_LIMIT
            and 98 in [entry["type"] for entry in sent]
            and 96 in [entry["type"] for entry in sent]
            and 97 in [entry["type"] for entry in sent]
            and len(statuses) == 1
            and statuses[0].get("request") == "exit-status"
            and statuses[0].get("status") == EXIT_CODE
            and len(channel_values) == 1
        )
        return ok, summary
    if mode == "no-exit-status":
        # Without exit-status the client must not report success.
        ok = not timed_out and exit_code not in (None, 0) and stdout == OUTPUT
        return ok, summary
    if mode in ("no-close", "eof-no-close"):
        # EOF (with or without exit-status) is not a session close: the client must still wait.
        ok = timed_out and exit_code is None
        return ok, summary
    if mode == "close-before-data":
        # CLOSE before the output: the late bytes must never be delivered as the command output.
        ok = b"late" not in stdout and exit_code not in (None, 0)
        return ok, summary
    return False, summary + " unknown mode"


def main() -> int:
    modes = [
        "positive",
        "no-exit-status",
        "no-close",
        "eof-no-close",
        "close-before-data",
    ]
    failures = 0
    for mode in modes:
        server = StubServer(mode)
        try:
            port = server.start()
            exit_code, stdout, stderr, elapsed, timed_out = run_client(port)
        finally:
            server.shutdown()
        ok, summary = check(
            mode, exit_code, stdout, stderr, elapsed, timed_out, server.sent, server.error
        )
        print(("PASS " if ok else "FAIL ") + summary)
        if not ok and stderr:
            print(f"      stderr={stderr[-160:]!r}")
        if not ok:
            failures += 1
    print(f"session-termination: cases={len(modes)} failures={failures}")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
