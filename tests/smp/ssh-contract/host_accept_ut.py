#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Unit test for the QEMU `info usernet` host-forward parser."""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import host_accept

SAMPLE = """Hub -1 (n0):
  Protocol[State]    FD  Source Address  Port   Dest. Address  Port RecvQ SendQ
  TCP[HOST_FORWARD]   8       127.0.0.1 40261       10.0.2.15 23097     0     0
  TCP[HOST_FORWARD]   3       127.0.0.1 36709       10.0.2.15 23098     0     0
(qemu) """


def main():
    ports = host_accept.parse_usernet(SAMPLE)
    assert ports == {23097: 40261, 23098: 36709}, ports
    assert host_accept.parse_usernet("no forwards\n") == {}
    print("host_accept_ut: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
