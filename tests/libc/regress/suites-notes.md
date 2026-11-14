# How exit-status suites map to programs

<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

| Suite | Framework | Program identity | Source | Pass |
|---|---|---|---|---|
| `openbsd-libc` | exit-status | `<dir>/<PROG>` | `PROG.c` or `SRCS` | exit 0 |
| `openbsd-libm` | exit-status | same | same | exit 0 (vendor absent in this checkout) |
| `openbsd-libpthread` | exit-status | same | same | exit 0 (vendor absent in this checkout) |
| `picolibc` | exit-status | `test-*.c` without `.c` | one file | exit 0, except overrides |
| `mlibc` | exit-status | `ansi/name`, `posix/name`, `rtld/name` | `tests/<name>.c` | exit 0; `rtld` is `needs-dynamic` |
| `pjdfstest` | TAP | `<group>/<script>` | `tests/<group>/<script>.t` | `ok`, pending the FS lane |
| `ltp` | exit-status | external prebuilt binary | external | 0 pass; 4/5 skip; 1/2 fail |

Every program carries `expect_exit` (default 0) and `args` (default empty). A program killed by a
signal is `crash:<signal>`; a timeout is `timeout after <s>s`; an exit other than the expected one
is `behaviour:expected exit N, got M`. No exit-status row is ever a pass because a file was
missing: only the observed exit status is used.
