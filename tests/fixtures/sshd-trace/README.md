<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# sshd session-close trace (K498)

This fixture drives the release image's upstream OpenSSH 10.6p2 `sshd` with `-D -ddd` on port
2222. It starts the manual daemon from the guest root console, writes the debug log to
`/tmp/sshd-trace.log`, and starts the guest's own `/bin/ssh` against `raven@127.0.0.1:2222`
through a forced `SSH_ASKPASS` script. The client runs in the background so the console remains
available to print both logs after the command child exits.

The qtest serial log is the artifact. It frames the sshd log with `sshd-trace: log-begin` and
`sshd-trace: log-end`; the report quotes the six-step sequence and the first missing step with
the OpenSSH source expectation.

The OpenSSH binaries are the unmodified add-on from `addons/ssh`; `build.sh` verifies their
presence and uses the release image at `build/qtest/sshd-trace.img`. It does not patch sshd and
does not touch `tests/ssh/**`.
