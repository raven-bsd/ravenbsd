<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# PTY and the privsep exchange

RavenBSD's pty support spans the `ptyd` scheme, ravenlibc's pty calls, and the OpenSSH
privsep exchange. The SSH server is the unmodified upstream OpenSSH 10.6p2 add-on
(`addons/ssh/openssh/`); no SSH code lives in the base tree.

## Layers

| Layer | Location | Contract |
|---|---|---|
| pty scheme | `userland/servers/ptyd/` | One line discipline and two byte queues per pair; `pty:master[/N]` and `pty:slave[/N]`; `SET_MODE`/`GET_MODE`/`FLUSH`, foreground group and hangup |
| libc pty calls | `libc/ravenlibc/crates/raven-libc/src/compat/` | `posix_openpt`, `grantpt`, `unlockpt`, `ptsname`, `ptsname_r`, `openpty` |
| identity | `ptyd` `fstat` | Pair `N` master inode `2N+1`, slave inode `2N+2`; both non-zero, role-distinct |
| privsep exchange | OpenSSH `monitor_wrap.c`/`monitor.c`; test `tests/libc/c/libc-pty-monitor.c` | Monitor allocates the pty and returns the master/slave descriptors plus the name; the session opens the slave by name |

## Privsep wire shape

The `pty-monitor` guest test implements the monitor side over `socketpair(AF_UNIX)`:

1. Session: 4-byte big-endian length `1`, type `MONITOR_REQ_PTY` (28).
2. Monitor: `posix_openpt`/`grantpt`/`unlockpt`/`ptsname`, then `open` the slave.
3. Monitor: 4-byte length, type `MONITOR_ANS_PTY` (29), payload
   `success` (`u32`), `name` (cstring), login message (cstring), then the test-only
   master/slave `dev`/`ino` identity fields.
4. Monitor: two `monitor_fdpass`-style messages (one data byte plus one `SCM_RIGHTS` cmsg):
   the master, then the slave.
5. Session: verifies `cmsg_len`, `SOL_SOCKET`, `SCM_RIGHTS`, the count and `msg_controllen`,
   opens the slave by name, checks `fstat` inode identity, and round-trips a byte each way.
6. Session: `MONITOR_REQ_PTYCLEANUP` (30); the monitor closes the pair and exits.

Run it with:

```sh
timeout 300 cargo xtask qtest --case pty-monitor --arch x86_64
```

The test also includes three negative exchanges: the monitor drops the master fd, the reply
names a different pty (the opened slave's inode does not match the passed slave), and the
request omits its type byte.

## External SSH gate

`ssh -tt raven@<guest> tty` must print `/dev/pts/<N>`. The host gate is
`tests/qtest/cases/ssh-login.toml` (K195) with the release image built from
`cargo xtask image --arch x86_64 --profile release --output build/qtest/ssh-login.img`.
The guest-side `pty-monitor` test isolates the exchange itself; the host gate exercises the
same path through the real OpenSSH monitor.
