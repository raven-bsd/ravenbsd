<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# du thread/TLS and printer-thread hang (K134/K148)

Status: **OBSERVED at base `0b6fc0d4`.** K136 fixed the `du` thread/TLS abort (the uutils link
script maps `PT_PHDR`/`PT_TLS`). The remaining `du /tmp/du` hang is the std mpsc hand-off
between the traversal thread and the printer thread; it is not a `safe_du`/VFS lock. On
RavenBSD `du` now traverses synchronously and drains the same result queue after the walk.

## 1. K134 abort (fixed)

The 2026-11-19 `du` was `ET_EXEC` with no `PT_PHDR`; `__raven_init_main_thread` could not find
`PT_TLS` through `AT_PHDR`, so the first `pthread_create` aborted with
`current thread handle already set during thread spawn`. K136's linker mapping fixed that:
`readelf -l build/x86_64/uutils/du` now shows `PHDR` and `TLS`, and the thread starts.

## 2. K148 post-fix hang

`tests/qtest/cases/base-tools-du.toml` creates `/tmp/du/a` and runs `du /tmp/du`; the size line
never arrives. With temporary markers in `uumain`, `safe_du`, `DirFd::open`, `Stat::new`,
`du_regular`, the mpsc send and the printer thread, the trace is:

```text
K148 uumain start
K148 spawn printer
K148 printer thread start
K148 before Stat::new /tmp/du/a
K148 after Stat::new ok=false
K148 error branch
K148 error branch sending        <- blocks here
```

The printer child is alive and blocked on `rx.recv()`; the main thread is blocked in
`print_tx.send`. `safe_du` itself is not the blocker: when the printer thread is removed, the
same run reaches:

```text
K148 DirFd::open before path="/tmp/du" flags=OFlag(O_CLOEXEC | O_DIRECTORY)
K148 DirFd::open done fd=OwnedFd { fd: 3 }
K148 read_dir entries=1 path=/tmp/du
K148 stat_at start name="a"
K148 stat_at done name="a"
K148 loop done path=/tmp/du
K148 safe_du returned
1	/tmp/du
```

The `after Stat::new ok=false` line is the redirected one-file run that took the error branch
(`/tmp/du/a` is a file); the key block is the mpsc wake, not the filesystem call. The stderr
markers only surfaced when stdout was redirected; on the console the process had not flushed
them before the hang.

Classification: **std mpsc/printer-thread hand-off**, not `safe_du`, `DirFd`/vfsd or the
ramfs stat path. Handoff: the mpsc/futex wake between two std threads to the libc/kernel owner
(Stuart/Kevin); this ticket does not edit libc or the kernel.

## 3. RavenBSD path

`userland/uutils/src/uu/du/src/du.rs` keeps the same `mpsc` queue but, on `target_os =
"ravenbsd"`, does not spawn the printer thread; it runs the traversal on the main thread and
calls `stat_printer.print_stats(&rx)` after `drop(print_tx)`. Other targets keep the streaming
printer thread. Output and exit semantics are unchanged; the visible difference is that a
RavenBSD listing is printed after the walk instead of while it runs.

The existing `base-tools-du` case is the regression: it still uses the 30 s step and the same
`du /tmp/du` command, and now prints `1 /tmp/du` and exits 0. No duplicate case is added.

## 4. Donor reconciliation

| Topic | Illumos finding | BSD file/licence | Linux finding | Decision |
|---|---|---|---|---|
| `du` traversal | `du` walks with `fts`/`ftw` and prints incrementally; no Raven implementation copied. | FreeBSD/NetBSD `du` use `fts` with `FTS_NOSTAT`/`FTS_PHYSICAL`; permissive BSD, not copied. | GNU `du` uses `fts` and a single-threaded printer by default. | Keep uutils `safe_du`; use a synchronous printer on RavenBSD until the mpsc hand-off is fixed. |
| Threaded printer | Not applicable. | Not applicable. | Read-only reference. | The thread/queue is first-party uutils std; the RavenBSD fallback is a local scheduling workaround, not donor code. |
