<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# aarch64 libc-gate parity (S56)

OBSERVED on `stuart/S56` (base `origin/master` `3e67c288`), qtest case `aarch64-libc`, QEMU
`virt`/aarch64 with one CPU. The fixture is `tests/fixtures/aarch64/`; it is a standalone
`no_std` package and does not use libc, the libc harness or a C toolchain.

## Why a separate fixture

S23 fixed the x86-64 `THREAD_SET_TLS` immediate install and S54 added the `libc-kernel`
reproducer. On aarch64 the register is `TPIDR_EL0`, and both the kernel and EL0 can write it:
the kernel installs `Thread::tls_base` on every switch-in (`kernel/src/sched/context.rs`), while
libc's `set_thread_pointer` writes the register directly (`raven-libc-sys/src/tp.rs`).

Before this ticket the two paths did not meet: `THREAD_SET_TLS` only stored the per-thread value,
so the caller kept reading the old `TPIDR_EL0` until a context switch. The first probe reproduced
that difference exactly:

```
aarch64-libc: fail main tls base=0x0 expected=0x1000
```

## Kernel change

`kernel/src/arch/aarch64/vectors.rs`:

* `save_user_tls()` runs at the top of `exception_dispatch` for the lower-EL slots (8..15) and
  copies the live `TPIDR_EL0` into the current thread's `tls_base`. This captures libc's direct
  `msr tpidr_el0` and keeps the kernel copy current before a signal context is resolved.
* `aarch64_restore_user_tls()` is called from `__exception_restore` for lower-EL frames, after
  `exception_dispatch` (and any context switch) and before `eret`, and installs
  `Thread::tls_base` in `TPIDR_EL0`.

Together these make `THREAD_SET_TLS` effective for the caller on the syscall return (S23 parity),
preserve direct EL0 writes across kernel round trips and preemption, and make `SIG_SETUP` resolve
its `TLS base + offset` context area against the live base. Current-EL frames are untouched, so
the kernel thread pointer in `TPIDR_EL1` and kernel threads' `TPIDR_EL0` handling are unchanged.

## Fixture probes

`tests/fixtures/aarch64/src/main.rs` maps a two-page TLS range for the main and child threads
(`MContext` is 4384 bytes, so the `0x100` signal area needs more than one page):

| Group | Probe | Expected |
|---|---|---|
| TLS | `THREAD_SET_TLS(SELF, page)`, then `mrs TPIDR_EL0` and a read at `0x31` | base equals the mapped page, marker `0x5a5a...` |
| thread create | `THREAD_CREATE_EX { tp, tid_out, clear_tid_addr }`; child reads its base and marker | child base equals `tp`, marker `0xa5a5...`, nonzero tid, clear word zero |
| futex | private `FUTEX_WAIT`/`FUTEX_WAKE`, clear-tid join wait, value mismatch, 10 ms timeout | waiter wakes, join blocks then clears, `EAGAIN`, `ETIMEDOUT` |
| signals | `SIG_SETUP(entry, 0x100, 0)`, self `THREAD_KICK(1)`, entry args, `mcontext_t.pc`, `SIGRETURN` | bits 1, fault 0, mask 0, interrupted PC in the fixture text, `SIGRETURN` resumes |

The aarch64 thread entry performs `bl body; mov x8, #THREAD_EXIT; svc #0`; the signal entry
performs `bl body` with the kernel's `x0`/`x1`/`x2` argument registers intact.

## Result (OBSERVED)

```
$ cargo xtask qtest --case aarch64-libc --arch aarch64
aarch64-libc: pass
```

Serial (`build/qtest/aarch64-libc.log`):

```
RavenBSD kernel 0.1 (aarch64) 3e67c288-dirty 2026-11-15
sched: running (1 cpu, slice 10 ms)
aarch64-libc: thread create ex ok
aarch64-libc: futex wait/wake ok
aarch64-libc: futex eagain ok
aarch64-libc: signal delivery ok
aarch64-libc: ok
init exited (0)
```

`build/qtest/aarch64-libc.json`: result `pass`, six matched expectations, no unmatched and no
forbidden hits. The pre-fix run failed at `fail main tls base=0x0 expected=0x1000`.

Existing aarch64 cases also pass with the change: `kernel-early-boot-aarch64`,
`kernel-exc-aarch64`, `kernel-paging-aarch64`, `kernel-timer-aarch64`,
`sched-kernel-threads-aarch64`, `boot-uname-aarch64`, `driver-aarch64` and `scheme-aarch64`.
Temporary aarch64 copies of the `tls-base`, `signal-entry` and `signal-fault` cases (their case
files are x86-64-only) also pass; the copies were removed and are not part of the branch.

## Follow-ups (not this ticket)

* libc `raven-libc-sys/src/tp.rs` writes `TPIDR_EL0` directly on aarch64. The kernel now captures
  and restores that value, so the existing libc sequence is safe. Unifying it on
  `THREAD_SET_TLS` would make the base visible to kernel-side consumers without waiting for an
  exception and would remove the x86-only sleep; owner: libc lane.
* `kernel/src/syscall/thread.rs` keeps the immediate `install_tls_base` call x86-only. aarch64
  parity is provided by the vector return path; a later refactor that moves the hook into
  `sched::context` must preserve both paths.
* The S54 libc follow-ups (`pthread_create` still `ENOSYS`, `raven-libc-signal` never calls
  `SIG_SETUP`) are unchanged and remain libc-lane work.
