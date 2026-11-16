<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `filesystem.record-lock` defect - P5-D2

The row remains `missing` because no layer carries a POSIX record lock:

* The kernel has no `FCNTL` syscall surface for `F_GETLK`/`F_SETLK`/`F_SETLKW`.
* The scheme `Ctx` carries effective credentials but no process id or lock-owner identity, so
  a VFS server cannot implement process-owned locks or release them on process exit.
* `raven-proto-fs` has no `LOCK`/`UNLOCK` control call. The libc `lockf` path and the
  `fcntl` record-lock commands return `ENOSYS` until PG-21.

Ticket: `P5-D2` (profile gap PG-21). Executable evidence requires the kernel lock-owner model
first; a server-only lock table keyed by open-file description would not be POSIX.
