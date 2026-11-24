<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH write path: OpenSSH S5 to the Raven NIC

This extends `tickets/ssh/SSH-CHAIN.md` (the lead's C1-C10 table) for the Raven half. The
OpenSSH and BSD lines below were read in the workspace donors; the Raven lines are the current
`origin/master` sources. A line that was not read is marked UNKNOWN and is not evidence.

## S5 chain (confirmation → host peer)

| # | Step | OpenSSH call site | FreeBSD donor | NetBSD donor | Raven call site |
|---|---|---|---|---|---|
| S5-1 | confirmation composed | `channels.c:2198` `sshpkt_start(SSH2_MSG_CHANNEL_OPEN_CONFIRMATION)`; `channels.c:5085`; `serverloop.c:644` | `sys/kern/uipc_socket.c:2645` `sosend` (sink only; the packet is user data) | `sys/kern/uipc_socket.c:904` `sosend` | `userland/bin/sshd` is vendored OpenSSH; no edit |
| S5-2 | output poll | `serverloop.c:364` `channel_output_poll`; `channels.c:3199` `channel_output_poll`; `channels.c:3089` `channel_output_poll_input_open` | `sys/kern/sys_generic.c:1675` `kern_poll` (current readiness) | `sys/kern/sys_select.c` `selrecord`/`selnotify` | `libc/ravenlibc/crates/raven-libc-platform/src/poll/poll.rs` `poll`; K409 live `Call::Poll` |
| S5-3 | flush | `serverloop.c:293` `process_output`; `:305` `ssh_packet_write_poll` | `sys/kern/sys_generic.c:397` `sys_write` / `:551` `dofilewrite` | `sys/kern/sys_generic.c:299` `sys_write` / `:325` `dofilewrite` | `libc/ravenlibc/crates/raven-libc-platform/src/platform.rs:816` `write` |
| S5-4 | POSIX write | `packet.c:2178` `ssh_packet_write_poll`; `:2185` `write(state->connection_out, ...)` | `sys/kern/sys_generic.c:551` `dofilewrite` → `fo_write` → `uipc_socket.c:2645` `sosend` | `sys/kern/sys_generic.c:325` `dofilewrite` → `uipc_socket.c:904` `sosend` | `backend.rs:172` `RealSys::write` → `raven-libc-sys/src/sys.rs:578` `write` → `Syscall::WRITE` |
| S5-5 | kernel route | — | — | — | `kernel/src/syscall/ipc.rs:145` `write` → `kernel/src/ipc/router.rs:238` `write` / `:258` `write_at` (`Opcode::Write`) |
| S5-6 | netd file/sid | — | — | — | `userland/servers/netd/src/netd.rs:865` `Netd::write` → `:289` `SocketCore::send` → `NetCore::send` |
| S5-7 | TCP queue | — | `sys/kern/uipc_socket.c:2645` `sosend` (enqueue, partial count, `EAGAIN`) | `sys/kern/uipc_socket.c:904` `sosend` | `crates/raven-tcpip/src/tcp_data.rs:208` `tcp_send_data` → `:356` `tcp_try_send` |
| S5-8 | segment build | — | `sys/netinet/tcp_output.c` `tcp_output` (read-only donor) | `sys/netinet/tcp_output.c` `tcp_output` (read-only donor) | `crates/raven-tcpip/src/tcp_state.rs:2908` `send_tcp_packet` → IPv4 output |
| S5-9 | NIC frame | — | `sys/dev/pci/if_vioif.c` TX ring (BSD-2; K382/K433 contract) | `sys/dev/pci/if_vioif.c` (BSD-2) | `userland/servers/netd/src/nic.rs:236` `transmit` → B39 ring; `userland/drivers/virtio-net/src/main.rs:762` `pump_tx` |
| S5-10 | host receives | — | — | — | host peer in `tests/fixtures/socket-write-identity/host.py` |

## C8/C10: fd and OFD across exec

| # | Step | OpenSSH | FreeBSD | NetBSD | Raven |
|---|---|---|---|---|---|
| C8-1 | dup | `session.c:do_exec_no_pty` `dup2` | `sys/kern/kern_descrip.c:1040` `kern_dup` | `sys/kern/kern_descrip.c:751` `fd_dup` | `libc/ravenlibc/crates/raven-libc-platform/src/platform.rs` `dup2`: one OFD, two fds |
| C8-2 | fork table | `session.c:do_exec_no_pty` `fork()` | `sys/kern/kern_descrip.c:2608` `fdcopy` | `sys/kern/kern_descrip.c:1427` `fd_copy` | `kernel/src/process/mod.rs:new_forked_process` `HandleTable::fork_into` |
| C8-3 | exec keeps fds | `session.c:do_child` `execve` | `sys/kern/kern_exec.c:351` `kern_execve` / `:458` `do_execve` | same family | `kernel/src/syscall/exec.rs:exec` + `libc/.../process/exec_common.rs` (serialized `__RAVEN_FDMAP`) |
| C8-4 | fd 0/1/2 after exec | `channels.c:1340` `channel_set_fds` | inherited file table | inherited file table | libc `restore_inherited_fds`/`restore_exec_fdmap` |

**Recorded divergence (needs reviewer approval).** BSD `execve` keeps the process's file descriptor
table in the kernel (`kern_descrip.c:fdcopy`/`fdfree`); Raven's `PROC_EXEC` replaces the image and
keeps the kernel handles, so libc serializes the fd→handle table into the `__RAVEN_FDMAP`
environment entry and restores it in the new image. HARD-RULES section 3 names this as a
red-flag pattern; the divergence is recorded here and tested by `socket-write-identity` (fd 1/2
and fd 7 must share one socket file/sid across exec).

## S5 contract to assert

- A successful `write()` means the bytes were enqueued in the socket send queue; it is not a
  delivery promise (FreeBSD `sosend`, NetBSD `sosend`). `EAGAIN` is only for a non-blocking socket
  with a full buffer; a short count is allowed.
- The host peer must receive the exact bytes on the **same TCP connection** that carried the
  banner/KEX/auth. The fixture asserts that with one accepted connection and one ordered byte
  stream.
- Across `dup2` + `fork` + `exec`, fd 1, fd 2 and fd 7 must resolve to the **same** netd file id
  (`fstat().st_ino`), and a sentinel written to fd 1/2 must reach the peer and must not appear on
  the guest console.
- If the test is red at S5-9 (NIC), the single step and test go to K433; this ticket does not
  edit the driver. If it is red at S5-4..S5-6 (libc/netd/tcpip), this ticket owns the fix.

## Gaps (UNKNOWN, not evidence)

- The 44-byte plaintext confirmation length is not observable from the client log (SSH-CHAIN S6).
- `fstat().st_ino` for a netd socket is the netd file id (`netd::fstat` returns `ino: file`); the
  kernel handle value is not exposed to userspace, so the fixture asserts the netd file id and the
  single-connection byte stream, not the raw handle number.
- The FreeBSD `if_vioif.c` TX line is the K433 donor contract; no driver edit is made here.
