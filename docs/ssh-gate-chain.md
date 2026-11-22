<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH gate chain: OpenSSH, FreeBSD, NetBSD and Raven

Status: OBSERVED (donor lines read in `ravenbsd-resources` on 2026-11-22; Raven lines read in this
tree). **No inference here.** Every donor row is a file:function:line that was read; anything not
read is under Gaps and is not evidence. This extends `tickets/ssh/SSH-CHAIN.md` (lead's half) with
the Raven side and the state differences.

Donor revisions: `ravenbsd-resources/donors.lock`; FreeBSD sparse checkout (with `sys/kern`),
NetBSD full tree, OpenSSH 10.6p2 vendored in `addons/ssh/openssh/`.

## Chain

| # | Step | OpenSSH call site | FreeBSD (file:function:line) | NetBSD (file:function:line) | Raven equivalent | Contract / difference we follow |
|---|---|---|---|---|---|---|
| C1 | accept | `sshd.c:912` `server_accept_loop`, `:1130` `accept()` | `sys/kern/uipc_syscalls.c:332` `kern_accept4`, `:433` `sys_accept` | `sys/kern/uipc_syscalls.c:177` `do_sys_accept`, `:278` `sys_accept` | netd `Call::Accept` (`userland/servers/netd/src/netd.rs` accept), libc `accept` (`libc/ravenlibc/crates/raven-libc-net/src/socket.rs:194`), kernel scheme file | accepted descriptor is a new socket; the listener keeps accepting. Raven: one `net:` scheme file per accepted connection; no BSD-specific deviation |
| C2 | nonblocking session socket | `sshd-session.c:1211` `ssh_packet_set_nonblocking`, `sshd-auth.c:710` | `fcntl(F_SETFL, O_NONBLOCK)` (POSIX; OpenSSH has no BSD branch) | same | libc `fcntl(F_SETFL)` (`raven-libc-platform` fd/OFD status), OFD flag; service fd status observed `0x802` in the K195 traces | reads/writes on a nonblocking socket may return `EAGAIN`, never block. Raven follows POSIX: `EAGAIN` on an empty read/full write |
| C3 | wait for readiness | `serverloop.c:226` `ppoll(...)`, `:167` `channel_prepare_poll` | `sys/kern/sys_generic.c:1675` `kern_poll` | `sys/kern/sys_select.c` poll path, `:743` `selrecord` | libc `poll` (`raven-libc-platform/src/poll/poll.rs`): live `Call::Poll` verdict + attached notification event as wake hint; kernel EVQ (`kernel/src/syscall/event.rs`) | readiness is current state, recomputed per call; a wake is a hint to re-check. Raven: the same model, with netd answering the live query and the doorbell only waking |
| C4 | readiness backend | `serverloop.c:384` `channel_after_poll` | `sys/kern/sys_generic.c:1955` `selrecord`, `:2003` `selwakeup`, `:131` `doselwakeup`; `sys/sys/socketvar.h:566` `sowakeup_aio` | `sys/kern/sys_select.c:743` `selrecord`, `:878` `selnotify`; `sys/kern/uipc_socket2.c:558` `sowakeup` | netd `socket_readiness`/`readiness` (`userland/servers/netd/src/netd.rs`) and the doorbell (`userland/servers/netd/src/main.rs` `call_attach`: signals readable and, after K409, writable); kernel `readiness` for `Object::SchemeFile` | `POLLIN`/`POLLOUT` reflect socket buffers at call time; wakeups do not carry truth. Raven follows this: the live `Call::Poll` is the verdict; the event is a hint |
| C5 | channel open -> confirmation | `channels.c:2198`, `:3330`/`:3453` `SSH2_MSG_CHANNEL_OPEN_CONFIRMATION`; `channel_output_poll` `:3199`; `serverloop.c:293` `process_output`, `:305` `ssh_packet_write_poll`; `packet.c:1508` `ssh_packet_write_wait` | `sys/kern/uipc_socket.c:2645` `sosend` | `sys/kern/uipc_socket.c:904` `sosend` | libc `Platform::write` -> netd file write (`netd.rs` `write`) -> tcpip send -> virtio-net; K195 write traces (`k195n write file=N len=44`, `k195w ok=44`) | one 44-byte packet: type 91, recipient channel = the client's channel, on the same connection. Raven: the confirmation is written by sshd-session after `server_input_channel_open` (serverloop.c:650-660) and must be delivered by netd on the same socket |
| C6 | session write | `serverloop.c:305` `ssh_packet_write_poll` -> `write()` | `sys/kern/uipc_socket.c:2645` `sosend` | `uipc_socket.c:904` `sosend` | libc `write` -> netd `write` -> tcpip | POSIX write: short count allowed; `EAGAIN` only if nonblocking and the send buffer is full; success means enqueued, not delivered. Raven follows POSIX |
| C7 | session read | `serverloop.c` read path -> `read()` | `sys/kern/uipc_socket.c:2790` `soreceive_generic_locked`, `:3266`/`:3297`/`:3469`/`:3532` `soreceive*` | `sys/kern/uipc_socket.c:1162` `soreceive` | libc `read` -> netd -> tcpip; K429 fixed the scheme read response COW so no `EFAULT` | POSIX read: short count allowed, `EAGAIN` only if nonblocking and empty; a valid user buffer never yields `EFAULT`. Raven follows this (K429) |
| C8 | fd/OFD across fork+exec | privsep fork/exec before `sshd-session` | `sys/kern/kern_descrip.c:1040` `kern_dup`, `:2608` `fdcopy`; `sys/kern/kern_exec.c:351` `kern_execve` | `sys/kern/kern_descrip.c:751` `fd_dup`, `:1427` `fd_copy` | libc fdmap (`build_fdmap`/`exec_fdmap_env`, `raven-libc-platform/src/platform.rs`) over the RFDM auxv; kernel `PROC_EXEC`/`PROC_FORK` | a duplicate shares the same open file description and socket; readiness belongs to the socket, not to a copy. Raven follows this; the notification event is duplicated in the exec fdmap |
| C9 | PTY allocation | `sshpty.c:57` `pty_allocate`; `session.c:1890` `mm_pty_allocate` | `sys/kern/tty_pts.c:735` `pts_alloc`; `lib/libutil/pty.c:47` `openpty`, `:88` `forkpty` | `sys/kern/tty_pty.c:370` `ptsopen`; `lib/libutil/pty.c:63` `openpty`, `:139` `forkpty` | ptyd `pty:` scheme (`userland/servers/ptyd`); libc `openpty`/`grantpt`/`unlockpt`/`ptsname_r` (`raven-libc-compat` pty) | `posix_openpt`/`grantpt`/`unlockpt`/`ptsname`; master/slave pair; termios defaults applied. Raven: ptyd supplies the pair; `ssh -tt` currently answers `PTY allocation request failed on channel 0` (K430) |
| C10 | session exec | `session.c:128` `do_exec`; `do_exec_pty` `:126` | `sys/kern/kern_exec.c:458` `do_execve` | same file/function family | raven-spawn (`crates/raven-spawn`) + kernel `PROC_EXEC` (0x6a); the shell is spawned by sshd-session | exec keeps inherited descriptors; no readiness or socket identity change. Raven follows this; exec carries the fdmap |

## State differences and which we follow

1. **Readiness model (C3/C4).** FreeBSD recomputes `kern_poll` per call over `selinfo`/`selrecord`;
   NetBSD uses `selnotify` on the same contract. Raven uses the same level-triggered contract, with
   two mechanisms: the live `Call::Poll` answer (the verdict) and a notification event (the wake
   hint). We follow the BSD contract that a wake is a hint and readiness is recomputed; the Raven
   event must therefore never be the only writability source (K409 fixed exactly this).
2. **Write semantics (C5/C6).** Both BSDs allow a short `sosend` count and `EAGAIN` only when the
   send buffer is full on a nonblocking socket. Raven follows POSIX: `Platform::write` may return a
   short count, and netd returns `EAGAIN`/`TxError::Busy` backpressure. Success means enqueued, not
   delivered, so the confirmation needs a later readback test (S5/S6), not just `write() == 44`.
3. **Read semantics (C7).** Both BSDs allow short reads; `EFAULT` is only for an invalid user
   buffer. K429 (COW for scheme read response buffers) restored this; the S4 row asserts it.
4. **FD/OFD identity (C8).** Both BSDs share the open file description across dup/fork/exec.
   Raven's exec fdmap duplicates the event handle so the post-exec poll keeps its wake; the socket
   identity (handle/file id) is preserved.
5. **PTY (C9).** FreeBSD `pts_alloc` and NetBSD `ptsopen` create the master/slave pair; `openpty`
   is a libutil convenience over `posix_openpt`. Raven: ptyd is the scheme server; the libc
   `openpty` path must match `lib/libutil/pty.c`. The current `ssh -tt` failure is at this step
   (K430), not at OpenSSH.

## Gaps (not evidence)

- FreeBSD `sowakeup` is not a standalone symbol in the sparse checkout; `socketvar.h:566`
  declares `sowakeup_aio`, and `kern_poll` re-evaluates readiness per call. The exact socket-buffer
  wake symbol is not read; do not cite one until read.
- NetBSD `pty_alloc` by that name was not found; `ptsopen` (`tty_pty.c:370`) is read, the
  allocation helper name/line is not. S8 cites `ptsopen`/`openpty` only.
- The Raven chain (`libc -> scheme -> netd -> tcpip -> virtio-net`) is a separate donor chain with
  its own tickets (K431/K435/K433); this file only fixes the OpenSSH/BSD side and the row mapping.
