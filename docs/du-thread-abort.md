<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# `du` thread-spawn abort: missing TLS module from `AT_PHDR=0`

Status: **BLOCKED — root cause found; the fix is outside the K134 Owns paths.** The abort is
reproduced and classified. A tested one-line uutils linker-script change removes it; a second
`du` traversal hang remains after that change and is handed off separately.

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

## Second finding after the link fix

With the tested link-script change, `du` no longer aborts but does not complete: the printer
thread starts (`K134 child start`) and blocks on its mpsc receiver; the main thread never reaches
`pthread_join` (`K134 join enter` is absent), so it is stuck in the `du` traversal before the
join. A temporary QMP run with four vCPUs showed every vCPU `HLT`/idle and no `du` output. That is
a separate uutils/VFS path, not the thread-handle/TLS abort; hand to **Mel** with this trace (and
to Stuart if the traversal wait is an IPC/futex kernel stall).

## Regression

`tests/qtest/cases/base-tools-du.toml` asserts `du` prints a size line for `/tmp/du` and leaves
`$?=0`. It currently fails with the exact abort above and should pass once the linker/AT_PHDR fix
lands. The case is inside the K134 Owns glob `tests/qtest/cases/base-tools*`.

## Provenance

First-party BSD-3-Clause analysis. No donor code was copied or translated. The behaviour was
checked against the documented ELF/auxv contract; NetBSD/FreeBSD and Linux both rely on a mapped
program-header table for `AT_PHDR`, which the Raven uutils layout currently does not provide.
