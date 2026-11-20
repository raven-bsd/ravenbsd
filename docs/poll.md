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
accepted but not swapped.

The donor comparison is NetBSD `sys/kern/sys_socket.c:soo_poll`,
`sys/kern/uipc_socket.c:sopoll`, `sys/kern/uipc_socket2.c:sowakeup` and
`sys/kern/sys_select.c:selrecord`/`selnotify`. `sopoll` re-checks the socket level on every call;
the Raven replay provides the same outcome for a queued level that the one-shot event already
delivered. No donor code is copied.
