<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Process-death cleanup contract

S69 audits and tests what a RavenBSD process or thread owns at death, what the kernel releases,
and what a surviving process can still observe. The kernel is the sole owner of process
semantics (DR-46); there is no process server to run a second cleanup pass.

## Teardown sequence

`process::terminate_once` is the single terminal transition. It takes the process state lock,
publishes the strong zombie reference, marks the process `Exited`/`exiting`, reparents children,
walks the robust-futex lists, and calls `wake_process_waiters`. It then wakes `PROC_WAIT`, hangs
up the controlling terminal, notifies the parent/SIGCHLD, and calls `teardown`.

`teardown` is one-shot through `torn_down`; a terminate racing the target's own `EXIT` observes
the flag and returns. In order it:

1. runs `ipc::on_process_exit`: event registrations, scheme names, queued/active/client requests,
   reply attachments and deferred last-close notices;
2. takes and releases the user address space (own context switches CR3; a foreign teardown frees
   the target's frames and tables without switching);
3. replaces the handle table, dropping every remaining object (DMA, MMIO, IRQ, timer, memory
   object, pipe, thread, scheme file, request);
4. prints the S69 evidence line when `s69-cleanupcheck` is on the command line and the process
   command starts with `s69-` (the soak's generated victims do not print).

`wake_process_waiters` runs before the handle table is replaced. It removes and wakes the dying
process's entries from the futex buckets, its signal/child/scheme wait queues, target
`PROC_WAIT` queues reachable through process handles, IRQ wait queues, pipe read/write queues and
memory-object page queues. The wake status is `EIO`; a woken thread then observes
`process.exiting` in `block_current` and runs the normal thread-exit path instead of returning to
user mode. Waking without removing the queue entry would retain the waiter and its stack.

## Resource table

| Resource | Owner | Release path | S69 evidence |
| --- | --- | --- | --- |
| Handle table and lineage | process | table replaced; `LineageNode::Drop` | `handles=0`, `lineage=` baseline |
| In-flight client request | client | `pending request` canceled/failed, client list drained | `client=0`, `active=0`, `queued=0` |
| Server request / reply attachment | server | queued and active requests fail, captures released, request handles closed | `active=0`, `captures=` |
| Deferred last-close notice | client/server | delivered to a live server; purged when the server exits | `schemewait=0`, scheme name reusable |
| Scheme registration | server | registry entry removed, waiters woken | replacement server registers immediately |
| IRQ line/vector | process | blocked waiter woken; last live process retires the GSI/vector | `irq=0`; `IRQ_CREATE` succeeds again |
| DMA buffer | process | `DmaBuffer::Drop` frees frames and both quotas | `dma=0`, `dma_live=0` |
| MMIO window | process | `MmioRegion::Drop` | `mmio=0` |
| Shared mapping / memory object | process | `UserAddressSpace::drop` and `MemObject::Drop` | `pager=0`; frames reusable |
| Futex waiter | process | `WaitQueue::wake_process` with `EIO` | 100 blocked victims; stable stacks/heap |
| Event registration and queue | process | `event::on_process_exit` clears state, wakes, retires and unregisters the queue | `events=0` |
| Timer | process | `Timer::Drop` decrements quota | `timers=0`, `timers_live=0` |
| Signal/child/process waiter | process | `WaitQueue::wake_process` with `EIO` | `sigwait=0`, `childwait=0`, `wait=0` |
| Thread, kernel stack, FPU area | thread | thread exits; stack is deferred to `reap_dead`; FPU area drops with `Thread` | `stack_fresh` stable, `stack_cached` non-zero |
| Pipe endpoint and ring | process | waiter woken; last live process retires the 64 KiB ring | stable heap over 100 owner cycles |

The IRQ and pipe retirement checks scan the pid registry for another live process holding the
same object, so a shared IRQ/pipe is not retired while a survivor still owns it.

## Evidence line

With `s69-cleanupcheck` the kernel prints one line per fixture process teardown:

```
s69: exit <command> processes=<n> zombies=<n> handles=<n> lineage=<n>
    timers=<n> timers_live=<n> dma=<n> dma_live=<n> irq=<n> mmio=<n>
    events=<n> captures=<n> pager=<n> wait=<n> childwait=<n> sigwait=<n>
    schemewait=<n> client=<n> active=<n> queued=<n>
    stack_fresh=<n> stack_cached=<n> heap=<n>
```

Counters are process-local where a process owns the object (`handles`, `timers`, `dma`, the
waiter/request queues) and global for shared kernel objects (`lineage`, `captures`, `pager`,
`irq`, `mmio`, `events`, `timers_live`, `dma_live`). A drain process runs after the soak and
after the server-death scenario, so the final line must show zero live resources and zero
waiters.

## Fixture

`tests/fixtures/proc-death-cleanup` is one arch-neutral binary with roles selected by argv:

* `s69-server` registers scheme `s69` and implements `hold`, `sync`, `state` and `attach`. A
  `hold` `WRITE` keeps its request and capture until `sync`; `state` reports the held count;
  `attach` returns a memory-object handle as a reply attachment.
* `s69-owner` is created stopped, receives a timer, DMA buffer, MMIO window, IRQ and shared
  memory object, maps a control page and a 4 MiB shared object, then starts helper threads for a
  futex wait, pipe read, `SIGRECV`, a 2 s sleep, an IRQ wait (x86-64), a held scheme `WRITE` and
  a reply attachment. Its main thread blocks in `EVQ_WAIT`.
* The controller kills the owner and checks that every resource can be created again. One
  hundred owner cycles run.
* Before the resource cycles, ten thousand process teardown cycles run: the first hundred spawn a
  futex-blocked victim; the rest use created-but-never-started process objects.
* Finally the controller blocks a client in `WRITE`, kills the server, observes the client's
  `EIO` without client corruption, and re-registers `s69`.

## Running

```
cargo xtask qtest --case proc-death-cleanup --arch x86_64
cargo xtask qtest --case proc-death-cleanup --arch aarch64
make check-fast
```

## Known limits

These are outside the S69 Owns paths or are bounded deferrals. They are not observed by the
fixture's counters, and none of them retains a hardware claim, a handle, a waiter or a capture:

* `sched::schedule_locked` keeps `from`/`to` `Arc<Thread>` locals on the outgoing kernel stack.
  A thread that never resumes after a context switch (killed while blocked or after a yield) does
  not unwind that frame, so the `Arc` and the `Thread`/`Process` heap objects are not dropped.
  Kernel stacks themselves are still reaped. Fixing this needs a `kernel/src/sched/mod.rs`
  change, outside S69's Owns.
* A blocked syscall frame can retain an `Arc<EventQueue>`, `Arc<PipeEnd>` or `Arc<PendingRequest>`
  after thread exit. S69 explicitly retires the externally visible resources (event queue slot,
  pipe ring, IRQ claim, captures, registration) but the small Rust object remains an inert
  allocation.
* A process's main thread has no handle of its own. If it is killed while in `nanosleep` rather
  than a wait queue, its stack is released only when the timeout fires; the fixture exercises
  the wakeable waiters instead.
* A pager-backed PageIn owned by a killed process is completed or failed by the pager's reply;
  S69 does not abort that in-flight page state at process death. Eager and shared mappings are
  covered.
