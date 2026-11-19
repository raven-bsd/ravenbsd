<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `du` thread-spawn abort: missing TLS module from `AT_PHDR=0`

Status: **PARTIAL — K136 (`d1ad5ca8`) fixes the AT_PHDR/thread-spawn abort; the remaining `du`
traversal hang is BLOCKED and handed off to Mel.** The abort analysis below is kept as the K136
history; the post-K136 marker trace is at the end.

## Reproduction

`tests/qtest/cases/base-tools-du.toml` boots the `shell-test` profile, creates `/tmp/du/a` and
runs `du /tmp/du`. On the current tree (`af0e1a7b` plus the K134 branch), with the staged uutils
rebuilt against the current libc:

```text
raven# du /tmp/du
process: child exited (134)
fatal runtime error: current thread handle already set during thread spawn, aborting
```

The first `base-tools-du` run (before the uutils rebuild) failed earlier with
`failed to spawn thread: Os { code: 38, ... }` because the staged `du` dated from 2026-11-17 and
did not match the current libc; after `cargo xtask build uutils`, the exact K134 abort appears.

## Trace

1. `build/x86_64/uutils/du` is `ET_EXEC`, 5 program headers, **no `PT_PHDR`**. The first `LOAD`
   starts at file offset `0x1000`; the ELF/program-header bytes at file offset `0x40` are not
   covered by any `LOAD`.
2. The kernel sets `AT_PHDR = elf.phdr_vaddr().unwrap_or(0)` (`kernel/src/process/mod.rs`).
   `raven-elf::phdr_vaddr()` returns `None` when there is no `PT_PHDR` and no load segment
   contains `e_phoff`; the K134 binary matches that case, so **`AT_PHDR=0`**.
3. `__raven_init_main_thread` (`libc/ravenlibc/crates/raven-libc/src/pthread/...`) scans the
   program headers through `AT_PHDR` to find `PT_TLS`. With `AT_PHDR=0` it records **no TLS
   module**; a temporary marker printed `K134 main tls none`.
4. `init_main_thread` still builds a TCB/TLS block, but with zero modules. The main thread's std
   TLS reads inside the zeroed `builtin_tls` buffer by luck; a new thread's TLS block is a fresh
   stack mapping without the module image or the compiler's expected zero span. `std`'s
   `CURRENT`/`ID` `#[thread_local]` values read nonzero, so `set_current` returns `Err` and
   `ThreadInit::init` runs `rtabort!("current thread handle already set during thread spawn")`.
5. The FS base is correct: a temporary marker in `thread_start` printed `K134 child tp=child`
   (the child TCB, tid > 100), so this is **not** the kernel's TLS-base install race.

## Tested fix (outside Owns)

The uutils link script `userland/uutils/x86_64.ld` omits the file header and program headers from
the first load segment. Adding them:

```ld
text PT_LOAD FILEHDR PHDRS FLAGS(5);
```

makes `AT_PHDR` name the mapped header table; the temporary probe then printed
`K134 main tls module`, the child TLS was initialised from `PT_TLS`, and the abort disappeared.
The generic `userland/linker/x86_64.ld` has the same header placement; it needs the same
treatment for any TLS-bearing C/std program.

Owner: **Mel** (uutils) or **Kevin** (toolchain/linker contract). A kernel alternative is to copy
the program headers into the startup stack and set `AT_PHDR` there; no kernel edit was made without
that handoff.

## After K136 (`d1ad5ca8`)

K136 merged the link-script change (`phdr PT_PHDR PHDRS; text PT_LOAD FILEHDR PHDRS ...`) and
Kevin's `base-tools-du` case. On the merged tree with `du` relinked, the thread-spawn abort is
gone. A direct kernel-serial marker trace (temporary, reverted) shows:

```text
K134D du uumain
K134 pc enter
K134 create before kernel
K134 child start
K134 child call
K134D du printer start
K134 create kernel ok
K134 pc ok
K134D du spawned
K134D du root stat
K134D du root stat done
K134D du safe_du enter
K134D du safe fn
K134D du initial stat done
K134D du open dir
K134D du dir opened
K134D du read_dir ok
K134D du entry
K134D du entry stat done
```

The pthread path is complete: `pthread_create` returns, the child reaches std's `print_stats` and
blocks on its mpsc receiver, and the parent proceeds into uutils' `safe_du`/`Stat::new`. The last
marker is the first entry's `stat_at`; `safe_du done`, `traversal done`, `before join` and `joined`
never appear. The main thread is inside uutils' directory traversal after the entry stat, before
the next traversal marker; the child waits on the channel. This is not the libc thread/TLS path
and not a kernel panic; it is the uutils `safe_du`/VFS path. Hand to **Mel** (uutils `safe_du`)
with this trace; hand to the filesystem/kernel owner if the post-stat VFS call is the stall.

## Regression

`tests/qtest/cases/base-tools-du.toml` (K136/Kevin's version) asserts `du` prints a size line for
`/tmp/du` and leaves `$?=0`. On the merged tree it now fails at step 7 with the traversal hang, not
the thread-spawn abort; it is the regression for the remaining handoff. The case is inside the
K134 Owns glob `tests/qtest/cases/base-tools*`.

## Provenance

First-party BSD-3-Clause analysis. No donor code was copied or translated. The behaviour was
checked against the documented ELF/auxv contract; NetBSD/FreeBSD and Linux both rely on a mapped
program-header table for `AT_PHDR`, which the Raven uutils layout currently does not provide.
