#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""Host-side SSH login over the K227 guest network, no hostfwd substitution.

Runs three sessions as the unprivileged demo account and prints the wall time for each:

* `ssh luser@<guest> ls` returns success;
* `ssh -tt luser@<guest>` reaches the shell, runs `pwd`, and exits;
* a wrong password is refused and never reaches a shell.
"""

from __future__ import annotations

import os
import sys
import time

import pexpect

HOST = os.environ.get("SSH_GUEST_HOST", "")
PORT = os.environ.get("SSH_GUEST_PORT", "22")
USER = "luser"
PASSWORD = "raven"


def ssh_base() -> list[str]:
    return [
        "ssh",
        "-o", "StrictHostKeyChecking=no",
        "-o", "UserKnownHostsFile=/dev/null",
        "-o", "PreferredAuthentications=password",
        "-o", "PubkeyAuthentication=no",
        "-o", "NumberOfPasswordPrompts=1",
        "-o", "ConnectTimeout=10",
        "-p", PORT,
        f"{USER}@{HOST}",
    ]


def timed(label: str, fn) -> None:
    started = time.monotonic()
    fn()
    elapsed = int((time.monotonic() - started) * 1000)
    print(f"ssh-guest-login: {label} ok {elapsed} ms")


def command_session() -> None:
    child = pexpect.spawn(
        ssh_base()[0], ssh_base()[1:] + ["ls"], encoding="utf-8", timeout=30
    )
    child.logfile = sys.stdout
    child.expect(r"(?i)password:\s*")
    child.sendline(PASSWORD)
    child.expect(pexpect.EOF)
    child.close()
    if child.exitstatus != 0:
        raise RuntimeError(f"ssh command exit {child.exitstatus}")


def interactive_session() -> None:
    args = ssh_base()[1:]
    # `-tt` after the options and before the destination.
    args.insert(args.index(f"{USER}@{HOST}"), "-tt")
    child = pexpect.spawn(ssh_base()[0], args, encoding="utf-8", timeout=30)
    child.logfile = sys.stdout
    child.expect(r"(?i)password:\s*")
    child.sendline(PASSWORD)
    child.expect(r"raven[# ]")
    child.sendline("echo luser-ok")
    child.expect(r"luser-ok")
    child.sendline("exit")
    child.expect(pexpect.EOF)
    child.close()
    if child.exitstatus not in (0, None):
        raise RuntimeError(f"ssh -tt exit {child.exitstatus}")


def wrong_password_session() -> None:
    child = pexpect.spawn(
        ssh_base()[0], ssh_base()[1:] + ["ls"], encoding="utf-8", timeout=30
    )
    child.logfile = sys.stdout
    child.expect(r"(?i)password:\s*")
    child.sendline("not-the-password")
    child.expect(r"(?i)(permission denied|authentication failed)")
    child.expect(pexpect.EOF)
    child.close()
    if child.exitstatus == 0:
        raise RuntimeError("wrong password was accepted")


def main() -> int:
    if not HOST:
        print("ssh-guest-login: SSH_GUEST_HOST is not set; waiting for K227", file=sys.stderr)
        return 2
    timed("command", command_session)
    timed("interactive", interactive_session)
    timed("wrong-password", wrong_password_session)
    print("ssh-guest-login: ok")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (pexpect.EOF, pexpect.TIMEOUT, RuntimeError) as error:
        print(f"ssh-guest-login: failed: {error}", file=sys.stderr)
        sys.exit(1)
