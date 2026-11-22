<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# sshd session-close trace (K498)

This fixture boots the release image with the unmodified upstream OpenSSH 10.6p2 `sshd` built
from the current tree. The overlay changes only test inputs: `/etc/ssh/sshd_config` gets
`LogLevel DEBUG3` and the `sshd` service runs `-D -ddd -E /tmp/sshd-trace.log`.

`run.sh` isolates the network path: it creates the private `ravenbrtrace`/`ravetrace0` bridge
with the host at 10.0.2.2 and boots the image with a tap. The guest publishes its static
10.0.2.15 address, so no DHCP or NAT is involved. The slirp hostfwd proxy is not used because
K382 proved it emits a FIN/RST after the seed; the K194 tap is the path the K195 gate uses.

`trace-host.py` runs two host OpenSSH clients:

1. `ssh raven@10.0.2.15 'echo sshd-trace-command-ok'` (password login). It records whether the
   command output arrives and whether the client then reaches EOF (`closed`) or waits past
   20 s (`close-timeout`); the timeout is the S7/S5 symptom, not a harness failure.
2. `ssh raven@10.0.2.15 'cat /tmp/sshd-trace.log'` opens a second session and copies the full
   sshd DEBUG3 log to `build/fixtures/sshd-trace/host-sshd.log`. This avoids the guest console
   for log extraction.

Artifacts: `host-client1.log`, `host-sshd.log`, `host-serial.log`, `host-summary.txt`. The
report quotes the six-step session-close sequence and the first step the server loop does not
reach, with the OpenSSH source expectation and the exact log line. The fixture does not patch
sshd and does not touch `tests/ssh/**` or the shared `ravenbr0`/`ravendemo0`.
