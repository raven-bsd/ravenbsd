<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pipes

The kernel provides `pipe(2)`/`pipe2(2)` as two ordinary handles over one shared, fixed-capacity
byte ring. There is no second descriptor table: `read`, `write`, `handle_dup`, `handle_close`,
`HANDLE_TRANSFER` and `PROC_FORK` all operate on the existing process handle table.

## Syscall numbers

`crates/raven-abi` is a reviewer-owned contract crate, so P2j adds kernel-owned raw numbers next
to the existing positional I/O calls:

| Number | Call | Arguments |
|---|---|---|
| `0x6e` | `pipe` | `(fds: *mut i32) -> 0` |
| `0x6f` | `pipe2` | `(fds: *mut i32, flags: u32) -> 0` |

`fds[0]` receives the read handle and `fds[1]` the write handle. `userland/rt` exposes
`raven_rt::pipe::{pipe, pipe2}` and the two flag constants. A later ABI review can promote the
numbers into `raven-abi` without changing the primitive.

## Ring and endpoints

* Capacity is 64 KiB (`PIPE_CAPACITY`). The ring is a single allocation; the host-testable wrap
  arithmetic lives in `crates/raven-ipc/src/pipe_ring.rs` and the kernel compiles the same source.
* A pipe has one read endpoint and one write endpoint. Each `Object::Pipe` clone is one live
  handle; endpoint `Drop` runs when the last handle clone disappears, so `PROC_FORK`,
  `HANDLE_DUP` and `HANDLE_TRANSFER` share the pipe without a reference-count table.
* Closing the last write end wakes blocked readers and makes the next read return 0 (EOF).
  Closing the last read end wakes blocked writers and makes the next write return `EPIPE`.
* `read` returns the bytes currently available (at most the caller's length). `write` to a
  blocking pipe completes the whole buffer, waiting for readers when the ring fills; a
  non-blocking write returns the bytes that fitted, or `EAGAIN` when none did.
* `O_NONBLOCK` applies to both ends, as on Linux. `O_CLOEXEC` is accepted and recorded per
  endpoint; there is no `exec` yet, so a future exec ticket must sweep handles carrying it.
  Unknown flag bits are `EINVAL`.

## Blocking and signals

Blocking uses the scheduler wait queues: a read waits while the ring is empty and a writer
remains, a write waits while the ring is full and a reader remains. The condition runs under the
wait-queue lock, and the pipe state lock is released before any wake, so the two locks do not
nest in opposite orders.

A write with no live read end queues POSIX `SIGPIPE` on the calling process and returns `EPIPE`.
The kernel signal queue does not interpret dispositions; `PROC_SIGRECV` reports the signal and
the userspace signal layer decides what happens next. A pipe with no live write end returns 0.

## Inheritance

`PROC_FORK` clones the handle table, including both pipe endpoints. The child and parent then
share the same ring and the same endpoint liveness: a child's `handle_close` only removes its
copy, and the last close in either process drives EOF or `EPIPE` for the other side. The
`pipe-fork` qtest exercises a blocked child read, a parent wake, EOF and a broken pipe across the
fork.

## Scope

This ticket provides the primitive and its qtests only. `popen`/`system`, `libc`'s `pipe2` platform
seam and shell pipelines are follow-ups; no process or shell conformance row is claimed here.

## Tests

```
cargo xtask qtest --case pipe --arch x86_64
cargo xtask qtest --case pipe-fork --arch aarch64
```

`pipe` covers data, the 64 KiB ring, `O_NONBLOCK`/`EAGAIN`, EOF, `EPIPE`/`SIGPIPE`, wrong-direction
handles, unknown flags and wraparound. `pipe-fork` covers inheritance, cross-process blocking
read, a 72 KiB blocking write, EOF and `EPIPE`/`SIGPIPE`. Host tests for the ring are
`crates/raven-ipc/src/pipe_ring_ut.rs`.
