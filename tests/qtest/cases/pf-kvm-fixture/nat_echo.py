#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K296: one-shot UDP NAT echo. It replies with the source port it observed so the guest
# probe can prove the pf `nat` rule translated the source port. Used only by pf-kvm*.

import socket
import sys

port = int(sys.argv[1])
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(("0.0.0.0", port))
_data, addr = sock.recvfrom(65535)
print("nat-echo", addr, flush=True)
sock.sendto(("nat-port=%d" % addr[1]).encode(), addr)
