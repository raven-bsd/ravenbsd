<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SCM_RIGHTS fd passing over AF_UNIX (K438)

Status: **the core fd-passing contract is green; no product change was required.** The test passes
a regular file, a pty master and two descriptors in one message, asserts the control fields and
the truncated/absent-control errno, and writes/reads through each received descriptor. A kernel
mutation that ignores `SCM_RIGHTS` makes the test fail. The connected-TCP-socket phase from the
ticket is **not run** because the network-profile fixture build is blocked by a pre-existing
`xtask libc-pack` archive error; the regular file and pty master cover the same `SchemeFile`
transfer path (the TCP socket is a netd `SchemeFile`).

This is the contract OpenSSH privsep uses: `monitor_fdpass.c:43` `mm_send_fd` builds a `sendmsg`
with `SCM_RIGHTS` (`:70`) and `:136` receives it; the pty master is the descriptor the session
needs from the monitor.

## Chain

| # | Step | Raven |
|---|---|---|
| 1 | Guest `sendmsg` with a C `cmsghdr` | `libc/ravenlibc/crates/raven-libc/src/socket/recv.rs` / `sendmsg` wrapper; `raven-libc-net/src/socket.rs:362` `Net::sendmsg` |
| 2 | Parse the C control and resolve each fd to its kernel handle | `raven-libc/src/socket/duplex.rs:136` `unix_sendmsg`, `:271` `encode_control` (level/type/len checks, `DUPLEX_MAX_HANDLES = 16`) |
| 3 | Kernel `duplex_sendmsg` | `kernel/src/syscall/duplex.rs:140` `sendmsg`; `:263` `parse_control`; `handle_xfer.rs:157` `clone_transferable` (SchemeFile, Pipe, Duplex, ...) |
| 4 | Queue the message and handles | `kernel/src/process/duplex.rs:730` `send` / `write_stream` |
| 5 | Peer `recvmsg` | `kernel/src/process/duplex.rs:629` `recv` (control capacity checked before data), `kernel/src/syscall/duplex.rs:154` `recvmsg`, `:306` `install_handles` |
| 6 | Install the received handles as descriptors | `raven-libc/src/socket/duplex.rs:169` `unix_recvmsg`; `:311` `parse_raw_control`; `platform.adopt` probes the kind |
| 7 | Return the C cmsg | `cmsg::encode_cmsg(SOL_SOCKET, SCM_RIGHTS, ...)`; `Net::recvmsg` sets `control_len`; the C wrapper writes `msg_controllen` |

The kernel checks the control capacity **before** consuming data or handles: a too-small buffer
returns `EMSGSIZE` and leaves the message queued. A message with no control returns
`msg_controllen = 0` and no cmsg.

## Donor reconciliation

| Topic | Illumos (CDDL, read-only) | BSD (permissive) | Linux (GPL, read-only) | Decision |
|---|---|---|---|---|
| `sendmsg`/`recvmsg` syscall entry | `usr/src/uts/common/sys/socketvar.h:672` `sop_sendmsg`, `:706` `sosendmsg`, `:950` `sosendmsg` | FreeBSD `sys/kern/uipc_syscalls.c:724` `kern_sendit`, `:889` `kern_recvit`; NetBSD `sys/kern/uipc_syscalls.c:519` `do_sys_sendmsg_so`, `:643` `do_sys_sendmsg`, `:701` `sys_recvmsg` | `net/socket.c` `__sys_sendmsg`/`__sys_recvmsg` (outcome only) | Behaviour read; Raven first-party |
| `SCM_RIGHTS` internalize/externalize | sockfs ancillary path (behaviour) | FreeBSD `sys/kern/uipc_usrreq.c:3967` `unp_internalize`, `:3778` `unp_externalize`; NetBSD `uipc_usrreq.c:1558` `unp_internalize`, `:1428` `unp_externalize` | `net/core/scm.c` (outcome only) | No code copied; the Raven `parse_control`/`install_handles` is first-party |
| fd duplication/ownership | n/a | FreeBSD `sys/kern/kern_descrip.c` `fdcopy`/`closef`; NetBSD `kern_descrip.c` `fd_dup`/`fd_copy` | `fs/file.c` (outcome) | The kernel's `clone_transferable`/`insert_derived` own the handle lineage |
| Control-buffer overflow | `EMSGSIZE` | `unp_externalize` `EMSGSIZE` | `scm_detach_fds` `MSG_CTRUNC`/`ETOOMANYREFS` | Raven returns `EMSGSIZE` and leaves the message queued |

Donor revisions: FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`, illumos-gate
`586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`, NetBSD workspace checkout (BSD-2-Clause). No donor
code was copied.

## Test and evidence

`tests/libc/c/libc-fdpass.c`, registered in `tests/libc/target/cases.tsv`, run by
`tests/qtest/cases/fdpass.toml` on the `libc-test` profile. Green run (15.20 s):

```text
fdpass: start
fdpass: file fd=3 received=6 cmsg_len=20 controllen=20
fdpass: file roundtrip=ok
fdpass: pty master=3 slave=4 received=7 controllen=20
fdpass: pty roundtrip=ok
fdpass: two fds first=7 second=8 cmsg_len=24 controllen=24
fdpass: two fds roundtrip=ok
fdpass: truncated errno=90 ok=1
fdpass: absent controllen=0 ok=1
fdpass: ok
init exited (0)
```

The pty phase disables `ECHO` on the slave first (`tcsetattr`), so the reverse roundtrip reads the
bytes the slave wrote, not the line-discipline echo.

Mutation: replacing the kernel `sendmsg` `parse_control` with an empty handle vector (temporary
edit, restored) makes the test fail at the first file `recvmsg`; the case reports `fdpass: fail file
recvmsg` and no assertion matches. The restored tree is green again.

## Open gaps

* **Connected TCP socket fd**: not run. The `network-config-test` fixture image build fails in
  `cargo xtask libc-pack` (`libraven_libc.a not found` for the tiny `libc-a` profile) — a
  pre-existing build-system issue outside this ticket's Owns. The file and pty master prove the
  `SchemeFile` transfer; a TCP socket is the same object kind through netd.
* **`ssh -tt ... tty` external gate**: owned by K195/Ken; it needs the SSH image and the K194 tap
  helper (`iptables`/`dnsmasq` absent here). Not claimed.
* `pty-calls` (the ticket's second test line) does not exist in this tree; `pty-alloc` is the
  merged pty regression and it passes.
