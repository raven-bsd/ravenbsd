#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Prove the release-login password prompt match against the captured interleaving.

The rev 250 gate-2 serial had ``Password: svcd: sshd up`` on one line: the prompt has no
trailing newline and the service line follows it.  The case must match the prompt token at
line start without requiring end-of-line, while a serial with no password line must still fail.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
CASE = ROOT / "tests/qtest/cases/release-login.toml"

# Exact excerpt captured by the rev 250 gate-2 run (report K555 rev 250).  The original
# build/qtest/release-login.log was truncated by the rev 251 pre-build, so the excerpt is
# carried here verbatim for the regression check.
CAPTURED = "login: root\nPassword: svcd: sshd up\n"
NO_PROMPT = "login: root\nWelcome to RavenBSD 0.1 x86_64\n"


def patterns() -> list[tuple[str, str]]:
    found: list[tuple[str, str]] = []
    for line in CASE.read_text().splitlines():
        stripped = line.strip()
        for label, key in (("expect", "regex = "), ("step", "wait = ")):
            if stripped.startswith(key):
                value = stripped[len(key) :].strip().strip("'\"")
                if "password" in value.lower():
                    found.append((label, value))
    return found


def main() -> int:
    found = patterns()
    if len(found) < 2:
        print(f"release-login prompt: FAIL found {len(found)} password patterns, expected 2")
        return 1
    for label, pattern in found:
        if re.search(pattern, CAPTURED, re.MULTILINE) is None:
            print(
                f"release-login prompt: FAIL {label} pattern does not match the captured "
                f"interleaved line: {pattern}"
            )
            return 1
        if re.search(pattern, NO_PROMPT, re.MULTILINE) is not None:
            print(
                f"release-login prompt: FAIL {label} pattern matches a serial without the "
                f"prompt: {pattern}"
            )
            return 1
    print("release-login prompt: PASS (interleaved capture matches; no-prompt serial rejected)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
