<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# File-descriptor passing over `AF_UNIX`

OpenSSH privsep passes the pty master to the session process with `sendmsg(2)` and
`recvmsg(2)` ancillary data (`addons/ssh/openssh/monitor_fdpass.c`, `mm_send_fd`/`mm_recv_fd`;
`monitor_wrap.c`, `mm_pty_allocate`). RavenBSD implements the same POSIX `SCM_RIGHTS`
contract on the kernel duplex endpoint pair (`K210`) and in ravenlibc (`K211`).

## Contract

| Item | Value |
|---|---|
| Socket | `AF_UNIX`, `SOCK_STREAM` (the pair from `socketpair(2)`) |
| Control | `cmsg_level = SOL_SOCKET`, `cmsg_type = SCM_RIGHTS` |
| Payload | One or more native file descriptors |
| Send | `sendmsg` attaches the descriptors to the first data byte; a zero-length send is control-only |
| Receive | `recvmsg` installs an independently closable duplicate of each descriptor and reports one `SCM_RIGHTS` record |
| Truncated control | `EMSGSIZE`; the data and descriptors stay queued for a retry |
| Malformed control | `EINVAL` (bad length, unknown level/type, or a non-multiple-of-four payload) |
| No control | `msg_controllen = 0`; ordinary data is unchanged |

The kernel record and the C `cmsghdr` marshalling are documented in
[`duplex-control.md`](duplex-control.md).

## Tests

`tests/libc/c/libc-fdpass.c` is the guest fixture. It forks a receiver, matching the privsep
shape, and covers:

* a regular file: the child writes through the received descriptor and the parent reads the
  same bytes through its original descriptor;
* a pty master: the child writes through the received master and the parent reads the bytes on
  the slave;
* one and two descriptors in a single `SCM_RIGHTS` record, asserting `msg_controllen`,
  `cmsg_len` (`20` for one fd, `24` for two), the level/type, the count and the returned
  descriptor numbers;
* a too-small receive control buffer: `EMSGSIZE`, then a successful retry with the same
  descriptor still queued;
* a malformed send control buffer: `EINVAL`;
* an ordinary send with no control: `msg_controllen = 0`.

Run it with:

```sh
timeout 300 cargo xtask qtest --case fdpass --arch x86_64
```

`fdpass-tcp` adds a real TCP descriptor. The guest connects to the host side of QEMU user
networking (`10.0.2.2:23084`), passes the connected descriptor over the same `AF_UNIX`
socketpair, drops every inherited copy, and the child writes the payload through the received
descriptor. The host server verifies the exact bytes and sends an acknowledgement that the child
reads back through the received descriptor:

```sh
timeout 300 cargo xtask qtest --case fdpass-tcp --arch x86_64
```

The external sshd gate is `ssh -tt raven@<guest> tty`; the session must print
`/dev/pts/<N>`. That gate is `tests/qtest/cases/ssh-login.toml`.
