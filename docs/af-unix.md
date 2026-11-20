# AF_UNIX socketpairs

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: BLOCKED on a kernel duplex-endpoint primitive. This file records the contract the libc
`AF_UNIX` backend must satisfy, the primitives that exist in the tree, and the exact syscall/ABI
gap. It is the K206 design note; no AF_UNIX endpoint is claimed to work.

## 1. Why

OpenSSH's privilege separation uses an unnamed `AF_UNIX` pair:

```c
socketpair(AF_UNIX, SOCK_STREAM, 0, sv);
```

The parent keeps one end for the monitor protocol and passes the other to the unprivileged child.
The pair must be full duplex: each end writes and reads, a close by one peer gives the other end
EOF, and `shutdown`/`close` release the directions independently. OpenSSH does not use path
binding or `SCM_RIGHTS` for this channel.

`raven-libc-net` already has the seam (`UnixBackend`, `unix.rs`) and the C `socketpair` wrapper
(`raven-libc/src/socket/socketpair.rs`). The target shell instantiates `Net<..., NoUnix>`, so
`pair` returns `EAFNOSUPPORT (97)` before any data path is reached.

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
  -> UnixBackend::pair(type) -> (Sock, Sock)
  -> SocketFds::install(Some(sock)) for each end
```

Data then flows through `Net::sendmsg`/`Net::recvmsg`/`shutdown`, which must select the AF_UNIX
backend for a record whose `domain` is `AF_UNIX`; `AF_INET`/`AF_INET6` keep using `NetBackend`.

## 3. What exists

| Primitive | Location | What it provides | Why it is not enough |
|---|---|---|---|
| `PIPE` / `PIPE2` | `kernel/src/syscall/pipe.rs:19-21` (raw `0x6e`/`0x6f`) | One 64 KiB ring and two **one-directional** handles: a reader and a writer | `read` on the writer and `write` on the reader are `EBADF` (`kernel/src/process/pipe.rs:187-188`, `241-242`). A `socketpair` end must be one handle that is both readable and writable. |
| Pipe drops/EOF | `kernel/src/process/pipe.rs` | Last write handle closed -> reader EOF; last read handle closed -> writer `EPIPE` | Two pipes per endpoint still need one handle per endpoint and a close hook; the fd table owns one handle per OFD. |
| `EVQ_*` | `crates/raven-abi/src/syscall.rs:128-136` | Readability/writability/HUP readiness for an event source | There is no duplex endpoint object to register. |
| Scheme packet router | `SCHEME_REGISTER`, `RECV`, `SEND` (`crates/raven-abi/src/syscall.rs:145-164`) | Request/reply packets to a registered userspace server | The ticket forbids a userspace process server, and this is not a byte-stream pair. |
| `raven-chan` | `crates/raven-chan`, `docs/ipc.md` | Pure call/reply/state model | It is a model only: no kernel `Channel` object, no syscall number, no payload bytes (`docs/ipc.md` section 8 says the kernel object and syscalls are later KN-102/103 work). |

The fd layer makes a two-pipe shim unworkable without a kernel object:

* `Net::socketpair` installs each returned `Sock` through `SocketFds::install`
  (`raven-libc-net/src/socket.rs:93-118`).
* `FdGlue::install` treats `Sock.0` as exactly one kernel `Handle` and creates one
  `OfdKind::Socket` OFD (`raven-libc-net/src/fdglue.rs:34-38`).
* An `Ofd` stores exactly one `Handle` (`raven-libc-platform/src/fd/ofd.rs:45-52`), and
  `Platform::close` closes that handle (`raven-libc-platform/src/platform.rs:401-411`).
* `Net::close_hook` only calls `backend.close(sock)` (`raven-libc-net/src/socket.rs:859-866`), and
  the C `close(2)` goes straight to `Platform::close`; there is no AF_UNIX close hook.
* `Net::sendmsg`, `Net::recvmsg`, `Net::shutdown` and `wait_one` use `self.backend`
  unconditionally (`raven-libc-net/src/socket.rs:496`, `589`, `655`, `907`), so AF_UNIX data
  operations also need a routing change once a real backend exists.

A shim that installs one pipe handle as the descriptor and keeps the other direction in a global
map would leak the second handle on `close`, would not share it across `dup`/`fork`, would not
produce EOF when the last descriptor of the peer end closes, and would not support `poll`. That
is the "single file descriptor" fake the ticket forbids.

## 4. Exact missing ABI

The kernel needs one operation that creates two connected, duplex endpoint handles. No such
variant exists in `crates/raven-abi/src/syscall.rs`; the `0x110..=0x11F` native IPC range is
documented as future work and is unallocated.

Proposed contract (the reviewer owns the final number and name):

```text
channel_create(flags: u32, out: *mut [u32; 2]) -> 0     // 0x110, native
```

`flags` selects `SOCK_STREAM` (byte stream) or `SOCK_DGRAM` (message preserving). Each returned
handle must support:

* `READ` and `WRITE` in both directions, with peer-close EOF and `EPIPE`.
* A `SHUTDOWN` operation for read, write or both, per handle.
* `EVQ_CTL` registration of READABLE, WRITABLE and HUP.
* `HANDLE_DUP`, `HANDLE_TRANSFER` and `CLOSE` with endpoint lifetime counts (the last handle of
  one endpoint closes that direction; the peer sees EOF).
* `FSTAT`/kind reporting sufficient for `OfdKind::Socket`, or a new `OfdKind` that the shell can
  map without changing the fd table's one-handle-per-OFD invariant.
* Bounded queues and backpressure (`EAGAIN` for non-blocking endpoints).

Alternatively, a raw `socketpair(domain, type, proto, out)` call (`0x70` is free) may create the
same two endpoints; the libc side only needs `(Sock, Sock)` and the operations above.

Until that exists, `NoUnix` is the correct target backend:

```text
socketpair(AF_UNIX, SOCK_STREAM, 0, sv) -> -1, errno = 97 (EAFNOSUPPORT)
```

## 5. What K206 did and did not do

* Added the target `socketpair` qtest fixture (`tests/qtest/cases/socketpair-fixture/`) and the
  case file. The case is `skip`ped with the exact reason above until the kernel primitive lands;
  a red run of the same fixture prints `socketpair: socketpair errno=97`.
* Did not add a target `UnixBackend`, did not change `ShellNet`, and did not modify
  `addons/ssh/**`. The OpenSSH `ssh-version` smoke still passes through its `/dev/null`/`eventfd`
  test fd; it does not exercise `socketpair`.
* `AF_INET`/`AF_INET6` behavior is unchanged; no existing path was rewritten.

The qtest checks, once enabled:

1. `write(sv[0])` -> `read(sv[1])` and `write(sv[1])` -> `read(sv[0])`.
2. `shutdown(sv[0], SHUT_WR)` -> `read(sv[1]) == 0`.
3. Close one end, `write` the other -> `-1` with `EPIPE`.
4. `SOCK_DGRAM`: two writes, two reads, boundaries intact.

## 6. Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| AF_UNIX pair | `socketpair`/STREAMS behavior only; CDDL, no copy | FreeBSD/NetBSD `unp`/`socketpair` behavior; no code copied | `AF_UNIX` outcome only | first-party over a Raven duplex endpoint; no donor code. |

No donor code is copied. Illumos and Linux are behavior references only; FreeBSD/NetBSD are
behavior references for the POSIX outcome, not a source to translate.
