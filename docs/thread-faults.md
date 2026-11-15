# Thread and signal target faults (S23)

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

## Fault 1: libc-threads page fault at `0x31`

C73c's `libc-threads` binary printed `start` and then died at `0x31`. The address is a TLS
offset: the C test's first thread-local words live at `%fs:0x31`. The kernel's `THREAD_SET_TLS`
stored the base in the per-thread `tls_base`, but `IA32_FS_BASE` was only written by the next
context switch. libc worked around that with a one-millisecond sleep in
`raven_libc_sys::tp::set_thread_pointer`; any path that reads TLS before the switch faults at the
offset.

`tests/fixtures/kernel-thread` reproduces this without libc:

1. map a TLS page and write a marker at offset `0x31`;
2. `THREAD_SET_TLS(Handle::NONE, page)`;
3. immediately read `%fs:0x31`.

Before the fix the serial is exactly

```
process: /init killed by page fault at 0x31
```

After the fix `kernel/src/syscall/thread.rs` writes `IA32_FS_BASE` immediately when the target is
the calling thread (another target still receives the base on its next switch-in), and the test
prints `kernel-thread: ok`.

The same fixture creates a second user thread with `THREAD_CREATE_EX`, `tp` set to a second TLS
page, and reads `%fs:0x31` after the switch-in. That path already worked; it is now covered as
well.

`Tests/qtest/cases/kernel-thread-faults.toml` is the acceptance case. The negative control is the
temporary `if false` on the immediate-install branch, which brings back the exact `0x31` fault.

## Fault 2: libc-signals page fault at `0x0`

C73c's `libc-signals` printed through `setjmp ok` and then died in `check_ucontext()` at `0x0`.
This is a **libc wrapper bug**, not a kernel context-switch bug:

* `libc/ravenlibc/crates/raven-libc/src/ucontext/mod.rs` defines the C entry points
  `getcontext`, `setcontext` and `swapcontext` as direct tail-jumps:
  `jmp __rl_getcontext`, `jmp __rl_setcontext`, `jmp __rl_swapcontext`.
* The `__rl_*` functions in `raven-libc-signal/src/raw/context_x86_64.rs` take an
  `mcontext_t *` (the `ucontext_t` plus `UC_MCONTEXT_WORD * 8` = 40 bytes), not a `ucontext_t *`.
  The public C ABI passes the `ucontext_t *` unchanged, so `getcontext` writes at the wrong
  offsets and `swapcontext`/`setcontext` later jump through a value read from the wrong slot
  (typically the stack top or zero).
* A target probe that calls `raven_libc_signal::raw::getcontext_save` and
  `raw::swapcontext_save` with the adjusted `uc_mcontext_ptr` addresses performs the full
  1000-iteration `makecontext`/`swapcontext` loop on RavenBSD/x86-64 without a fault, so the
  libc context assembly itself is sound.

The fix belongs to libc: make the three wrappers call
`raven_libc_signal::context::{getcontext, setcontext, swapcontext}` (which apply the mask and
pointer adjustment), or add the `UC_MCONTEXT_WORD` offset before the tail-jump. S23 does not edit
`libc/**`.

## Thread creation follow-up

The S23 fixture uses raw `THREAD_CREATE_EX` with a TLS base and a user entry point; the kernel
path works. libc's `SysKernel::create` still returns `ENOSYS` (C57 deliberately refused while
`Handle::SELF_PROC` access and the libc half of PG-2 were missing), so C73c's `pthread_create`
cannot yet exercise it. That is a libc/toolchain follow-up, not the page fault fixed here.

## Coverage

* `kernel-thread-faults`: TLS install immediate read, plus `THREAD_CREATE_EX` thread TLS.
* `kernel-thread-faults` negative control: no immediate install -> page fault at `0x31`.
* Existing regressions: `tls-base`, `thread-tid`, `proc-identity`, `proc-fork`, `shell-session`.
* The libc ucontext probe was scratch-only; no failing libc case was added to the tree.

## Limits

* The TLS fix is x86-64-specific (`IA32_FS_BASE`); aarch64 and riscv64 already write their
  user-writable thread registers from libc.
* The signal wrapper fix is a libc change; it must be done in a libc ticket with the C58 host and
  target tests.
* `PROC_FORK` still starts the child's first thread from the default TLS/FP state; that is a
  separate process-fork follow-up.
