<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# sshd tap-vs-loopback trace (K547)

This fixture boots the release image with the unmodified upstream OpenSSH 10.6p2 `sshd` built
from the current tree. The overlay changes only test inputs: `/etc/ssh/sshd_config` gets
`LogLevel DEBUG3`, the `sshd` service runs `-D -ddd -E /tmp/sshd-trace.log`, the private-tap
network drop-in is static (`10.0.2.15/24`, gateway `10.0.2.2`) and the loopback control binaries
are installed.

`run.sh` creates the private `ravenbrtrace`/`ravetrace0` bridge (host `10.0.2.2`), boots the image
with the tap and a pty serial, and runs `trace-host.py`. In one boot it:

1. logs in to the guest console as root;
2. runs the host tap client `ssh raven@10.0.2.15 'echo sshd-trace-command-ok'` and records
   whether the password prompt/command/close is reached or the connection resets before the
   banner;
3. tails `/tmp/sshd-trace.log` and records whether `Connection from` appears;
4. starts the guest loopback client `/bin/brush /etc/ssh/trace-client.sh` (ELF askpass, password
   login to 127.0.0.1) as the control, then tails the same log and the loop log.

Artifacts: `host-serial.log` (both outcomes and the log tails), `host-client.log` (host client),
`host-summary.txt`. The fixture does not patch sshd and does not touch `tests/ssh/**`, the shared
`ravenbr0`/`ravendemo0`, or K548's guest `poll` finding.
