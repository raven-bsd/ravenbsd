<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# sshd session-close trace (K498)

This fixture boots the release image with the unmodified upstream OpenSSH 10.6p2 `sshd` built
from the current tree. The overlay changes only test inputs: `/etc/ssh/sshd_config` gets
`LogLevel DEBUG3`, the `sshd` service runs `-D -ddd -E /tmp/sshd-trace.log`, and
`/etc/ssh/askpass.sh` is installed 0755 for the guest client.

The qtest drives one password login and one command over guest loopback:

1. the root console starts `/etc/ssh/trace-client.sh` in the background; the script waits 2 s,
   then runs `/bin/ssh ... raven@127.0.0.1 'echo sshd-trace-command-ok'` with
   `SSH_ASKPASS_REQUIRE=force` so the image password comes from the askpass helper and no
   `/dev/tty` prompt is needed. The long command lives in the script, so the brush line editor
   only redraws the short launcher line;
2. the console sleeps 12 s, then prints `/tmp/client.log` and the full
   `/tmp/sshd-trace.log` between the `sshd-trace: client-log`/`sshd-trace: log-end` markers.
   The script writes `sshd-trace-client: exit=<n>` only when the client returns; a missing exit
   marker is part of the close trace.

The qtest serial log is the artifact. The report quotes the six-step session-close sequence and
the first step the server loop does not reach, with the OpenSSH source expectation and the exact
log line. The fixture is a trace: it does not patch sshd and does not touch `tests/ssh/**`.

The guest has no `/bin/chmod`, so the overlay tool installs the askpass helper with mode 0755.
The OpenSSH binaries are built by `sh addons/ssh/build.sh`; `build.sh` here verifies them,
builds the release initfs when it is missing, overlays the three test files and wraps the
overlay initfs in `build/fixtures/sshd-trace/sshd-trace.img`.
