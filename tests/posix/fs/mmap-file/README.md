<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `filesystem.mmap-file` defect - P5-D1

The row remains `partial` because there is no file-backed mapping primitive:

* `libc/ravenlibc/crates/raven-libc-platform/src/mem/mmap.rs` deliberately returns `ENODEV`
  for a file descriptor mapping until VFS `OP_MMAP` exists (PG-36).
* The kernel has the pager/supply plumbing (`MEMOBJ_CREATE`, `PAGER_SUPPLY`,
  `kernel/src/memory/uspace.rs`), but no syscall that creates a pager object bound to a VFS
  file handle and offset.
* Without that binding there is no way for a qtest to observe private/shared file mapping
  updates or write-back ordering.

Ticket: `P5-D1` (profile gap PG-36). The executable tests for this row must wait for the
`OP_MMAP`/pager attachment contract.
