<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Poll readiness

`poll(2)` and `ppoll(2)` are implemented in `raven-libc-platform/src/poll`. A scheme-backed
socket is registered through its `net:` notification event; the event is created after
`bind`/`listen`/`connect` and attached to the OFD.

The kernel event queue consumes a `UserEvent` when `EVQ_WAIT` delivers it. A stored readiness
mask is only a wake hint; it is never returned as `revents`. A scheme event can therefore wake a
`poll` with nothing queued, and a blocking read waits for the real data rather than turning that
hint into a fatal `EAGAIN`.

Kernel-backed sources (socketpair/`Duplex`, `Pipe`, `Tty`) do not use a stored mask at all: their
data handle is registered directly and the event queue reports its current kernel readiness.

`ppoll` converts the absolute `timespec` to the poll millisecond granularity. The atomic signal
mask swap is not implemented (PG-26); OpenSSH's packet path passes a NULL mask. The mask is
accepted but not swapped. `pselect` still returns `ENOSYS` until that atomic swap lands.

Across `execve`, the fd table is carried in the `__RAVEN_FDMAP` environment block. Version 2 of
that block includes the scheme event handle and a readiness scalar; the scalar is retained for
wire compatibility but is not accepted as proof of `POLLIN`/`POLLOUT`. Version 1 (the
`raven-spawn` form) is still decoded with zero event/readiness. The child uses the live event or
kernel state, and a queued packet is read by the blocking read path.

The donor comparison is NetBSD `sys/kern/sys_socket.c:soo_poll`,
`sys/kern/uipc_socket.c:sopoll`, `sys/kern/uipc_socket2.c:sowakeup` and
`sys/kern/sys_select.c:selrecord`/`selnotify`, plus `sys/kern/sys_pipe.c:pipe_poll`. `sopoll` and
`pipe_poll` re-check the live level on every call; the Raven data-handle registration follows
that for kernel-backed sources. No donor code is copied.

A blocking socket read treats an empty netd queue as "wait", not an error: after `EAGAIN` it
waits on the same per-thread event queue as `poll` and retries. A non-blocking socket still
returns `EAGAIN`. This is what makes a stale doorbell harmless to a POSIX-style reader.

`poll` registers one event-queue source per watched handle, merges the requested events and fans
the result back to every `pollfd` that named it, each from its own requested direction. An entry
with `events == 0` is not ready. A failed registration removes the ones already added.

The readiness doorbell is a `UserEvent`, not a scheme file; it is released with `HANDLE_CLOSE`,
not `CLOSE`.

A forked child inherits its parent's cached event queue handle. Because a queue allows one waiter,
`poll` treats `EVQ_WAIT` `EBUSY` as that inherited-queue case: it drops the cached handle, creates
a fresh queue for the child and retries once. The parent keeps its own queue; neither waiter sees
`EBUSY`.

The exec `FdMap` duplicates each inherited socket event handle (`HANDLE_DUP`) and records the
child's handle, so a parent that closes its accepted fd/event immediately after fork cannot
invalidate the child's readiness source. If the duplicate is unavailable, the original handle is
kept so the data fds still inherit.

The child entry of `fork` drops the inherited cached event queue immediately
(`raven_fork_child_reset`). The child's first `poll` creates its own queue. This is unconditional,
not only the `EBUSY` fallback, because a fork-only child (the post-auth session) must never share
the parent's queue even when the parent is not waiting at that instant.

For a scheme socket, `poll` now scans the current readiness with `Call::Poll` before sleeping and
again after a wake. The registration is only the wake hint; only directions that are currently
ready become `revents`. A spurious wake is retried with the remaining monotonic deadline, and
`timeout == 0` performs only the immediate scan. Socketpairs, pipes and TTYs continue to use the
kernel data-handle level; they do not use the net query.

## K248 fork differential

`tests/fixtures/post-auth-fork` builds the same C source twice: the host build
(`build-host.sh`, Linux/glibc) and the Raven static build (`build.sh`). The host baseline captures
`strace -f -tt -e trace=network,read,write,poll,ppoll,clone,fork,vfork,execve` (plus `close` for
the parent-close phase). The Raven qtest uses the `server` initfs so the fork topology runs
without the brush shell; the service stack (init/svcd/initfsd/vfsd/netd) is unchanged.

After the K250 file-reference and K245 event fixes, the two traces agree: the parent `close(5)`
drops one reference, the child `poll(5)` wakes after the parent's `write(4, "POSTAUTH 5 hello")`,
reads 16 bytes, writes `POSTOK 2 ok`, and both sides exit 0. There is no divergence in this
fork/listener/child-poll topology. The qtest markers are the durable regression check:

```text
post-auth-fork: child poll ok
post-auth-fork: child read ok
post-auth-fork: child write ok
post-auth-fork: parent response ok
post-auth-fork: ok
init exited (0)
```
