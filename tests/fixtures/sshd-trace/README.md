<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# sshd session-close trace (K498)

This fixture boots the release image with the unmodified upstream OpenSSH 10.6p2 `sshd` built
from the current tree. The overlay changes only test inputs: `/etc/ssh/sshd_config` gets
`LogLevel DEBUG3` and the `sshd` service runs `-D -ddd -E /tmp/sshd-trace.log`. The product
service and binary are unchanged.

`run.sh` isolates the network path: it creates the private `ravenbrtrace`/`ravetrace0` bridge
with the host at 10.0.2.2 and boots the image with a tap. The guest publishes its static
10.0.2.15 address, so no DHCP or NAT is involved. The slirp hostfwd proxy is not used because
K382 proved it emits a FIN/RST after the seed; the K194 tap is the path the K195 gate uses.

`trace-host.py` runs the host OpenSSH client `ssh raven@10.0.2.15 'echo
sshd-trace-command-ok'` (password login) and keeps the serial console and the client log. The
first session's close state (`closed` or `close-timeout`) is the S7/S5 measurement.

Current result: the host tap connection resets before the SSH banner (`kex_exchange_identification:
read: Connection reset by peer`), while the same image accepts a guest loopback login. The full
guest log extraction is also blocked: the guest console aborts (`init exited (134)`) on large
reads and the brush line editor is too slow under host load. The report records the partial
server log from the guest-loopback run and the failure states; the ticket is BLOCKED on the tap
session reset and the guest `poll: Permission denied` client failure.

Artifacts: `host-client1.log`, `host-serial.log`, `host-summary.txt`, and the retained partial log
in the report. The fixture does not patch sshd and does not touch `tests/ssh/**` or the shared
`ravenbr0`/`ravendemo0`.
