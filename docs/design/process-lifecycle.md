# Process and thread lifecycle

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: OBSERVED for the code as of S36; PROPOSED where a follow-up is named.

This is the kernel's process/thread lifecycle contract: states, transitions, the locks that
make them atomic, and the reaping that returns memory.  It implements contract C16
(`PROC_TERMINATE`) and completes the lifecycle that K6/K7/A1 flagged.  The scheduler and
concurrency prerequisites are those of `docs/design/concurrency.md` (user threads on CPU 0,
non-preemptible kernel).

## 1. States

```
Process:  Created --(THREAD_CREATE admitted)--> Running --(terminate_once)--> Exited
              |                                                            ^
              `-------------------(reap_unstarted)--------------------------'
Thread:   Ready -> Running -> Blocked -> Ready ...
                        `-> Dead (exit_thread or the terminal-process filter)
```

* `Created`: `PROC_CREATE` made the process; no thread yet.
* `Running`: at least one thread was admitted.
* `Exited` (terminal): the process code is published, waiters are woken and the teardown has
  run.  The transition is one-way; a terminal process can never be resurrected.

## 2. Transitions and locks

### EXIT (process-wide)

`EXIT` from any thread terminates the whole process:

1. `terminate_once(process, code, own_context = true)`: under the process state lock, set
   `Exited`, store the code, set the `exiting` flag, wake every `PROC_WAIT` waiter.
2. Teardown (one-shot, `torn_down` flag): fail pending scheme requests and release scheme names
   while the page tables still exist, release the address space (`UserAddressSpace::release`
   switches CR3 to the kernel root, which is correct here because the caller is a thread of
   the dying process and never returns to user mode), replace the handle table, run the
   PID-1/test-exit path and the per-exit evidence prints.
3. `exit_thread`: the caller becomes `Dead` and switches away.

No other thread of the process can observe an inconsistent state:

* a runnable thread of a terminal process is dropped by the filter in `schedule_locked`
  (marked `Dead`, queued for reaping);
* a thread blocked in a syscall re-checks `exiting` when `block_current` returns and exits
  before returning to user mode;
* under the CPU-0 model no two user threads run concurrently, so the teardown never races a
  running thread of the same process.

### THREAD_EXIT

`THREAD_EXIT` terminates only the calling thread.  The process stays alive while other threads
live; when the last thread exits, `notify_exit` runs the same `terminate_once` + teardown as
`EXIT` (own context).

### PROC_TERMINATE (contract C16)

`proc_terminate(proc, code)` requires `Rights::SIGNAL` on the process handle:
`EPERM` without it, `EBADF` for a stale handle, `ESRCH` when the target is already terminal.
Terminating self behaves like `EXIT(code)`.  The teardown runs in the *caller's* context
(foreign context): the address space is freed **cold** — `free_user_frames` plus the page-table
drop, without the CR3 switch of `release()` — because the caller must keep its own CR3 to
return through `sysret`, and the target's root is inactive (CPU-0 model: the caller is the only
running user thread).  The evidence prints are skipped on the foreign path; a terminated
process with threads prints them from its last thread's exit.

### Admission (THREAD_CREATE)

The process state lock is held from the terminal check until the thread is enqueued
(state lock → address-space lock, matching the exit path's order).  A terminal process gets
`ESRCH`; a failed handle insert or enqueue rolls the handle back and queues the stack for
reaping.  `Created` becomes `Running` under the same lock.  No admission can interleave with
`terminate_once` (same lock), so the racing create either joins a live process (its thread is
then killed by the exit filter) or fails — never a resurrected `Exited`.

### PROC_WAIT

`PROC_WAIT` returns the stored code of a terminal process, any number of times.  A `Created`
process is reaped immediately (`reap_unstarted`): it has no thread and can never run, so the
terminal transition and the cold teardown happen in the waiter's context.  The no-lost-wakeup
protocol is the state lock held across the `Exited` check and the wait-queue enqueue (the exit
path takes the same lock before waking).

## 3. Reaping

### Kernel stacks (`kernel/src/sched/thread.rs`)

`allocate_kernel_stack` first reuses a cached stack; a fresh carve allocates a 5-page heap
block and unmaps the lowest page as a guard.  `free_kernel_stack(base, top)` returns a stack
to the bounded cache (64).  **Stacks are never returned to the heap**: the guard page's
physical frame is discarded by the unmap at creation, so a `dealloc`ed block could not be
re-carved (its guard unmap would find the page missing).  The cache bounds the steady-state
cost instead; `stack_fresh()` counts fresh carves and is the reuse evidence.

### Deferred thread reaping (`kernel/src/sched/mod.rs`)

A thread cannot free the stack it runs on, and the frame that switches away is abandoned with
its stack.  `exit_thread` (and the terminal filter, and creation rollback) therefore hand the
owning `Arc<Thread>` to the `REAP` queue; the next `schedule_locked` on a live stack turns it
back into an `Arc`, calls `free_kernel_stack`, and drops it.  `reap_dead` runs after the
context switch, on the incoming thread's stack.

The trampoline (`thread_entry`) drops its `current_thread()` clone **before** diverging into
`enter_user`: its frame never returns, and a live local there would leave the thread owning a
reference to itself on its cached stack (a measured ~200-byte leak per exited thread before
this fix).  The dead `exit_thread` frame still orphans one `Arc` of the thread it switched to;
that reference is lost when the stack is reused, so each such thread's `Thread` struct can
outlive it by one allocation — bounded (the woken/resumed threads are long-lived), and the
10,000-cycle evidence shows the heap at baseline.

### Address space and handle table

The teardown releases the address space once (`torn_down`) and replaces the handle table, so
DMA buffers and every other object drop even while a last `Arc<Process>` survives on a dead
thread's cached stack.

## 4. Evidence

`s36-stackcheck` prints, at every own-context exit, the fresh/cached stack counters and the
heap used; `s21-framecheck` prints the free-frame count.  The `process-lifecycle` qtest drives
the six scenarios; the 10,000 spawn/exit cycle returns `fresh` and the frame count to their
marker baseline (heap within ~1.5 KiB of allocator bookkeeping).

## 5. Known gaps (follow-ups)

* Wait-queue entries of a terminated process's blocked threads (futex buckets, `RECV`,
  `IRQ_WAIT`, `EVQ_WAIT`) are not all cancelled by the teardown; the threads are killed but a
  stale `Waiter` can remain until its timeout.  S31 handles the `IRQ_WAIT`/handle-close case;
  the general cancel belongs with K68's service-stop hardening.
* The abandoned `exit_thread` frame's orphaned `Arc` (above) — a bounded count-only leak until
  the switch frame can be made reentrant.
* The stack cache never returns memory to the heap (the guard-frame discard above); a stack
  cache that remaps the guard (one shared frame) could close this, at the cost of the
  round-2 crash risk — deliberately not attempted here.

## 6. Trigger points for a new audit

User-thread SMP or kernel preemption invalidates the CPU-0 argument that makes the cold
foreign teardown safe (no concurrent thread of the target can be running); revisit the
teardown context rules then.
