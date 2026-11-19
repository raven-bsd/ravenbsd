<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# KVM2 `smp-libc-concurrency` start hang: libc audit

Status: **static audit complete; QEMU reproduction is blocked on S137.** No kernel fix or handoff
is claimed. The closeout cell at `d79d7679` timed out after `smp-libc-concurrency: start`; S136
could not reproduce that stop from a clean image and then hit a separate qtest/QEMU lifecycle bug
that S137 owns. S138 does not start QEMU until S137 is green.

## Evidence

- KVM2 cell: `smp-libc-concurrency-kvm2`, timeout, `duration_ms=29857`.
- Serial content after the banner: only `smp-libc-concurrency: start`.
- Missing: `malloc ok`, `stdio ok lines=16000`, `done`, `svcd: settled`, `init exited (0)`.
- KVM1 passes in 14.5 s; TCG4 in 38.1 s; host load 1.2 with only the cell QEMU.
- The r7-full cell log named by the ticket is not present in this clone; the evidence above is the
  S136 quotation and the S136 report.

## What runs after `start`

`tests/libc/target/smp-libc-concurrency.c` prints `start`, then:

1. `fopen("/tmp/smp-libc-concurrency.log", "w")` (single-threaded, vfsd/ramfs request);
2. eight `pthread_create` calls (stack map, TLS/TCB init, list insert, kernel create);
3. eight `pthread_join` calls; each joins a worker that loops:
   `malloc/free` + `snprintf` + `flockfile` / `fputs` / `funlockfile`;
4. only after every join succeeds does it print `malloc ok`.

Workers print nothing on success, so a stop anywhere in the worker loop or in a join is
indistinguishable from an early stop in the serial log.

## Audit

### `fopen` (`raven-libc-stdio/src/open.rs`, `ofl.rs`, `state.rs`)

- `open` is called before any FILE or list lock; the descriptor is then wrapped in a leaked
  `File` and added to the open-file list.
- `OflGuard` is a CAS list lock plus `Env::wait`; no futex, no sleep, no wake. At first `fopen`
  the process is still single-threaded, so this lock is uncontended.
- No libc lock is held across the vfsd `open` request. A stop inside `fopen` therefore points at
  the `/tmp` vfsd/ramfs path, not at a libc lock cycle.
- **Conclusion:** no libc-only deadlock found in `fopen`.

### `fputs` and the stdio locks (`raven-libc-stdio/src/lock.rs`, `line_io.rs`, `backend_fd.rs`)

- The fixture brackets each `fputs` with explicit `flockfile`/`funlockfile`; `flockfile` always
  takes the real owner-tid lock, independent of the single-thread fast path.
- `lockfile`/`unlockfile` use owner tid + `MAYBE_WAITERS`; the owner check is exact because the
  fixture's tids are unique (main 100, workers 101-108).
- `ShellEnv::wait` is a pure user-space spin and `ShellEnv::wake` is a no-op (`ffi/env.rs`).
  A wakeup cannot be lost, but a waiter burns CPU until the owner releases. The owner is only
  delayed by the scheduler or by its own `write`/`open` request.
- `backend_fd::write` retries short writes and stops on zero/error; every loop iteration either
  consumes bytes of `first`/`second` or returns. No unbounded retry found.
- **Conclusion:** no lock/unlock mismatch or lost-wakeup path in `flockfile`/`fputs`. A stop here
  would mean the holder is blocked in the descriptor backend (vfsd/console) or never scheduled.

### `pthread_create` (`raven-libc-thread/src/thread/create.rs`, `stack.rs`, `raw.rs`)

- Stack, TLS/TCB and `StartArgs` are fully written before `kernel.create`; `join_word` is set to 1
  and `clear_tid_addr` names it. The list insert and `LIBC.add_thread()` happen before the kernel
  call, under the list lock, which is held until the call returns.
- **Finding (fixed):** `insert_links` left the creator's back link self-referential when the list
  was the single main node (`main.prev == main` instead of the new thread). Every later insert by
  main kept reusing that stale `previous_prev`. It is dormant after the S132 forward-walk unlink,
  but it violates the circular-list invariant and made `create` depend on `unlink` never trusting
  `prev`. Fixed by making the first insertion link the creator to the new node on both sides;
  `create_ut.rs` now drives the snapshots from the live links instead of hand-picked values.
- No libc lock is held while the new thread's user code runs, except the list lock across
  `kernel.create`; the child can only block on that lock in `pthread_exit`, and the creator
  releases it immediately after the syscall.
- **Conclusion:** no create-side deadlock found after the insert fix.

### `pthread_join` (`raven-libc-thread/src/thread/join.rs`, `sync/wait.rs`, `timedwait.rs`)

- The joiner waits on `exit_word`, re-checking it before each futex wait, so an exit before the
  join cannot be lost. It then spins on `kernel.thread_is_dead` (`join_word == 0`, cleared by the
  kernel inside `thread_exit`) before it unmaps the stack.
- **Open risk:** that death wait is a `1_000_000`-iteration `yield_now()` loop with no wall-clock
  bound. If the kernel never clears `join_word`, it consumes the whole 30 s cell cap before the
  `EINVAL` line can print. This is the one libc path that can turn a kernel clear-tid failure into
  a start-only log, and it is the first place to inspect in a preserved hang.
- **Conclusion:** no lost wakeup in the join wait; the death-confirmation spin is the libc-side
  amplifier to classify first.

### `pthread_exit` (`raven-libc-thread/src/thread/exit.rs`)

- Order is cleanup, TSD, kill lock, list lock, `tid = 0`, robust list, unlink, unlock,
  `exit_word = 1` + wake, then `kernel.exit`.
- After `exit_word = 1` the thread only stores `sigmask` and enters the kernel; the kernel clears
  `join_word` before switching away. `list::unlink` walks forward from the main TCB (S132), so the
  stale back link above cannot direct an unlink at the wrong node.
- **Conclusion:** no use-after-unmap or missed exit store found in the exit path.

## Classification so far

The libc audit found one real list-invariant defect (fixed with host regressions) and no
libc-internal lost wakeup. The start-only log can still be the `/tmp` vfsd request, a worker held
in the descriptor backend, or the join death-confirmation loop waiting on a kernel clear-tid that
never happens. Those are distinguishable only by a preserved hang: S137 green, then the bounded
`timeout 30 ... --no-build` iterations in the S138 ticket.

If the preserved hang shows the main thread in `pthread_join` with `join_word != 0`, or a worker
with no runnable state, the evidence goes to Stuart as a kernel scheduling/clear-tid handoff; no
kernel edit is made here without that trace.

## Provenance

First-party BSD-3-Clause work. No donor code was read or copied for the audit or the insert fix;
the existing musl-derived thread code keeps its recorded donor revision and licence headers.
