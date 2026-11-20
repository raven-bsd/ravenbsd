# AF_UNIX socketpairs

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: DONE for unnamed `AF_UNIX` pairs. The target `UnixBackend` uses the K208 kernel duplex
endpoint pair, each endpoint is one handle, and the existing one-handle-per-OFD fd table installs
both handles without a shim. Named `bind`/`connect`, `SCM_RIGHTS` and `SO_PEERCRED` remain out of
scope; K195 consumes the pair for `sshd` privilege separation.

## 1. Why

OpenSSH's privilege separation uses an unnamed `AF_UNIX` pair:

```c
socketpair(AF_UNIX, SOCK_STREAM, 0, sv);
```

The parent keeps one end for the monitor protocol and passes the other to the unprivileged child.
The pair must be full duplex: each end writes and reads, a close by one peer gives the other end
EOF, and `shutdown`/`close` release the directions independently. OpenSSH does not use path
binding or `SCM_RIGHTS` for this channel.

## 2. Contract

For one pair:

* `socketpair(AF_UNIX, SOCK_STREAM, 0, sv)` returns two descriptors.
* `write`/`send` on `sv[0]` is read by `sv[1]`, and the reverse direction works at the same time.
* `SOCK_DGRAM` preserves message boundaries; a read returns exactly one written datagram.
* Closing the last descriptor of one end makes the peer's next `read` return `0` (EOF); a
  `write` to a fully closed peer fails `EPIPE` (and raises `SIGPIPE` unless `MSG_NOSIGNAL`).
* `shutdown(sv[i], SHUT_WR)` makes the peer see EOF while the caller can still read; `SHUT_RD`
  discards the caller's receive direction; `SHUT_RDWR` does both.
* `close` is per descriptor; `dup`/`fork` share the same open description. The pair survives as
  long as at least one descriptor of each end is live.
* `poll`/`select` can wait for readability, writability and HUP.

The C ABI path is:

```text
socketpair(2)
  -> raven_libc::socket::socketpair
  -> Net::socketpair(domain, type, proto)
  -> UnixBackend::pair_flags(type, nonblock, cloexec) -> (Sock, Sock)
  -> SocketFds::install(Some(sock)) for each end
```

`Net::sendmsg`/`Net::recvmsg`/`shutdown` select the `UnixBackend` for records whose `domain` is
`AF_UNIX`; `AF_INET`/`AF_INET6` keep using `NetBackend`. `close` goes through the ordinary
`Platform` fd table, which closes the endpoint handle and lets the kernel update the peer's
EOF/EPIPE state.

## 3. Kernel primitive

K208 adds a first-party duplex endpoint object (`kernel/src/process/duplex.rs`,
`docs/duplex-endpoint.md`). It is not a userspace server.

| Number | Call | Arguments | Result |
|---|---|---|---|
| `0x110` | `duplex_pair` | `rdi = *mut [u32; 2]`, `rsi = flags` | `0`, or `-errno` |
| `0x111` | `duplex_shutdown` | `rdi = handle`, `rsi = how` | `0`, or `-errno` |

Both handles carry `READ | WRITE | DUP | TRANSFER`; either endpoint can read and write. The flags
word selects datagram mode (`0x1`), `O_NONBLOCK` (`0o4000`) and `O_CLOEXEC` (`0o2000000`). The
ordinary `READ`/`WRITE` syscalls move bytes or one datagram. `SHUT_RD`, `SHUT_WR` and `SHUT_RDWR`
are `0`, `1` and `2`.

The kernel tracks the two side lifetimes across `HANDLE_DUP`, `HANDLE_TRANSFER`, `HANDLE_COPY`,
`PROC_FORK`, `CLOSE` and the exec sweep. `CLOSE` (`0x64`) drops a pipe or duplex endpoint
directly (there is no userspace server to notify), so libc `close(2)` reaches the endpoint
`Drop`. The last handle of one side gives the peer EOF and `EPIPE`; a duplex endpoint is also an
`EVQ_CTL` source with READABLE, WRITABLE and HUP.

## 4. Libc mapping

| Piece | Location | Behavior |
|---|---|---|
| Seam | `raven-libc-net/src/unix.rs` | `pair`, `pair_flags`, `read`, `peek`, `write`, `shutdown`, `wait`; `UnixShutdown` carries the POSIX selector. The defaults delegate to `NetBackend`, so the host fake keeps working; the target overrides the pair and shutdown. |
| Target backend | `raven-libc/src/socket/duplex.rs` | `ShellUnix` calls `0x110`/`0x111`, forwards `SOCK_DGRAM`/`SOCK_NONBLOCK`/`SOCK_CLOEXEC` in the pair flags, and maps `EINVAL`/`ENOMEM`/`EPIPE` unchanged. |
| Shell wiring | `raven-libc/src/socket/mod.rs` | `ShellNet` is `Net<..., ShellUnix>`; the process-wide object is built with `Net::with_unix(..., Some(ShellUnix))`. |
| Pair install | `raven-libc-net/src/socket.rs` | `Net::socketpair` installs each returned `Sock` through the existing `FdGlue`/`Platform` one-OFD path. There is no virtual token and no side table. |
| Data routing | `raven-libc-net/src/socket.rs` | `sendmsg`/`recvmsg`/`shutdown` call `self.unix` when `rec.domain == AF_UNIX`; otherwise the `net:` backend path is unchanged. |
| Addresses | `raven-libc-net/src/socket.rs` | `getsockname`/`getpeername` return an unnamed `sockaddr_un` (`AF_UNIX`, empty path) for a connected pair; `getpeername` on an unconnected pair is `ENOTCONN`. |
| Readiness | `raven-libc-platform/src/poll/poll.rs` | `poll` registers the endpoint handle with EVQ directly, so READABLE/WRITABLE/HUP work without a libc-side waiter. |
| Lifecycle | `raven-libc-platform` fd table | `close(2)` calls `CLOSE`, `dup(2)` shares the OFD, and `fork` clones the handle table; the last live descriptor of a side produces peer EOF/`EPIPE`. |

The fd table never sees two pipes pretending to be one socket: each `AF_UNIX` descriptor is one
kernel duplex endpoint handle, and `close`/`dup`/`fork` keep their ordinary meaning.

## 5. Tests

The `socketpair` qtest (`tests/qtest/cases/socketpair.toml`) links the real `libc.a` and runs:

1. `write(sv[0])` -> `read(sv[1])` and `write(sv[1])` -> `read(sv[0])`.
2. `shutdown(sv[0], SHUT_WR)` -> `read(sv[1]) == 0`; a peer close makes `send` fail `EPIPE`.
3. `shutdown(sv[0], SHUT_RD)` -> local read is `0` and the peer write is `EPIPE`.
4. `shutdown(sv[0], SHUT_RDWR)` -> local read is `0` and local write is `EPIPE`.
5. `SOCK_DGRAM`: two writes, two reads, boundaries intact.
6. `dup(fd)`, close the original, then write/read and observe EOF after the peer closes.
7. `fork`: the child writes an inherited endpoint, the parent reads, and the child exit gives
   the parent EOF; the parent reaps the child.

The K208 kernel fixture (`duplex-endpoint`) covers the raw ABI, including `dup`, `transfer`, EOF,
`EPIPE`, all shutdown selectors, datagrams, EVQ readiness and `CLOSE`.

## 6. Out of scope

* Named `bind`/`connect` still return `EAFNOSUPPORT` (`RD-78`); no `uds:` server is added.
* `SCM_RIGHTS` and `SO_PEERCRED` need the PG-43 request-attachment and identity work.
* `AF_INET`/`AF_INET6` behavior is unchanged.
* No SSH service or login is claimed; K195 consumes the pair.

## 7. Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| AF_UNIX pair | `socketpair`/STREAMS behavior only; CDDL, no copy | FreeBSD/NetBSD `unp`/`socketpair` behavior; no code copied | `AF_UNIX` outcome only | first-party over the K208 kernel duplex endpoint; no donor code. |

No donor code is copied. Illumos and Linux are behavior references only; FreeBSD/NetBSD are
behavior references for the POSIX outcome, not a source to translate.
