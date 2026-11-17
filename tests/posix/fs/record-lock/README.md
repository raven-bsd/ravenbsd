<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `filesystem.record-lock`

The kernel provides POSIX advisory byte-range locks through the raw `RECORD_LOCK` syscall
(`0x0C`) and the libc `fcntl(F_GETLK/F_SETLK/F_SETLKW)` wrapper. Both paths share one lock
table keyed by `(pid, server, inode)`; a close of any descriptor for the file releases the
process's locks for that file, and process exit releases all of them.

The qtest (`posix-fs-record-lock`) runs one parent and short-lived children:

* raw syscall phases: free `F_GETLK`, child conflict, conflicting-owner `F_GETLK`, blocking
  `F_SETLKW`, close release and exit release;
* libc `fcntl` phases on a separate byte range: free `F_GETLK`, conflicting-owner `F_GETLK`,
  `EAGAIN` from `F_SETLK`, a raw child seeing a libc-held lock, libc `F_SETLKW` blocking until
  the child exits, libc close release and a libc lock released on child exit.

The fixture links `build/x86_64/libc.a`; `build.sh` refreshes the archive with
`cargo xtask libc-pack --arch x86_64` when the libc sources are newer.

Ticket: `P5c` (raw kernel path) and `P5c2` (libc `fcntl` glue). Executable evidence:
`cargo xtask qtest --case posix-fs-record-lock --arch x86_64`.
