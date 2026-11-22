<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SCM_RIGHTS fd passing over AF_UNIX (K438)

Status: **green.** A guest passes a regular file, a pty master, a connected TCP socket and two
descriptors in one message over an `AF_UNIX` `SOCK_STREAM` pair. The receiving side checks the
control record and writes/reads through each received descriptor; the host peer proves the TCP
descriptor reaches the wire. A kernel mutation that ignores `SCM_RIGHTS` makes the test fail, and
the original red case was a real libc defect: an adopted socket degraded to `OfdKind::Unknown`, so
a blocking read returned `EAGAIN` instead of waiting for the host acknowledgement.

This is the contract OpenSSH privsep uses: `monitor_fdpass.c:43` `mm_send_fd` builds a `sendmsg`
with `SCM_RIGHTS` (`:70`) and `:136` receives it; the pty master is the descriptor the session
needs from the monitor.

## Chain

| # | Step | Raven |
|---|---|---|
| 1 | Guest `sendmsg` with a C `cmsghdr` | `libc/ravenlibc/crates/raven-libc/src/socket/socketpair.rs:84`; `raven-libc-net/src/socket.rs:362` `Net::sendmsg` |
| 2 | Parse the C control and resolve each fd to its kernel handle | `raven-libc/src/socket/duplex.rs:136` `unix_sendmsg`, `:271` `encode_control` (level/type/len checks, `DUPLEX_MAX_HANDLES = 16`) |
| 3 | Kernel `duplex_sendmsg` | `kernel/src/syscall/duplex.rs:140` `sendmsg`; `:263` `parse_control`; `handle_xfer.rs:157` `clone_transferable` (`SchemeFile`, `Pipe`, `Duplex`, ...) |
| 4 | Queue the message and handles | `kernel/src/process/duplex.rs:730` `send` / `write_stream` |
| 5 | Peer `recvmsg` | `kernel/src/process/duplex.rs:629` `recv` (control capacity checked before data), `kernel/src/syscall/duplex.rs:154` `recvmsg`, `:306` `install_handles` |
| 6 | Install the received handles as descriptors | `raven-libc/src/socket/duplex.rs:169` `unix_recvmsg`; `:311` `parse_raw_control`; `Platform::adopt` classifies the handle |
| 7 | Return the C cmsg | `cmsg::encode_cmsg(SOL_SOCKET, SCM_RIGHTS, ...)`; `Net::recvmsg` sets `control_len`; the C wrapper writes `msg_controllen` |

The kernel checks the control capacity **before** consuming data or handles: a too-small buffer
returns `EMSGSIZE` and leaves the message queued. A message with no control returns
`msg_controllen = 0` and no cmsg.

## Adopted socket classification

A transferred scheme handle carries the data capability only. The rail's `Call::Event` handle is a
separate capability, and the `file:`/`net:`/`pty:` server knows the object type behind the handle,
not the receiver's fd table. `Platform::adopt` therefore probes the handle once:

* `Call(GetMode)` still identifies a tty (pty master/slave);
* `FSTAT` identifies a `netd` socket from `S_IFSOCK`;
* an adopted socket gets a fresh `Call::Event` notification handle, so `read` keeps the normal
  blocking policy (`poll_signaled` is true and `wait_readable` runs) instead of returning the
  backend's `EAGAIN`.

Without the socket probe the TCP phase is red: `fdpass: fail tcp received read errno=11`.

## Donor reconciliation

| Topic | Illumos (CDDL, read-only) | BSD (permissive) | Linux (GPL, read-only) | Decision |
|---|---|---|---|---|
| `sendmsg`/`recvmsg` syscall entry | `usr/src/uts/common/sys/socketvar.h:672` `sop_sendmsg`, `:706`/`:950` `sosendmsg` | FreeBSD `sys/kern/uipc_syscalls.c:724` `kern_sendit`, `:889` `kern_recvit`; NetBSD `sys/kern/uipc_syscalls.c:519` `do_sys_sendmsg_so`, `:643` `do_sys_sendmsg`, `:701` `sys_recvmsg` | `net/socket.c` `__sys_sendmsg`/`__sys_recvmsg` | Behaviour read; Raven first-party |
| `SCM_RIGHTS` internalize/externalize | sockfs ancillary path (`sockipc.c`, `socketvar.h`) | FreeBSD `sys/kern/uipc_usrreq.c:3967` `unp_internalize`, `:3778` `unp_externalize`; NetBSD `uipc_usrreq.c:1558` `unp_internalize`, `:1428` `unp_externalize` | `net/core/scm.c` `scm_send`/`scm_detach_fds` | No code copied; Raven `parse_control`/`install_handles` is first-party |
| fd duplication and close | n/a | FreeBSD `sys/kern/kern_descrip.c` `fdcopy`/`closef`/`fget`/`fdrop`; NetBSD `kern_descrip.c` `fd_dup`/`fd_copy`/`closef` | `fs/file.c` | Kernel `clone_transferable`/`insert_derived` own the handle lineage |
| Received fd type and readiness | file ops and poll entries travel with `struct file` | FreeBSD `fget` keeps `f_ops`; `unp_externalize` installs the same `struct file` | `scm_detach_fds` installs the same `struct file` | **Divergence:** the scheme ABI transfers one capability, not the file ops. `Platform::adopt` recovers `S_IFSOCK` from `FSTAT` and asks `Call::Event` for a receiver-local readiness handle. Recorded for review; no donor code copied. |
| Control-buffer overflow | `EMSGSIZE` | `unp_externalize` `EMSGSIZE` | `scm_detach_fds` `MSG_CTRUNC`/`ETOOMANYREFS` | Raven returns `EMSGSIZE` and leaves the message queued |

Donor revisions: FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`, illumos-gate
`586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`, NetBSD workspace checkout (BSD-2-Clause), Linux
read-only. No donor code was copied. The public header `sys/socket.h` keeps the musl MIT notice
that already covered the `CMSG_*` macros.

## Test

`tests/libc/c/libc-fdpass.c`, registered in `tests/libc/target/cases.tsv`, run by
`tests/qtest/cases/fdpass.toml`. The fixture builds a `libc-test` image with the static virtio-net
stack and `ptyd`; a host peer connects through the QEMU host-forward on port 23082.

| Phase | Passed fd | Received fd | cmsg_len | msg_controllen | Bytes |
|---|---|---|---|---|---|
| regular file (`/tmp/fdpass-file`) | 3 | 6 | 20 | 20 | `original` + ` passed` = 15 |
| pty master (`openpty`) | 3 (slave 4) | 7 | 20 | 20 | `pty-passed\n` (11), `pty-back\n` (9) |
| connected TCP socket | 4 | 7 | 20 | 20 | `fdpass-tcp-payload` / `fdpass-tcp-ack` |
| two regular files | 7 and 8 (received) | 7, 8 | 24 | 24 | `first+a`, `second+b` |
| truncated control (`msg_controllen = 8`) | — | — | — | — | `EMSGSIZE` (errno 90), message preserved |
| absent control | — | — | — | 0 | `plain` (5 bytes), no cmsg |

Green serial:

```text
fdpass: start
fdpass: file fd=3 received=6 cmsg_len=20 controllen=20
fdpass: file roundtrip=ok
fdpass: pty master=3 slave=4 received=7 controllen=20
fdpass: pty roundtrip=ok
fdpass: tcp fd=4 received=7 controllen=20
fdpass: tcp roundtrip=ok
fdpass: two fds first=7 second=8 cmsg_len=24 controllen=24
fdpass: two fds roundtrip=ok
fdpass: truncated errno=90 ok=1
fdpass: absent controllen=0 ok=1
fdpass: ok
init exited (0)
```

Host peer result: `ok payload=b'fdpass-tcp-payload' ack=b'fdpass-tcp-ack'`.

## Mutation

The original red run (before the socket probe) and the post-fix mutation both fail at the same
assertion:

```text
fdpass: tcp fd=4 received=7 controllen=20
fdpass: fail tcp received read errno=11
```

The mutation disables `socket_probe` in `Platform::adopt`; the case exits 1 and the qtest forbids
`fdpass: fail`. Restored, the case is green.

## Commands

```sh
timeout 480 cargo xtask qtest --case fdpass --arch x86_64
timeout 300 cargo xtask qtest --case pty-calls --arch x86_64 --no-build
timeout 900 make check-fast
```

The built sysroot needs its `usr/lib/libc.so` script and `libc.so.1` link in the standard
`usr/lib/libc.so -> libc.so.1 -> ../../lib/libc.so.1` shape; the generated tree in this workspace
had a self-referential script, repaired as a build artifact only (no source change).

## Open gaps

* The `ssh -tt ... tty` external acceptance is K195/Ken's gate; it needs the SSH image and the
  K194 tap helper. Not claimed here.
* `ptsname`/`ptsname_r` are K436's dependency work; this branch merges `origin/dave/K436` so the
  ticket's `pty-calls` command exists and runs.
