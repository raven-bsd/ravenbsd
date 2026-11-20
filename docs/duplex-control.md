<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Duplex control messages

K210 extends the K208 kernel duplex endpoint with AF_UNIX ancillary data. The kernel owns the
handle transfer and the peer identity; there is no userspace server and no shared-memory
shortcut. K211 maps these raw calls to C `sendmsg`/`recvmsg` and `SO_PEERCRED`.

## Raw calls

| Number | Call | Arguments | Result |
|---|---|---|---|
| `0x112` | `duplex_sendmsg` | `rdi = handle`, `rsi = *mut DuplexMessage` | bytes sent, or `-errno` |
| `0x113` | `duplex_recvmsg` | `rdi = handle`, `rsi = *mut DuplexMessage` | bytes received, or `-errno` |
| `0x114` | `duplex_peercred` | `rdi = handle`, `rsi = *mut DuplexCred` | `0`, or `-errno` |

`duplex_pair` (`0x110`) and `duplex_shutdown` (`0x111`) are unchanged.

## `DuplexMessage`

`DuplexMessage` is 40 bytes, little-endian, `#[repr(C)]`:

| Offset | Size | Field | Meaning |
|---:|---:|---|---|
| 0 | 8 | `data` | User data pointer |
| 8 | 8 | `data_len` | Send length; receive capacity |
| 16 | 8 | `control` | Control buffer pointer |
| 24 | 8 | `control_len` | Send length; receive capacity on entry, bytes used on return |
| 32 | 4 | `flags` | Send: `DUPLEX_MSG_NOSIGNAL = 0x4000`; receive: 0 |
| 36 | 4 | `handle_count` | Receive: number of installed handles |

`duplex_recvmsg` returns the data byte count in the syscall result. It writes the used control
length and installed handle count back into the same message. A zero-length data or control range
may use a null pointer.

## Control buffer

The control buffer is a sequence of records:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | `len` (total record bytes, at least 16) |
| 4 | 4 | `level` (`SOL_SOCKET = 1`) |
| 8 | 4 | `type` (`SCM_RIGHTS = 1`) |
| 12 | 4 | `count` (handles in this record) |
| 16 | `4 * count` | Handle ids, little-endian `u32` |

Every record length is 4-byte aligned and exactly `16 + 4 * count`. A send may contain more than
one record, but the total handle count is capped at 16 and the control bytes at 256. Unknown
levels/types, bad lengths and misaligned records return `EINVAL`; too many handles or oversized
control data return `EMSGSIZE`.

Each source handle must be present in the sender's table with `TRANSFER`. The kernel clones the
object and lineage; `recvmsg` installs independently closable duplicates in the receiver's table
and returns their handle ids in a single `SCM_RIGHTS` record. Closing either copy is independent.
Handles are not copied as naked integer fds.

## Peer credentials

`DuplexCred` is 16 bytes:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 8 | `pid` |
| 8 | 4 | `uid` |
| 12 | 4 | `gid` |

The values are stamped from the process that created the pair. Both endpoints report the same
creator identity, matching the `socketpair` peer-credential outcome. A transferred endpoint does
not rewrite that creator stamp.

## Semantics

* Stream mode: handles sent with `sendmsg` are attached to the first byte of that send. A
  zero-length send creates a control-only record. A normal `read` discards ancillary data; a
  `recvmsg` returns the handles with the associated data.
* Datagram mode: each send is one message with its own optional handles; message boundaries are
  unchanged.
* A message with no control behaves exactly like `read`/`write`. EOF, `EPIPE`, `SIGPIPE`,
  shutdown, EVQ readiness and close keep the K208 semantics. `DUPLEX_MSG_NOSIGNAL` suppresses the
  `SIGPIPE` queue for one send.
* A receive with a control buffer too small for the pending handles returns `EMSGSIZE` and leaves
  the data/message queued, so the caller can retry with a larger buffer.
* A zero-handle control record is permitted but produces no installed handles.

## K211 handoff

K211 is responsible for:

* parsing C `msghdr`/`cmsghdr` and encoding the raw records above;
* calling `duplex_sendmsg`/`duplex_recvmsg` on the endpoint handle;
* installing the returned handle ids in the process fd table and reporting `SCM_RIGHTS` cmsgs;
* mapping `SO_PEERCRED`/`getpeereid` from `duplex_peercred`;
* preserving normal send/recv, datagram boundaries, shutdown and close.

No OpenSSH source change and no Pi 4 board claim.
