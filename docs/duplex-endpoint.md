<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Kernel duplex endpoint ABI

K208 adds a first-party kernel object for an AF_UNIX-style socket pair. It is not a userspace
server (DR-46): the two endpoint handles are ordinary entries in the caller's handle table and
the kernel owns the queues, blocking and lifetime.

The raw numbers live in the unallocated native range until the reviewer promotes the contract
into `crates/raven-abi`.

## Raw calls

| Number | Call | Arguments | Result |
|---|---|---|---|
| `0x110` | `duplex_pair` | `rdi = *mut [u32; 2]`, `rsi = flags` | `0`, or `-errno` |
| `0x111` | `duplex_shutdown` | `rdi = handle`, `rsi = how` | `0`, or `-errno` |

`duplex_pair` writes two 32-bit handle values in little-endian order: the first is the left
endpoint, the second is the right endpoint. Both handles carry `READ | WRITE | DUP | TRANSFER`.
Either endpoint can read and write; direction is selected by the call (`READ`/`WRITE`), not by
the handle.

`duplex_shutdown` uses the POSIX selectors: `SHUT_RD = 0`, `SHUT_WR = 1`, `SHUT_RDWR = 2`.
Unknown selectors or flags return `EINVAL`.

## Flags

| Bit | Name | Meaning |
|---:|---|---|
| `0x1` | `DUPLEX_DATAGRAM` | Message-preserving datagram mode instead of byte stream |
| `0o4000` | `O_NONBLOCK` | Both endpoints return `EAGAIN` instead of blocking |
| `0o2000000` | `O_CLOEXEC` | Both handles are closed by a future exec sweep |

Unknown bits return `EINVAL`.

## Data path

* **Stream** mode is a 64 KiB FIFO in each direction. A blocking write fills the peer's ring and
  waits; a non-blocking write returns the bytes that fitted, or `EAGAIN` when none did. A read
  returns at most the caller's buffer length.
* **Datagram** mode keeps a bounded FIFO of complete messages (64 KiB total, at most 64
  messages, at most 64 KiB per message). A write queues the whole message or fails; a read
  consumes one message. If the read buffer is shorter, the returned prefix is copied and the
  whole message is consumed.
* Closing the last handle of one endpoint makes the peer read EOF and makes subsequent peer
  writes fail with `EPIPE` (a `SIGPIPE` is queued for `PROC_SIGRECV`). `SHUT_RD` makes local reads
  return 0 and peer writes fail with `EPIPE`; `SHUT_WR` makes local writes fail with `EPIPE` and
  peer reads return 0; `SHUT_RDWR` does both.
* `HANDLE_DUP`, `HANDLE_TRANSFER`, `HANDLE_COPY`, `PROC_FORK`, `CLOSE` and the exec
  close-on-exec sweep all operate on the endpoint `Arc`; the kernel tracks the two side
  lifetimes so the EOF/EPIPE transition happens when the last handle for a side disappears.

## EVQ readiness

A duplex endpoint is an `EVQ_CTL` source. It reports:

* `READABLE` when incoming data is queued, the local read side is shut down, or the peer has
  closed/shut its write side.
* `WRITABLE` when the local write side is open, the peer read side is open, and the outgoing
  queue has room.
* `HUP` when the peer has closed or shut its write side, or the local read side is shut down.

## Errors

| Error | Condition |
|---|---|
| `EINVAL` | Unknown flags or shutdown selector |
| `ENOMEM` | Buffer allocation failed |
| `EAGAIN` | `O_NONBLOCK` and the operation would block |
| `EPIPE` | Write after local write shutdown, peer read shutdown or peer close |
| `EMSGSIZE` | Datagram larger than 64 KiB |
| `EFAULT` | The output pointer or data buffer is not a valid user range |
| `EBADF` | Unknown handle or the handle is not a duplex endpoint |
| `EPERM` / `EACCES` | The required `READ`/`WRITE` right is revoked or absent |

## Client ownership

K206 uses this primitive for `socketpair(AF_UNIX, SOCK_STREAM/SOCK_DGRAM)` and remains
responsible for the libc descriptor table, `sendmsg`/`recvmsg` translation and the AF_UNIX
address-space policy. No AF_INET/netd change belongs here, and no Pi 4 board claim is made.
