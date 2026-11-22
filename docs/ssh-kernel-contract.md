<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH session kernel contract (K435)

Status: **OBSERVED** on `origin/master` `b8472426`. This is the kernel half of the SSH session
chain; the OpenSSH half is K434. Each row names the FreeBSD and NetBSD `file:function:line`
behavior, the exact contract, the Raven implementation, the test, and the mutation that fails
when the behavior is broken. Donor reading is behavioural only; no donor code is copied.

Donor revisions: FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`, NetBSD
`36eddebcc36cb0775aa2962005b9c05419a6ff01`.

## K1-K6 guest conformance suite

The primary artifact is `tests/smp/ssh-contract/`: one guest binary, one qtest, one runner. It
prints `CONTRACT <id> PASS|FAIL <detail>` per row and exits non-zero on the first red row. The
runner is:

```sh
timeout 600 sh tests/smp/ssh-contract/run.sh
```

Rows: K1 poll level (current, no cache/replay), K2 nonblocking read (EAGAIN/exact bytes/valid
buffer), K3 nonblocking write (EAGAIN/partial/success delivered on the same socket), K4 dup and
fork+exec identity/readiness, K5 PTY open/grant/unlock/ptsname/termios, K6 accept (accepted fd
independent, listener still accepts two connections).

State on 2026-11-22 (rev 85, base `b8472426`):

| Row | Result | Detail |
|---|---|---|
| K1 | PASS | `level=current drain=no-replay hup=seen`; the Duplex write end registers as an EVQ source (`kernel/src/syscall/event.rs` no-rights arms) |
| K2 | PASS | `partial=10/10/80 eagain=1 eof=0` |
| K3 | PASS | `partial=positive eagain=1 delivered=4096` |
| K4 | **FAIL** | the exec child polls, reads PING and writes PONG on the inherited socket, but its `_exit` never reaches `process::exit_process`; the parent's `waitpid` reports 0 for every iteration; watchdog prints `CONTRACT K4 FAIL child-exit-hang` |
| K5 | known FAIL | libc exports `posix_openpt`, `grantpt`, `unlockpt` and `ptsname_r`, but **no `ptsname` symbol**; the suite declares it weak and would print `CONTRACT K5 FAIL ptsname symbol missing` (row not reached because K4 is first red) |
| K6 | not reached | the host peer opens two connections; it is not exercised while K4 is red |

The K4 defect is a libc/userspace exit hang after a poll on a duplex/socketpair; the kernel exit
trace is not reached. It is outside K435's kernel/syscall/process Owns and is handed to the
libc/K434 owner. The K5 missing `ptsname` is also libc (only `ptsname_r` exists). The K1-K3
rows and their mutations are green; the K1 fix is the Duplex no-rights EVQ source arm.


## 1. poll/select readiness

| Donor | Exact reference |
|---|---|
| FreeBSD | `sys/kern/sys_generic.c` `kern_poll` 1675, `selrecord` 1955, `selwakeup` 2003, `doselwakeup` 2019; `sys/sys/selinfo.h` `struct selinfo` |
| NetBSD | `sys/kern/sys_select.c` `pollcommon` 626, `pollscan` 679, `selrecord` 743, `selnotify` 878; `sys/sys/selinfo.h` 73 |

Contract: readiness is **level-triggered and computed on each call** from the descriptor's
current state. `POLLIN` is reported when data is queued; `POLLOUT` when the send path has space
(FreeBSD `kern_poll` masks `POLLOUT` when the socket buffer is full, `sys_generic.c:1829-1833`);
a closed peer adds `POLLHUP`. `selrecord` only registers a waiter and `selwakeup`/`selnotify` is
a wake hint, not the source of truth. After the data is drained, the next call reports not
readable; the same for a filled send path.

Raven: `libc/ravenlibc/crates/raven-libc-platform/src/poll/poll.rs` `poll` 36,
`delivered_revents` 313; kernel `kernel/src/syscall/event.rs` `source_from_handle`/`readiness`;
`kernel/src/process/duplex.rs` `DuplexEnd::readiness` 237; pipe readiness retains the K372
pattern. The current `DUPLEX_PAIR` ends carry READ|WRITE, so both ends register as EVQ sources;
the K372 no-rights arm remains for write-only pipe ends. The test polls the write end through
its READ|WRITE rights; no kernel change was required for these rows.

Test: `tests/smp/ssh-contract/ssh-contract.toml` `poll` row. It checks an empty writable
socketpair is `POLLOUT`, an empty read end is not ready, `select` agrees, a write makes the read
end `POLLIN`, a drain makes the next poll return 0 (level re-evaluation), and a closed writer
reports `POLLHUP`. Mutation: `DuplexEnd::readiness` temporarily returns 0; the test fails at
`poll empty write-ready` (`/tmp/opencode/k435-mutation-readiness.log`).

Observed libc gap (outside K435 Owns): a non-scheme descriptor's `poll(..., timeout=0)` returns 0
before the venue scan (`poll.rs:113-130` only scans `registration.scheme`); the kernel EVQ is
already ready. The SSH path polls with a nonzero timeout; the test uses 1 ms for the same
call-time level assertion. Follow-up owner: libc/K434.

## 2. Socket read (`soreceive`)

| Donor | Exact reference |
|---|---|
| FreeBSD | `sys/kern/uipc_socket.c` `soreceive` 3708, `soreceive_stream_locked` 3297, `soreceive_generic_locked` 2790, `sbwait` 2882; nonblocking empty returns `EAGAIN` at 3344 |
| NetBSD | `sys/kern/uipc_socket.c` `soreceive` 1162, `sbwait` 1286; nonblocking empty returns `EWOULDBLOCK` at 1283 |

Contract: a stream read may return **fewer bytes than requested**; it returns as soon as data is
available. `EAGAIN`/`EWOULDBLOCK` is returned only when the descriptor is nonblocking and the
receive queue is empty (and the peer is live); a blocking read waits. A **valid** user buffer
must never produce `EFAULT`; `EFAULT` is only the copyout failure for an invalid buffer. A
zero-length read returns 0.

Raven: `kernel/src/syscall/ipc.rs` `read` 120; `kernel/src/process/duplex.rs` `read` 542,
`read_stream` 588; `kernel/src/usercopy.rs` `copy_out` for the valid-buffer check; the AF_INET
scheme read uses the same `read` contract in the net stack. The K429 COW fix keeps a valid
writable read buffer from failing with `EFAULT` (`socket-read-efault`).

Test: `ssh-contract` `read` row: 100 bytes are written, reads return 10/10/80, the remaining
level is polled, the drained level is 0, and an empty nonblocking read returns `-1/EAGAIN`. The
socket is created with `SOCK_NONBLOCK`. `socket-read-efault` separately proves the valid COW
buffer path. Mutation: `duplex::read` temporarily returns `EFAULT` for buffers larger than one
byte; the test fails at `read partial first` (`/tmp/opencode/k435-mutation-read2.log`).

Observed libc gap (outside K435 Owns): `fcntl(F_SETFL, O_NONBLOCK)` on an existing blocking
socketpair does not reach the kernel `DuplexEnd`'s creation-time `nonblock` flag; the read then
blocks instead of returning `EAGAIN`. The test uses `SOCK_NONBLOCK` at creation. Follow-up owner:
libc/K434; OpenSSH sets its nonblocking sockets with `fcntl` and will need this.

## 3. Socket write (`sosend`)

| Donor | Exact reference |
|---|---|
| FreeBSD | `sys/kern/uipc_socket.c` `sosend` 2645, `sosend_generic_locked` 2365, `sbwait` 2494, `sosend_dgram` 2202 |
| NetBSD | `sys/kern/uipc_socket.c` `sosend` 904, `sbwait` 987 |

Contract: a stream write may queue a **partial** buffer and return the number queued; the
enqueue is independent of delivery. `EAGAIN` only when the descriptor is nonblocking and the
send buffer/control queue is full; a blocking write waits for space. A write after local
shutdown/peer close returns `EPIPE` (and `SIGPIPE` unless suppressed).

Raven: `kernel/src/syscall/ipc.rs` `write` 164; `kernel/src/process/duplex.rs` `write` 726,
`write_stream` 758; the 64 KiB stream ring is `DUPLEX_CAPACITY`; `EPIPE` is raised on shutdown or
peer close; datagram sends are atomic in `write_datagram`.

Test: `ssh-contract` `write` row: a nonblocking 128 KiB write returns a positive partial count,
the next write returns `-1/EAGAIN`, draining the accepted bytes restores the writable level, and
a 4 KiB write then succeeds. The socket is created with `SOCK_NONBLOCK`. Mutation:
`write_stream` temporarily returns `Ok(0)` for a full nonblocking write; the test fails at
`write full EAGAIN` (`/tmp/opencode/k435-mutation-write.log`).

## 4. PTY allocation

| Donor | Exact reference |
|---|---|
| FreeBSD | `sys/kern/tty_pts.c` `sys_posix_openpt` 835, `pts_alloc` 735, `TIOCGPTN` 325; `lib/libutil/pty.c` `openpty` 53-63; `lib/libc/stdlib/ptsname.c` `ptsname` 106 |
| NetBSD | `lib/libc/stdlib/posix_openpt.c` `posix_openpt` 42; `sys/kern/tty_ptm.c` `pty_alloc_master` 159, `pty_alloc_slave` 261; `lib/libc/stdlib/pty.c` `grantpt` 45, `unlockpt` 53, `ptsname` 60; `lib/libutil/pty.c` `openpty` 63 |

Contract: `posix_openpt` allocates a master/slave pair; `grantpt` makes the slave accessible to
the caller; `unlockpt` permits the slave to be opened; `ptsname` returns the slave path (the
Raven pty server names it `pty:slave/<id>`); the master and slave are paired; the slave has the
usual terminal defaults (canonical mode, echo) and supports termios; `openpty`/`forkpty` run the
child with the slave as controlling terminal and stdio.

Raven: `libc/ravenlibc/crates/raven-libc/src/compat/posix_openpt.rs`, `grantpt.rs`,
`unlockpt.rs`, `ptsname_r.rs`; `userland/servers/ptyd/src/pairs.rs` and `pty.rs` own the pair and
termios; `kernel/src/process/tty.rs` `acquire`/`get_fg`/`set_fg` own the controlling-terminal
state. Tests: `libc-forkpty` (pair + `pty:slave/<id>` name + master transcript + child status)
and `pty-session` (canonical/raw, echo, flush, hangup). Mutation: `tty::acquire` temporarily
returns `ENOTTY`; `libc-forkpty` fails with an empty transcript
(`/tmp/opencode/k435-mutation-pty.log`).

## 5. FD duplication across fork/exec

| Donor | Exact reference |
|---|---|
| FreeBSD | `sys/kern/kern_descrip.c` `kern_dup` 1040, `fdcopy` 2608, `closef` 3036, `dupfdopen` 4089, `fget` 3811 |
| NetBSD | `sys/kern/kern_descrip.c` `fd_dup` 751, `fd_dup2` 773, `fd_copy` 1427, `closef` 836 |

Contract: `dup` and fork copy the descriptor table entries but reference the **same open file
description/object**; `exec` preserves descriptors without `FD_CLOEXEC` and closes those with
it. The same socket/duplex object is visible through every duplicate, and readiness is shared:
a write through one duplicate makes every duplicate of the read end `POLLIN`; a drain through
one makes the next poll through any duplicate return 0.

Raven: `crates/raven-handles/src/lib.rs` `HandleTable::fork_into` 225 (shared `Arc` object,
generation/rights/lineage copied); `kernel/src/process/mod.rs` `new_forked_process` 539/606;
`kernel/src/process/exec.rs` 172-178 (only CLOEXEC handles close); `kernel/src/syscall/ipc.rs`
`dup` 213. Tests: `ssh-contract` `dup` row (same duplex/readiness after `dup` and after a drain),
`socketpair-exec` (dup/fork/exec data, EOF, CLOEXEC), and `socket-write-exec` (the inherited
AF_INET socket is pollable and writable after exec). Mutation: `exec.rs` marks every `Duplex`
handle CLOEXEC; `socketpair-exec` fails with `reader read errno=9` (`EBADF`)
(`/tmp/opencode/k435-mutation-exec2.log`).

## Test command

```sh
timeout 600 sh tests/smp/ssh-contract/run.sh
```

The runner builds `tests/smp/ssh-contract/main.c` into the base profile and runs the local
`poll`/`read`/`write`/`dup` rows, then `socket-read-efault`, `socketpair-exec`,
`socket-write-exec` and `libc-forkpty` for the remaining rows. Each row's mutation is documented
above; the mutations are temporary kernel edits and are reverted in the committed tree.
