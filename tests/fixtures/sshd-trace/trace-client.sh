#!/bin/brush
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# K498 guest loopback SSH client. The console starts this script through
# `/bin/brush` (the kernel executes ELF only, not shebang scripts) so the probe
# command line stays short. The askpass helper supplies the image password; the
# script stays alive while the client waits for the server to close the
# session, and the missing exit marker is itself trace evidence.

sleep 2
DISPLAY=:0 SSH_ASKPASS=/bin/sshd-trace-askpass SSH_ASKPASS_REQUIRE=force \
    /bin/ssh -p 22 \
    -o StrictHostKeyChecking=no \
    -o UserKnownHostsFile=/dev/null \
    -o PreferredAuthentications=password \
    -o PubkeyAuthentication=no \
    raven@127.0.0.1 'echo sshd-trace-command-ok'
echo "sshd-trace-client: exit=$?"
