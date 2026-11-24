#!/bin/brush
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K547 guest-loopback control. The console starts this script through
# `/bin/brush` (the kernel executes ELF only, not shebang scripts) so the probe
# line stays short. The askpass helper supplies the image password. The host tap
# client and this loopback client run in the same boot against the same sshd.

sleep 2
DISPLAY=:0 SSH_ASKPASS=/bin/sshd-trace-askpass SSH_ASKPASS_REQUIRE=force \
    /bin/ssh -p 22 \
    -o ConnectTimeout=10 \
    -o ConnectionAttempts=1 \
    -o StrictHostKeyChecking=no \
    -o UserKnownHostsFile=/dev/null \
    -o PreferredAuthentications=password \
    -o PubkeyAuthentication=no \
    raven@127.0.0.1 'echo sshd-trace-loop-ok'
echo "sshd-trace-loop: exit=$?"
