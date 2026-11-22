<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# libc fork and PTY primitives

K196 wires the POSIX process and terminal primitives OpenSSH needs onto first-party
kernel/server primitives. **OBSERVED** by the `libc-fork` and `libc-forkpty` qtests.

## fork, `_Fork` and `vfork`

`fork` is a naked per-architecture entry that captures the caller's callee-saved frame and calls
the Rust helper. The helper:

1. calls `PROC_FORK(self, 0)`, which clones the process copy-on-write (no thread);
2. maps the child process handle to its pid with `PROC_HANDLE_INFO`;
3. starts the child's first thread with `THREAD_CREATE_EX` at a child trampoline, on the captured
   stack, with the caller's TLS base;
4. closes both the child process handle and the thread handle.

The trampoline restores the captured callee-saved registers and returns zero from the same
`fork` call; the parent returns the child pid. `waitpid` uses the kernel's parent/child relation
and reports the encoded status.

`_Fork` and `vfork` are the same primitive (`vfork` has no separate address-space suspension on
Raven). The obsolete `ENOSYS`/`procd` comments in the shell path are gone.

The spawn loader (`raven-spawn`) now maps the initial stack and writable program segments
**private**, not `MAP_SHARED`. A shared mapping cannot be copy-on-write cloned by `PROC_FORK`; a
forked child would overwrite the parent's frames. The parent's scratch mapping still uses the
shared object to populate the image before the child mapping is installed.

Limitations: only the resume frame and TLS base are carried (the kernel does not snapshot the
interrupted thread context); atfork handlers and multi-thread fork bookkeeping in
`raven-libc-thread` are not on this release path.

## PTY

`posix_openpt` opens the clone device `pty:ptmx`; `ptyd` allocates the lowest free pair id and
installs the scheme handle as a descriptor. The pair id is read back from the endpoint inode
(`2N+1` master) and kept in a small fixed descriptor table; `ptsname_r` formats `pty:slave/<id>`.
`grantpt` and `unlockpt` are successful no-ops because the pty server owns both ends. The last
close of a pair releases the unit, so concurrent processes never share a pty and churn does not
grow the server table.

`openpty` opens the slave through the same `pty:` client, and `forkpty` runs `login_tty` in the
child: `setsid`, the native `PROC_CTTY_SET` (`TIOCSCTTY`), and `dup2` of the slave onto stdio.
The kernel's controlling-terminal policy accepts the `pty` scheme alongside `tty`/`ttyd`.
Closing the last slave duplicate hangs the pair up, so the master reads EOF instead of looping on
`EAGAIN`.

`ptsname_r` returns a scheme-qualified name (`pty:slave/<id>`); `openpty` opens it through the pty
client. A bare `open(ptsname(fd))` would need the pty scheme mounted into the file namespace,
which this release does not do.

## Evidence

```text
libc-fork: parent pid=13
libc-fork: child value=99
libc-fork: wait status=7
libc-fork: cow ok
libc-fork: fd ok
libc-fork: ok

libc-forkpty: parent pid=13 name=pty:slave/1
libc-forkpty: master=pty-child
libc-forkpty: ok
```

Both cases pass on `x86_64` and on `aarch64` (the case `pre` takes its architecture from
`RAVEN_QT_ARCH`): the aarch64 thunk saves/restores x19-x28/x29/x30, keeps the resume SP
16-byte aligned, and passes the `TPIDR_EL0` thread pointer to `THREAD_CREATE_EX`. The same PTY
master/slave/EOF path runs through `ptyd` on both architectures.
