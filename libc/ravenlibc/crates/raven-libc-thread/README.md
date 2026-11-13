<!--
SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
-->

# raven-libc-thread

pthreads, C11 threads, TSD, cancellation and the internal locks. Layer L2b; generic over
`ThreadKernel` (production `SysKernel`, host fake `HostKernel`), no `alloc`, no `std` outside
tests, `unsafe` only in `raw`.

`cargo test -p raven-libc-thread` runs the unit tests and the integration tests:

* `host_basic` - create/join/detach, the 200-cycle ledger check (the fake keeps 512 scheduler slots; the 1,000-cycle variant is reported as a limitation), the scheduler trace.
* `a_thr_3` - the exit/reclaim race with `exit_ex` on and off and the deliberately wrong join.
* `caps_matrix` - every `KernelCaps` flag off and its documented fallback.
* `sched_cond` - the deterministic scheduler, 100 seeds and about 1,900 steps per seed.
* `fork_model` - the cooperative registry, child reset and atfork order.
* `donor_ports` - the NetBSD/OpenBSD scenarios re-expressed as Rust.
* `golden_asm` - the hidden asm symbols and the three-target golden disassembly.

## Lock order (section 4.7)

1 atfork list, 2 loader, 3 key rwlock, 4 `ptc`, 5 subsystem array, 6 per-FILE, 7 malloc, 8
`killlock`, 9 thread list. A lower rank may never be taken while a higher one is held.
`sync::RankGuard` asserts the order in the lock-order test; internal locks are skipped when the
process is single-threaded.

## Cancellation points

Only `syscall_cp` (C54) and `__testcancel` are central; this crate adds `__testcancel` at the
entry of `cond_timedwait`/`cond_wait`, `join`, `sem_wait`/`sem_timedwait`, `testcancel`,
`thrd_join`, `cnd_wait`/`cnd_timedwait`, `mtx_timedlock`. `mutex_lock`, `rwlock_*lock`,
`spin_lock`, `barrier_wait`, `sem_trywait` and `sem_post` are NOT cancellation points (C57 3.9).

## Capability fallbacks

Without PG-1 `init_main_thread` is `ENOSYS`; without PG-2 the 30-bit user tid counter and the
zombie stack list are used; without PG-3 the condition variable wakes one waiter instead of
requeueing and deadlines are converted in libc; without PG-4 shared objects are `ENOTSUP`/
`ENOSYS`; without PG-5 robust processing runs on `pthread_exit`; without PG-6 PI is `ENOTSUP`;
without PG-7 `pthread_kill` is `ENOSYS` and cancellation acts at the next point; without PG-8 a
futex wait is not interruptible; without PG-11 only `SCHED_OTHER` priority 0 exists; without
PG-45 `fork`/`_Fork` is `ENOSYS`.
