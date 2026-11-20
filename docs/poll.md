<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Poll readiness

`poll(2)` and `ppoll(2)` are implemented in `raven-libc-platform/src/poll`. A scheme-backed
socket is registered through its `net:` notification event; the event is created after
`bind`/`listen`/`connect` and attached to the OFD.

The kernel event queue consumes a `UserEvent` when `EVQ_WAIT` delivers it. A socket may still
hold level-ready bytes after that delivery (for example, a second TCP packet was already queued
behind the first one). To preserve POSIX level semantics, `Ofd` keeps the delivered readiness
mask and the next `poll`/`ppoll` replays it once. A read that drains the queue then blocks again,
so a caller that polls without reading gets one spurious readiness, not a busy loop.

`ppoll` converts the absolute `timespec` to the poll millisecond granularity. The atomic signal
mask swap is not implemented (PG-26); OpenSSH's packet path passes a NULL mask. The mask is
accepted but not swapped. `pselect` still returns `ENOSYS` until that atomic swap lands.

Across `execve`, the fd table is carried in the `__RAVEN_FDMAP` environment block. Version 2 of
that block includes the scheme event handle and the one-shot readiness mask, so a child that
inherits an accepted socket can replay a packet that was already queued before the exec. Version
1 (the `raven-spawn` form) is still decoded with zero event/readiness. The child replays at most
one level per inherited fd; a read that drains the queue then blocks normally, and a second poll
without a read does not spin.

The donor comparison is NetBSD `sys/kern/sys_socket.c:soo_poll`,
`sys/kern/uipc_socket.c:sopoll`, `sys/kern/uipc_socket2.c:sowakeup` and
`sys/kern/sys_select.c:selrecord`/`selnotify`. `sopoll` re-checks the socket level on every call;
the Raven replay provides the same outcome for a queued level that the one-shot event already
delivered. No donor code is copied.

A blocking socket read treats an empty netd queue as "wait", not an error: after `EAGAIN` it
waits on the same per-thread event queue as `poll` and retries. A non-blocking socket still
returns `EAGAIN`. This is what makes a stale doorbell harmless to a POSIX-style reader.

`poll` registers one event-queue source per watched handle, merges the requested events and fans
the result back to every `pollfd` that named it, each from its own requested direction. An entry
with `events == 0` is not ready. A failed registration removes the ones already added.

The readiness doorbell is a `UserEvent`, not a scheme file; it is released with `HANDLE_CLOSE`,
not `CLOSE`.
