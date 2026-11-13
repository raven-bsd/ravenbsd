<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-libc-stdio

The `FILE` layer of ravenlibc: the musl buffer state machine with indices, the per-stream
recursive owner-tid lock, the open-file list, the fd/memory/cookie backends, wide stdio,
`<stdio_ext.h>`, and the `FileSink`/`FileSource` glue to `raven-libc-fmt`.

The specification is `architecture/raven-libc/donor-map/04-stdio.md` (sections 3.4-3.7, 4.2 and
4.3). The FILE core is transpiled from `musl:src/stdio/*` (MIT via `musl/COPYRIGHT`, pinned at
`7813e9119eb5fe681b6750b8a033a0490bca0941`); `fcloseall` is the FreeBSD BSD-2 file; `buf.rs`'s
`smakebuf`/flush-before-read policy, `funopen`, `glue_io` and the wide printf/scanf glue are
CLEANROOM, read from the OpenBSD/NetBSD design without copying.

## Seams

`SysFile` is every system-call-shaped operation, `Lock` is every blocking wait, `Env` is their
combination. The crate has no syscall number, no `errno` cell and no allocator of its own: the
shell (`raven-libc`, C60) implements the traits over C55's descriptor layer and C57's futexes,
and installs C49's allocator through `alloc`.

## Constants and policy

`UNGET = 8`, `BUFSIZ = 4096`, `STD_BUF = 1024`, `UNBUF_PRINT_BUF = 1024` (H-19). `stdin` and
`stdout` own `UNGET + 1024` bytes inside `StdStreams`; `stderr` is unbuffered. `fopen` streams
carry `F_LAZYBUF` and size their buffer from `st_blksize` (clamped to 512..65536) at the first
operation. `stdout` keeps the lazy, inverted tty probe: line buffering starts on and is turned
off by the first write to a non-terminal (A-stdio-6).

## Documented hazard: flush-before-read and the lock order

Flush-before-read follows the BSD `refill.c` shape: the input operation already holds its FILE
lock when the refill point takes the open-file-list lock and then one writer lock at a time. A
concurrent `fflush(NULL)` walks in the documented list-before-FILE order; the two paths can
contend in opposite order if an application mixes them from two threads. The area file asks
for the BSD rule, so the behaviour is kept; applications that read a terminal while another
thread flushes the world should serialise with `flockfile`/`funlockfile`.

## Documented hazard: the omitted locked-file list

`flockfile` records only the nesting count, not the per-thread list of locked FILEs (open item
O7). A thread that exits while it holds an explicit `flockfile` lock leaves that stream locked;
later users block. This is the documented default of the area file; callers must not leave
explicit locks held across thread exit.

## Tests

```
cargo test -p raven-libc-stdio
cargo test -p raven-libc-stdio --release
cargo xtask libc-oracle diff stdio-musl
```

The unit tests use the fake `Env` of `src/fake_ut.rs` (an in-memory descriptor table, a tty
kind, a pipe kind, short read/write injection and a lock-order detector). `tests/donor_*.rs`
ports the NetBSD/FreeBSD/OpenBSD/glibc expected values listed in `donor-map/04` 4.7. The
`stdio-musl` oracle family compiles the musl FILE core against `tests/libc/oracle/shim/stdio`
and runs both sides over one in-memory file system; see the C56 report for the closure and the
excluded cells.
