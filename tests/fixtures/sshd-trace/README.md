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

## Loopback-only close trace (K547 rev 72)

`tests/fixtures/sshd-trace/assert-loopback.py` needs no tap and no NIC. It boots the image, logs in,
starts the guest loopback client through the ELF askpass, and runs `/bin/sshd-trace-logdump`, which
prints only the close-sequence lines through the kernel debug writer (the full DEBUG3 `cat` aborts
the guest console, K498). The expected measured order is:

1. `Starting session: command for raven` - `session_input_channel_req`/exec.
2. `channel 0: read 293 from` and `channel 0: sent ext data 293` - the measured 293 bytes.
3. `Received SIGCHLD.` - `serverloop.c:324 collect_children`.
4. `session_by_pid: pid` - `session.c:1836`.
5. `session_exit_message: session 0 channel 0 pid ... exit 0` - `session.c:2349/2374`, the
   `exit-status` request is sent here.
6. `channel 0: send eof` - `nchan.c:186`.
7. `channel 0: send close` - `nchan.c:219`.
8. `Close session: user raven` - `session.c:2398 session_close`.

The run depends on K548: the kernel `EVQ_CTL` source-rights gate (`kernel/src/syscall/event.rs:323`,
`kernel/src/event/mod.rs:204`) still returns EACCES for the write-only session source, so the client
dies with `poll: Permission denied` and `exit=255` before step 3. The script reports
`BLOCKED-K548` in that state and never fakes a pass.

Commands after K548 and the measurement window:

```
timeout 20 python3 tests/fixtures/sshd-trace/assert-loopback.py --selftest
timeout 300 sh tests/fixtures/sshd-trace/build.sh
timeout 600 python3 tests/fixtures/sshd-trace/assert-loopback.py
```

`--selftest` exercises the matcher without a boot: the full nine-line sequence passes, a missing
`send eof` step reports `eof`, and a `send close`/`send eof` swap reports `close`.

The script writes `loopback-serial.log` and `loopback-summary.txt` and exits 0 only when all nine
ordered steps are present, 2 for the K548/client block, 1 for a missing step.
