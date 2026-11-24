<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Pollable pipe endpoints (K372)

Status: **OBSERVED** for the K372 pipe poll. `pipe-poll` makes an empty writable pipe `POLLOUT`,
a written pipe `POLLIN`, a closed writer `POLLHUP`/EOF, and wakes a blocked poll from a forked
writer. The pipe endpoint is a real `EVQ` source; the libc `poll` glue already handled
`OfdKind::Pipe` and needed no change. The K195 `ssh-login` gate is **separate**: with
`RAVEN_SSH_REPEAT=10` the server reaches `svcd: sshd up`, netd ready and TCP 22, but the host
still times out during the SSH banner exchange before auth; the owner `ssh raven@... ls` 10/10
was not reached (0/10), and the PTY/wrong-password checks are not reached. No SSH login is
claimed. The exact remaining primitive is the netd queued-TX flush/doorbell from the prior K195
trace (`tx=(44, 0, 37, 4454)` queued, not flushed).

## Why

OpenSSH's monitor polls its privilege-separation/session pipe descriptors
(`addons/ssh/openssh/monitor.c` `monitor_openfds()`/`monitor_read()`, `session.c`
`do_exec_no_pty`). The kernel `EVQ_CTL`/`EVQ_WAIT` path had no `Object::Pipe` source, so a pipe
fd was not pollable and `sshd` closed the connection before the SSH banner.

## Fix

* `kernel/src/syscall/event.rs`: `source_from_handle` accepts a live `Object::Pipe` without the
  read/write data right (the write handle carries WRITE, not READ); `readiness` delegates to
  `PipeEnd::readiness`.
* `kernel/src/process/pipe.rs`: `PipeEnd::readiness` returns the EVQ bits (read end:
  `READABLE`, closed writer `READABLE|HUP`; write end: `WRITABLE` with free space, `HUP` with
  no reader); `read`/`write`/`Drop`/`retire` call `crate::event::notify_all()` after the pipe
  state change. The updated K372 ticket now includes this file and
  `libc/ravenlibc/crates/raven-libc-platform/src/poll/poll.rs` in its Owns; the libc file needed
  no edit. The earlier Owns omission is resolved.
* `libc/ravenlibc/crates/raven-libc-platform/src/poll/poll.rs` already had the
  `OfdKind::Pipe` branch and maps `EV_READABLE`/`EV_WRITABLE`/`EV_HUP` to
  `POLLIN`/`POLLOUT`/`POLLHUP`; no libc edit was needed. The acceptance
  `cargo nextest run -p raven-libc-platform` passes 584/584.

## Poll matrix

| State | Request | Result |
|---|---|---|
| empty pipe, both ends live | `POLLOUT` on write fd | `POLLOUT` immediately |
| after one byte written | `POLLIN` on read fd | `POLLIN` |
| writer closed, buffer drained | `POLLIN`/`POLLHUP` | `POLLHUP` (and readable EOF) |
| read fd polled before writer writes | `POLLIN`, 2 s timeout | blocked, wakes `POLLIN` on the forked writer's byte |

The C fixture `tests/fixtures/pipe-poll/main.c` runs as `/init` on the base kernel and prints
through the kernel debug console (the `/init` has no stdio fds). The qtest serial:

```text
pipe-poll: pollout ok
pipe-poll: pollin ok
pipe-poll: hup ok
pipe-poll: wake ok
pipe-poll: ok
```

## Donor reconciliation (DR-52)

| Topic | Illumos (CDDL, read-only) | NetBSD (BSD-2, behavioral donor) | Linux (outcome) | Raven decision |
|---|---|---|---|---|
| read readiness | `usr/src/uts/common/syscall/pipe.c` (`pipe(2)` wrapper) and `usr/src/uts/common/io/pipemod.c` (STREAMS pipe; poll via the stream) — behavior only | `sys/kern/sys_pipe.c` `pipe_poll` 851-905 (`PIPE_EOF` => `POLLIN|POLLRDNORM|POLLHUP`; data => `POLLIN|POLLRDNORM`), `filt_piperead` 1164-1193 | `fs/pipe.c`/`fs/select.c` absent; POSIX outcome only | Original Rust `PipeEnd::readiness`: data `READABLE`; closed writer `READABLE|HUP` |
| write readiness | STREAMS poll behavior only | `pipe_poll` 851-905 (reader gone => `POLLOUT|POLLWRNORM|POLLERR`; `PIPE_BUF` free => `POLLOUT|POLLWRNORM`), `filt_pipewrite` 1195-1218 | outcome only | Original Rust: free ring `WRITABLE`; reader gone `HUP`; full 0 |
| transition wake | STREAMS `pollwakeup` behavior only | `pipeselwakeup` 391-420 (`selnotify`), `pipe_read` 427-582 (`pipeselwakeup(wpipe, POLL_OUT)` at 539/577), `pipe_write` 586-785 (`pipeselwakeup(rpipe, POLL_IN)` at 717/773), `pipe_close` 939-... (`POLL_ERR` 1059, `POLL_HUP` 1063) | outcome only | `event::notify_all()` on read/write/drop/retire; the existing `Duplex` model; divergence: NetBSD wakes one `selinfo`, Raven broadcasts to event-queue waiters |
| pipe header/state | n/a | `sys/sys/pipe.h` (`struct pipe`, `pipe_buffer`, `pipe_peer`, `PIPE_EOF`) has a 4-clause/FreeBSD notice; **read-only, no code copied** | n/a | Raven `PipeState { ring, read_ends, write_ends }`; original mapping |

The `sys_pipe.c` `pipe_poll` behavior is the only NetBSD logic used; the Raven ring/endpoint
state differs, so the implementation is the minimal original Rust mapping. No code from the
4-clause `pipe.h` is copied. No CDDL/GPL code is copied.

## Tests

```sh
timeout 300 cargo xtask qtest --case pipe-poll --arch x86_64
timeout 180 cargo nextest run -p raven-libc-platform
timeout 300 cargo xtask qtest --case ssh-login --arch x86_64 --no-build
timeout 900 make check-fast
```

The first command was red before the fix (`pipe-poll: fail pollout`, exit 1); after the fix it
is `pipe-poll: pass`. The mutation (temporary pipe readiness -> 0) reproduces the red and was
reverted.

The K195 gate is separate. `RAVEN_SSH_REPEAT=10` against the K372 image reaches
`svcd: sshd up`, netd ready and TCP 22, but the host still times out during the SSH banner
exchange before auth; the owner `ssh raven@... ls` 10/10 was not reached (0/10), and the
`ssh -tt`/wrong-password checks are not reached. The exact remaining primitive is the netd
queued-TX flush/doorbell (prior K195 bob rounds 32-33, `tx=(44, 0, 37, 4454)` queued but not
flushed). No SSH login is claimed; the K195 gate remains separate. The report is
`tickets/tim/reports/K372.md`.
