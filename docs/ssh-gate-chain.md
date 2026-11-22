<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SSH gate chain: OpenSSH, FreeBSD, NetBSD and Raven

Status: OBSERVED (donor lines read in `ravenbsd-resources` on 2026-11-22; Raven lines read in this
tree). **No inference here.** Every donor row is a file:function:line that was read; anything not
read is under Gaps and is not evidence. This extends `tickets/ssh/SSH-CHAIN.md` (lead's half) with
the Raven side, the syscall/return contract and the state differences.

Donor revisions: `ravenbsd-resources/donors.lock`; FreeBSD sparse checkout (with `sys/kern`),
NetBSD full tree, OpenSSH 10.6p2 vendored in `addons/ssh/openssh/`.

## Banner correction (S1)

The ticket says S1 expects `SSH-2.0-OpenSSH_10.6p2\r\n`. The vendored source says otherwise:

- `addons/ssh/openssh/version.h:3` `SSH_VERSION "OpenSSH_10.6"`;
- `:5` `SSH_PORTABLE "p2"`; `:6` `SSH_RELEASE SSH_VERSION SSH_PORTABLE` (used by the command-line
  version output, not the wire banner);
- `addons/ssh/openssh/kex.c:1297` builds the identification string with `PROTOCOL_MAJOR_2`,
  `PROTOCOL_MINOR_2` and **`SSH_VERSION`**, not `SSH_RELEASE`.

So the normative wire banner is exactly `SSH-2.0-OpenSSH_10.6\r\n` (22 bytes), and that is what S1
asserts. The `10.6p2` value in the ticket is a documentation expectation, not the protocol bytes;
this deviation is recorded in the K434 report.

## Chain

| # | Step | OpenSSH call site | FreeBSD (file:function:line) | NetBSD (file:function:line) | Raven equivalent |
|---|---|---|---|---|---|
| C1 | accept | `sshd.c:912` `server_accept_loop`, `:1130` `accept()` | `sys/kern/uipc_syscalls.c:332` `kern_accept4` (via `:326` `kern_accept`), `:433` `sys_accept`, `:438` `sys_accept4` | `sys/kern/uipc_syscalls.c:177` `do_sys_accept`, `:278` `sys_accept` | netd `Call::Accept` -> `net:` scheme file; libc `accept` (`raven-libc-net/src/socket.rs:194`); kernel scheme file |
| C2 | nonblocking session socket | `sshd-session.c:1211` `ssh_packet_set_nonblocking`, `sshd-auth.c:710` | `sys/kern/kern_descrip.c:648` `F_SETFL` in `kern_fcntl` (POSIX `fcntl`) | `sys/kern/kern_descrip.c:1982` `F_SETFL` (POSIX `fcntl`) | libc `fcntl(F_SETFL)` over the OFD status; service fd status observed `0x802` in the K195 traces |
| C3 | wait for readiness | `serverloop.c:226` `ppoll`, `:167` `channel_prepare_poll`, `:384` `channel_after_poll` | `sys/kern/sys_generic.c:1675` `kern_poll` | `sys/kern/sys_select.c:743` `selrecord`; poll path in the same file | libc `poll` (`raven-libc-platform/src/poll/poll.rs`): live `Call::Poll` verdict + attached notification event as wake hint |
| C4 | readiness backend | `serverloop.c:384` `channel_after_poll` | `sys/kern/sys_generic.c:1955` `selrecord`, `:2003` `selwakeup`, `:2030` `doselwakeup`; `sys/kern/uipc_sockbuf.c:469` `sowakeup`, `sys/kern/sys_socket.c:282` `soo_poll` | `sys/kern/sys_select.c:743` `selrecord`, `:878` `selnotify`; `sys/kern/uipc_socket2.c:558` `sowakeup` | netd `socket_readiness`/`readiness` and the doorbell (`call_attach`: readable and, after K409, writable); kernel `readiness` for `Object::SchemeFile` |
| C5 | channel open -> confirmation | `channels.c:2198`, `:3330`/`:3453` `SSH2_MSG_CHANNEL_OPEN_CONFIRMATION`; `channel_output_poll` `:3199`; `serverloop.c:293` `process_output`, `:305` `ssh_packet_write_poll`; `packet.c:1508` `ssh_packet_write_wait` | `sys/kern/uipc_socket.c:2645` `sosend`, `:2661` `sousrsend` | `sys/kern/uipc_socket.c:904` `sosend` | libc `Platform::write` -> netd file write -> tcpip send -> virtio-net; K195 write traces (`k195n write file=N len=44`, `k195w ok=44`) |
| C6 | session write | `serverloop.c:305` `ssh_packet_write_poll` -> `write()` | `sys/kern/uipc_socket.c:2645` `sosend`/`:2661` `sousrsend` | `sys/kern/uipc_socket.c:904` `sosend` | libc `write` -> netd `write` -> tcpip |
| C7 | session read | `serverloop.c` read path -> `read()` | `sys/kern/uipc_socket.c:2790` `soreceive_generic_locked`, `:3266`/`:3297`/`:3469`/`:3532` `soreceive*` | `sys/kern/uipc_socket.c:1162` `soreceive` | libc `read` -> netd -> tcpip; K429 fixed the scheme read response COW so no `EFAULT` |
| C8 | fd/OFD across fork+exec | privsep fork/exec before `sshd-session` | `sys/kern/kern_descrip.c:1040` `kern_dup`, `:2608` `fdcopy`; `sys/kern/kern_exec.c:351` `kern_execve` | `sys/kern/kern_descrip.c:751` `fd_dup`, `:1427` `fd_copy` | libc fdmap (`build_fdmap`/`exec_fdmap_env`, `raven-libc-platform/src/platform.rs`) over the RFDM auxv; kernel `PROC_EXEC`/`PROC_FORK` |
| C9 | PTY allocation | `sshpty.c:57` `pty_allocate`; `session.c:1890` `mm_pty_allocate` | `sys/kern/tty_pts.c:735` `pts_alloc`; `lib/libutil/pty.c:47` `openpty`, `:88` `forkpty` | `sys/kern/tty_pty.c:201` `ptyarralloc`, `:353` `ptyattach`, `:370` `ptsopen`; `sys/kern/tty_ptm.c:159` `pty_alloc_master`, `:261` `pty_alloc_slave`, `:339` `ptmopen`, `:396` `TIOCPTMGET`; `lib/libutil/pty.c:63` `openpty`, `:139` `forkpty` | ptyd `pty:` scheme (`userland/servers/ptyd`); libc `openpty`/`grantpt`/`unlockpt`/`ptsname_r` (`raven-libc-compat` pty) |
| C10 | session exec | `session.c:128` `do_exec`; `do_exec_pty` `:126` | `sys/kern/kern_exec.c:351` `kern_execve`, `:458` `do_execve` | same file/function family | raven-spawn (`crates/raven-spawn`) + kernel `PROC_EXEC` (0x6a) |

## Exact syscall, buffer and return contract per step

### C1 accept (`accept(2)`)

- OpenSSH: `sshd.c:1130` calls `accept(listen_socks[i], &from, &fromlen)`; a return of `-1` is
  ignored only for `EINTR`, `EWOULDBLOCK`, `ECONNABORTED`, `EAGAIN`; the accepted descriptor is a new
  socket, the listener remains open.
- FreeBSD: `kern_accept4` copies the peer address into the caller's `sockaddr` buffer (`sa` may be
  `NULL`), allocates a new file/socket, and returns the fd. Errors: `EBADF` bad fd, `ENOTSOCK`
  non-socket, `EINVAL` not listening, `EMFILE`/`ENFILE` fd exhaustion.
- NetBSD: `do_sys_accept` returns `EBADF`/`ENOTSOCK`, allocates a new file, blocks in the socket
  layer, and returns the new fd; the caller's `sockaddr` buffer is written only on success.
- Raven: netd accepts, publishes a new `net:` scheme file for the connection, libc returns a new
  descriptor; the listener stays open.

### C2 nonblocking (`fcntl(F_SETFL, O_NONBLOCK)`)

- OpenSSH sets `O_NONBLOCK` on the session socket before the packet loop (`sshd-session.c:1211`).
- FreeBSD/NetBSD implement this as the POSIX `F_SETFL` path (`kern_descrip.c`); the flag lives on the
  open file description, so a dup shares it.
- Return: `0` on success; `-1` with `EBADF`/`EINVAL` on failure. Raven follows POSIX: `read`/`write`
  on a nonblocking socket return `-1`/`EAGAIN` rather than blocking.

### C3/C4 readiness (`poll(2)`, `ppoll(2)`)

- OpenSSH passes a `struct pollfd` array to `ppoll` (`serverloop.c:226`). `POLLIN`/`POLLOUT` are the
  current state of the socket buffers at call time; the event is a wake hint, never the truth.
- FreeBSD: `kern_poll` (`sys_generic.c:1675`) copies the `pollfd` array in, registers waiters with
  `selrecord` (`:1955`), sleeps, and re-evaluates; a socket wake calls `sowakeup`
  (`uipc_sockbuf.c:469`), which calls `selwakeuppri` -> `doselwakeup` (`sys_generic.c:2030`).
  `soo_poll` (`sys_socket.c:282`) forwards to the protocol poll. Return: number of ready fds, `0` on
  timeout, `-1` with `EFAULT`/`EINVAL`/`ENOMEM` on failure.
- NetBSD: `selrecord` (`sys_select.c:743`) and `selnotify` (`:878`); `sowakeup`
  (`uipc_socket2.c:558`) computes the `POLLIN`/`POLLOUT`/`POLLHUP` band and notifies. Same return
  contract.
- Raven: the live `Call::Poll` answer is the verdict; the notification event is only a wake hint.
  The event must never be the sole writability source (K409 fixed exactly that).

### C5/C6 write (`write(2)`, `sosend`)

- OpenSSH writes the channel confirmation through the packet layer; `ssh_packet_write_poll`
  (`serverloop.c:305`) drains the output buffer to `write()`.
- FreeBSD: `sosend` (`uipc_socket.c:2645`) forwards to the protocol; `sousrsend` (`:2661`) updates
  `uio->uio_resid` and returns `0` on success, or `EWOULDBLOCK` when a nonblocking send buffer is
  full, `EPIPE`/`ENOTCONN`/`EMSGSIZE` on the named failures. A short count is legal.
- NetBSD: `sosend` (`uipc_socket.c:904`) returns `EINVAL`, `EPIPE`, `ENOTCONN`, `EDESTADDRREQ`,
  `EMSGSIZE`, or `EWOULDBLOCK` for a nonblocking full buffer; a short count is legal.
- Raven: `Platform::write` -> netd file write -> tcpip; success means enqueued, not delivered.
  `EAGAIN`/`TxError::Busy` is the nonblocking full-buffer result.

### C7 read (`read(2)`, `soreceive`)

- FreeBSD: `soreceive_generic_locked` (`uipc_socket.c:2790`) updates `uio` and returns `0`, or
  `EWOULDBLOCK` on an empty nonblocking socket, `EFAULT` only for an invalid user buffer. Short
  reads are legal.
- NetBSD: `soreceive` (`uipc_socket.c:1162`) has the same contract.
- Raven: K429 fixed the scheme read response COW so a valid user buffer never yields `EFAULT`.

### C8 fd/OFD across fork+exec

- FreeBSD: `kern_dup` (`kern_descrip.c:1040`) and `fdcopy` (`:2608`) duplicate file references; a
  duplicate shares the open file description and the socket, so readiness belongs to the socket.
  `kern_execve` (`kern_exec.c:351`) keeps descriptors without `FD_CLOEXEC`. Errors:
  `EBADF`/`EMFILE`/`ENFILE`.
- NetBSD: `fd_dup` (`kern_descrip.c:751`), `fd_copy` (`:1427`) have the same identity rule.
- Raven: the exec fdmap duplicates the notification event so the post-exec poll keeps its wake; the
  socket identity (handle/file id) must be preserved.

### C9 PTY (`openpty(3)`, `posix_openpt(3)`)

- OpenSSH: `pty_allocate` (`sshpty.c:57`) opens a master/slave pair, applies termios, and returns the
  slave name; `session.c:1890` fails the channel request when it returns false.
- FreeBSD: `pts_alloc` (`tty_pts.c:735`) allocates the pts unit and a tty, exposes `/dev/pts/N`, and
  returns `EAGAIN` on resource exhaustion; `openpty` (`lib/libutil/pty.c:47`) is
  `posix_openpt` + `grantpt` + `unlockpt` + `ptsname` + `open`, returning `-1` with the failing
  errno. `forkpty` (`:88`) adds `fork`/`login_tty`.
- NetBSD: `ptyarralloc` (`tty_pty.c:201`) allocates the softc array; `pty_alloc_master`/
  `pty_alloc_slave` (`tty_ptm.c:159`/`:261`) allocate the pair; `ptmopen` (`:339`) and
  `TIOCPTMGET` (`:396`) expose the two fds and the slave name; `ptsopen` (`tty_pty.c:370`) sets the
  termios defaults and returns `EBUSY` when the slave is already open. `openpty` (`lib/libutil/pty.c:63`)
  and `forkpty` (`:139`) are the libutil convenience over that path.
- Raven: ptyd supplies the `pty:` scheme pair; libc `openpty`/`grantpt`/`unlockpt`/`ptsname_r` must
  match the libutil sequence. K430 and K436 are merged; the `pty-alloc` and `pty-calls` qtests pass,
  including `ptsname`/`ttyname` `/dev/pts/N`. The SSH PTY request still answers
  `PTY allocation request failed on channel 0`; K438 (SCM_RIGHTS) is the owning fix and is not on
  master. A test-only `/bin/tty` is staged from `tests/fixtures/ssh-tty` but the allocation fails
  before it runs.

### C10 exec

- `do_exec`/`do_exec_pty` (`session.c:128`/`:126`) replace the session image. FreeBSD
  `kern_execve`/`do_execve` (`kern_exec.c:351`/`:458`) and the NetBSD exec family keep inherited
  descriptors; no readiness or socket identity change. Raven uses raven-spawn + `PROC_EXEC` (0x6a)
  and carries the fdmap.

## Matrix rows matched to the chain

| Row | Asserts chain steps | Exact expectation |
|---|---|---|
| S0 | C1 | guest accepts a TCP connection over the tap and echoes the exact payload |
| S1 | C1, C2 | `SSH-2.0-OpenSSH_10.6\r\n` (22 bytes) on a fresh connection within 10 s; a second fresh connection is the control |
| S2 | C2, C3 | one KEXINIT packet: `uint32 length=956`, `padding_length=9`, total 960, msg id 20, parse succeeds |
| S3 | C5 precondition, C9 | server log line `Accepted password for raven ... ssh2` (observed `from UNKNOWN port 65535`, a getpeername gap); wrong password exits 255 with `Permission denied` |
| S4 | C2, C7 | post-auth read returns `hostkeys-00@openssh.com` and the command output; no `EFAULT` |
| S5 | C5, C6, C8 | the client receives the channel-open confirmation on the authenticated connection (file/sid identity is K431's fixture) |
| S6 | C5 | the confirmation is message type 91 on the client's channel 0 |
| S7 | C1-C10 | `ssh raven@... cat /tmp/ssh-marker` returns exactly `K434-SSH-MARKER-2026-11-22\n`; then 10/10 command sessions |
| S8 | C9 | `ssh -tt ... tty` returns `/dev/pts/[0-9]+`; the `pty-alloc` dynamic case is K430's |

## State differences and which we follow

1. **Readiness model (C3/C4).** FreeBSD recomputes `kern_poll` per call over `selinfo`/`selrecord`;
   NetBSD uses `selnotify` on the same contract. Raven follows the level-triggered contract with two
   mechanisms: the live `Call::Poll` answer and a notification event. The event is a hint; the live
   answer is the verdict.
2. **Write semantics (C5/C6).** Both BSDs allow a short `sosend` count and `EAGAIN`/`EWOULDBLOCK`
   only when the send buffer is full on a nonblocking socket. Raven follows POSIX; success means
   enqueued, so delivery needs a separate readback test (S5).
3. **Read semantics (C7).** Both BSDs allow short reads; `EFAULT` is only for an invalid user
   buffer. K429 restored this; S4 asserts it.
4. **FD/OFD identity (C8).** Both BSDs share the open file description across dup/fork/exec. Raven's
   exec fdmap duplicates the event handle; the socket identity must be preserved.
5. **PTY (C9).** FreeBSD `pts_alloc` and NetBSD `pty_alloc_master`/`ptsopen` create the pair;
   `openpty` is the libutil convenience. Raven: ptyd is the scheme server; the current `ssh -tt`
   failure is at this step (K430), not in OpenSSH.
6. **Banner (S1).** The wire banner uses `SSH_VERSION` (`OpenSSH_10.6`), not `SSH_RELEASE`
   (`OpenSSH_10.6p2`). S1 asserts the source bytes.

## Rev 18 observations (S5-session and S7 output path)

1. **The previous S5-session EPIPE was the test stdin, not the socket.** With `stdin=/dev/null`,
   strace recorded `read(4</dev/null<char 1:3>>, "", 32768) = 0`; `sshbuf_read`
   (`addons/ssh/openssh/sshbuf-misc.c:340-353`) maps a zero-length read to `SSH_ERR_SYSTEM_ERROR`
   with `errno = EPIPE`, so OpenSSH logged `Broken pipe`. The network socket fd 3 stayed `ESTAB`.
   With a live stdin pipe there is no EPIPE; the client still does not complete and is killed at the
   15 s cap. S5-session records that completion failure, the sampled fd4 `socket:`/`pipe:` target
   and the `ESTAB` TCP peer.
2. **S7 executes the command but loses its stdout.** The guest serial log contains the exact marker
   `K434-SSH-MARKER-2026-11-22` 11 times (the single attempt plus 10/10), while every SSH client
   stdout is 0 bytes. The session stdout is on the guest console, not the SSH channel.
3. **S8 has two independent gaps.** The K430 `pty-alloc` qtest passes, but `ssh -tt ... tty` still
   fails the PTY request and the release image has no `tty` executable (`rsh: tty: command not
   found`). Rebuilding the static `sshd-session` against the merged libc did not change the result.

## Rev 19 observations (K436 scratch, S7 routing)

1. **K436 is not on `origin/master`.** Master is still `d50c35bb`; K436 is only
   `origin/dave/K436` `9d1a30cb`. It was verified on the local scratch branch
   `ken/K434-k436verify` `b86cc544` (not pushed). Its `pty-alloc`/`pty-calls` pass, but
   `ssh -tt ... tty` still returns `PTY allocation request failed on channel 0`.
2. **S7 routing is checked at both ends.** The master run has client stdout 0 bytes and
   `console_total_delta=0` (the output is on neither endpoint in that run). The K436 scratch run has
   client 0 bytes and the delayed console capture shows the exact marker 11 times. The S7-routing
   row records the client bytes, the per-attempt console max and the total console delta; it is red
   in both runs because the client never receives the marker.
3. **S8 remains red with K436.** The K436 libc qtests pass, but the SSH PTY request still fails and
   the image has no `tty` utility. Rebuilding the static `sshd-session` against K436 and reimaging
   did not change the client result.

## Rev 20 observations (K431/K436/K439 merged)

1. **S7 routing is fixed.** On master `1f2c0e64` the client receives the exact marker
   `K434-SSH-MARKER-2026-11-22\n` on the single attempt and all ten 10/10 attempts; the guest
   console marker delta is 0 (`max_console_new=0`, `console_total_delta=0`). The client still exits
   -9 after delivering, which remains S5-session's completion failure.
2. **S5 identity fixture fails before identity.** K431's `socket-write-identity` tap test runs, but
   the fixture shell fails with `rsh: tty:console: no such file or directory` and
   `socket-write-identity: shell status=1`; the `server` profile has no consd. S5-identity is FAIL.
3. **S8 still fails.** K436's `pty-alloc`/`pty-calls` pass, but `ssh -tt ... tty` still returns
   `PTY allocation request failed on channel 0`. A test-only `/bin/tty` is staged
   (`tests/fixtures/ssh-tty` via `profiles/release.d/89-ssh-tty.toml`), but the allocation fails
   before the command runs.
4. **S5-session** remains: no EPIPE, exit -9 at 15 s, fd4 socket/pipe, TCP peer `ESTAB`; the
   server-side handle is not exposed.

## Rev 21 observations (S7 exit/wall, S5 identity fixture)

1. **S5 identity fixture fixed in K434's Owns.** K431's static `server` image had no consd, so
   `rsh` failed at `tty:console`. Staging `raven-consd` and a consd service in
   `tests/fixtures/socket-write-identity/build-ssh.sh` makes the identity qtest pass
   (`socket-write-identity-tap: pass`).
2. **S7 bytes and routing pass; the gate exit does not.** The client receives the exact marker on
   the single attempt and 10/10, and the console delta is 0, but every attempt exits -9 at the 30 s
   cap (walls 30.0-30.05 s). S7-routing is PASS; S7's exit/wall assertion is FAIL.
3. **S8 waits for K438.** `pty-alloc`/`pty-calls` pass; `ssh -tt ... tty` still returns
   `PTY allocation request failed`. K438 (SCM_RIGHTS) is not on master.

## Gaps (not evidence)

- FreeBSD `soo_poll` is read (`sys_socket.c:282`); the protocol-specific `pr_sopoll` for TCP was not
  opened in this pass. The chain uses the generic `sowakeup`/`selwakeup` path, which is read.
- NetBSD `pty_alloc_master`/`pty_alloc_slave`/`ptmopen` are read; the termios line-discipline call
  chain inside `ptsopen` is only read to the `l_open` dispatch.
- The Raven chain (`libc -> scheme -> netd -> tcpip -> virtio-net`) is a separate donor chain with
  its own tickets (K431/K435/K433); this file fixes the OpenSSH/BSD side and the row mapping.
- The S5 "same file/sid" identity assertion belongs to K431's `tests/fixtures/socket-write-identity`
  fixture, which is not on this base; S5 records the client-side delivery fact and the Report records
  the identity gap.
- The S6 44-byte plaintext length is constructed by `channels.c:2198`/`:5085`; the client debug log
  exposes the message type and recipient channel, not the packet length. The matrix records the
  observable bytes and the Report records the length gap.
