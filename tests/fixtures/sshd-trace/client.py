#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K498 host-side SSH client for the sshd session-close trace.

The client connects through the QEMU user-net host forward on 127.0.0.1:2498,
sends the image password, runs one command that exits, and waits for the server
to close. A close timeout is recorded, not hidden; the trace is the server log.
"""

from __future__ import annotations

import os
import sys

import pexpect

HOST = os.environ.get("SSHD_TRACE_HOST", "127.0.0.1")
PORT = os.environ.get("SSHD_TRACE_PORT", "2498")
LOG = os.environ.get("SSHD_TRACE_CLIENT_LOG", "build/fixtures/sshd-trace/client.log")


def main() -> int:
    log = open(LOG, "w", encoding="utf-8")
    child = pexpect.spawn(
        "ssh",
        [
            "-p", PORT,
            "-o", "StrictHostKeyChecking=no",
            "-o", "UserKnownHostsFile=/dev/null",
            "-o", "PreferredAuthentications=password",
            "-o", "PubkeyAuthentication=no",
            "-o", "ConnectTimeout=10",
            "-o", "NumberOfPasswordPrompts=1",
            f"raven@{HOST}",
            "echo sshd-trace-command-ok",
        ],
        encoding="utf-8",
        timeout=30,
    )
    child.logfile = log
    state = "no-password"
    try:
        child.expect(r"(?i)password:\s*")
        child.sendline("raven")
        child.expect("sshd-trace-command-ok", timeout=30)
        state = "command-ok"
        child.expect(pexpect.EOF, timeout=15)
        state = "closed"
    except pexpect.TIMEOUT:
        if state == "command-ok":
            state = "command-ok-close-timeout"
    except pexpect.EOF:
        pass
    finally:
        child.close(force=True)
        log.close()
    print(f"sshd-trace-client: {state}", flush=True)
    return 0 if state.startswith("command-ok") else 2


if __name__ == "__main__":
    sys.exit(main())
