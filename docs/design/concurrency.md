# Concurrency, faults and event timing

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Status: OBSERVED for the code as of S34; PROPOSED where a future model is named.

This document is the kernel's working concurrency model: what the code assumes,
which objects rely on those assumptions, and what must be re-audited before the
assumptions change.  It is deliberately narrower than a full memory-model proof.
It is not an architecture document; the lead owns `architecture/`.

## 1. Execution model

### 1.1 CPU roles

* **CPU 0 (BSP)** runs every user thread, every process-context syscall, process
  creation and teardown, and the driver `Irq`/vector code.
* **CPUs 1..N (APs)** run kernel threads only today: idle loops and the
  `selftest=smp`/`selftest=sched` workers.  No AP enters ring 3.
* Device interrupts are routed to the BSP.  MSI messages are encoded with the
  BSP APIC id (`irq_create(2)`), and legacy IOAPIC entries use the BSP
  destination.
* The timer LAPIC interrupt runs on the CPU whose timer was programmed; the BSP
  keeps the S07 timer and APs may run their own timer for the scheduler self-test.

Consequences that the rest of this document relies on:

1. A process-context close of an `Irq`, a vector slot or a GSI registry entry
   cannot run concurrently with the interrupt handler that may dereference it.
2. Two user threads never run at the same time on different CPUs, so process
   page tables are only mutated by one CPU at a time.
3. A user page-table shootdown is only needed for the APs' kernel mappings and
   for TLB entries left by an earlier user thread on the BSP; APs never execute
   user code.

### 1.2 Non-preemptible kernel

`sync::critical()` and every `IrqSpinLock` guard increment the per-CPU
`preempt_count`; the scheduler refuses to switch while it is non-zero
(`sync::preemption_enabled`).  `sched::timer_return` calls `preempt_if_needed`
only when the interrupted context was user mode **and** the counter is zero.
`syscall_return` and `ipi_return` use the same gate.  Interrupt handlers always
return to the interrupted kernel instruction; a handler never switches away
from kernel mode.

A blocking syscall explicitly leaves kernel mode through
`sched::block_current`; that is the only place a thread yields the CPU while
inside the kernel.  A guard live across that switch is restored when the thread
resumes (the context switch saves and restores `preempt_count`, and the guard's
saved IF is the thread's own).

### 1.3 Syscall entry

`syscall_entry` loads `IA32_FMASK = 0x0004_0700`: IF, TF, DF and the trap flag
are cleared by `syscall`.  Syscalls therefore start with interrupts masked.
Code that may block or wait re-enables them through the scheduler; the shootdown
wait loop does so explicitly (section 5).  A syscall never holds a user lock
across a blocking point unless that is stated in the object's section below.

### 1.4 Blocking points

Every place that can sleep, and what it holds when it does:

| Blocking point | Holds | Notes |
|---|---|---|
| `sched::block_current` (wait queues, `IRQ_WAIT`, `EVQ_WAIT`, `PROC_WAIT`, futex) | no user-visible lock; a wait-queue entry is removed by the waiter after wake | the scheduler critical section is released before the switch |
| `sched::sleep` / `nanosleep` | nothing | deadline stored in the per-CPU sleeper list |
| IPC `CALL` / `SEND` / `RECV` | no address-space lock; the request/reply captures are dropped before waiting | server queues are per process |
| `IRQ_WAIT` | an `Arc<Irq>` clone and a wait-queue entry | cancelled by `HANDLE_CLOSE` (section 6) |
| `PROC_WAIT` | a process handle clone | process exit wakes the parent |
| shootdown ack wait | the shootdown `IrqSpinLock`; IF temporarily enabled | section 5 |

No lock is held while copying to/from user memory in a way that could fault into
the COW resolver; section 7 states the address-space-lock rule.

## 2. Interrupt-safe locks

A lock reachable from an interrupt handler must either disable interrupts while
held or prove that the handler never takes it.  The kernel uses three classes:

* **`sync::IrqSpinLock<T>`** — disables interrupts, increments `preempt_count`,
  and restores the saved IF on drop.  This is the default for anything an ISR
  can touch.  It is a real spinlock (the flag serialises CPUs), so it is safe
  across CPUs as long as no holder waits for the ISR on another CPU.
* **`sync::critical()`** — the same interrupt/preemption discipline without a
  spin flag.  Used for per-CPU scheduler state and for short sequences that
  need to exclude the local tick.
* **Plain `spin::Mutex`** — only where no interrupt handler can reach the lock.
  Today that is `EventQueue.state`: the timer tick's `notify_all` walks only the
  queue table and the wait queues, never `state`.  If an ISR ever needs `state`,
  it must become an `IrqSpinLock` first.

### 2.1 Objects an ISR can touch

| Object | Locking | Allocation in ISR? |
|---|---|---|
| IOAPIC redirection table | `IOAPICS: IrqSpinLock`; ISR masking takes the same lock | no |
| vector pool / owner slots | `AtomicU64` bitmap + `AtomicPtr<Irq>`; handler only loads | no |
| legacy GSI registry | `AtomicU64` bitmap | no |
| event queue table (`QUEUES`) | `IrqSpinLock<Vec<Weak<EventQueue>>>`, bounded by `MAX_QUEUES` | no (weak upgrades only) |
| event wait queue | `IrqSpinLock<Vec<Waiter>>`; wake removes under the lock, then calls the scheduler | no |
| serial console | plain `SpinMutex<Serial>`; **never taken from an ISR** | — |
| scheduler run queue | `critical()` + per-CPU `SpinMutex` | no |

`time::on_tick` may call `event::notify_all` from the timer ISR.  `notify_all`
copies at most `MAX_QUEUES` strong references under the `IrqSpinLock`, releases
the lock, and only then wakes waiters, so it does not hold an unrelated table
lock across the scheduler.  Exhausting `MAX_QUEUES` makes `EVQ_CREATE` return
`ENOSPC`; the scan is never unbounded.

`irq::handle_interrupt` performs no allocation, takes no sleepable lock and no
process-context lock.  It records unclaimed/unexpected vectors in atomic bits;
the reporting `kprintln!` happens in the next IRQ syscall, in process context.
This avoids taking the serial spinlock from an ISR while a kernel thread holds
it with IF set (a self-deadlock).

## 3. Scheduler monitoring (C3)

`sched::switch_count` walks the per-CPU slots to sum `switches`.  The timer tick
takes `critical()` and then a per-CPU slot lock; the monitor must use the same
discipline or a tick delivered while it holds a slot lock would re-enter `tick`
and deadlock on that slot.  `switch_count` therefore enters `sync::critical()`
for the whole walk.  The guest test is the existing `selftest=sched` monitor
(`sched-kernel-threads`) and the `selftest=smp` counter (`smp-boot`), both of
which call `switch_count` with timers active; `concurrency-stress` also runs the
monitor path indirectly through the scheduler while timers tick.

## 4. Lock order

The order is object-specific rather than a single global graph.  The rules:

1. A scheduler critical section (`critical()` / `IrqSpinLock`) is entered before
   any per-CPU slot lock; never the reverse.
2. `process.handles` may be taken while holding `EventQueue.state`
   (`readiness`, `EVQ_CTL` source validation), but no path takes a
   `process.handles` guard and then blocks on `EventQueue.state` while still
   holding it.  Look up and clone, drop the table, then take the object lock.
3. The event queue table (`QUEUES`) is above `EventQueue.state` and the wait
   queue: `on_handle_close` takes `QUEUES`, drops it, then takes `state`.
   `notify_all` takes `QUEUES`, drops it, then wakes.
4. Wait-queue locks never call into the scheduler while held.  `wake` /
   `wake_all` / `wake_one` remove entries under the wait-queue lock and call
   `sched::wake` after it is released.
5. The IOAPIC lock is a leaf: no code takes another lock while holding it.
6. The shootdown and call mailboxes are leaves with respect to the address-space
   lock; the IPI handlers take neither the address-space lock nor a wait queue.

Exceptions are documented at the call site rather than silently reordered.  A
new lock that can be reached from an ISR must be added to the table in section
2.1 and to this order.

## 5. Shootdown and IPI

`flush_range` (TLB shootdown):

* **Entry locks:** the caller may hold an address-space lock (usually does), but
  must not hold a scheduler slot lock, a wait-queue lock or the shootdown lock.
* **Shooter:** takes `LOCK` (IrqSpinLock), publishes the range and sequence with
  `SeqCst`, sends one IPI per remote CPU.  It then enables IF while waiting for
  the acknowledgements so a target spinning on an address-space lock can still
  take the IPI.  On return the guard restores the caller's original IF.
* **Handler:** executes `invlpg` and stores the acknowledgement; it takes no
  lock and depends only on this module's atomics.  It must stay short.
* **Timeout:** 100 ms per shootdown.  A missing acknowledgement clears that
  CPU's `PENDING` slot, logs `smp: tlb shootdown timeout on cpu N`, and returns
  `false`.  No frame or slot is allocated, so a timeout cannot leak.  The caller
  decides whether the stale TLB is fatal.
* **Injection:** the `s34-tlb-fail` boot flag arms a one-shot skip of one remote
  IPI.  `concurrency-stress` uses it to exercise the timeout path; it is never
  armed without the flag.

`call::probe` uses the same 100 ms timeout: it holds its `IrqSpinLock` (IF
masked) while waiting, so the target must never depend on the shooter taking an
interrupt.  On timeout it clears the pending slot, logs
`smp: call probe timeout on cpu N`, and returns `None`; the next probe uses a
fresh sequence.

## 6. IRQ lifetime (S31, for reference)

* A vector slot holds a typed `AtomicPtr<Irq>`; the same-CPU/non-preemptible
  argument in `vector.rs` is what keeps the pointer valid.  `Irq::drop` clears
  the slot before the object is freed.
* A live MSI source closed without the driver's final `IRQ_ACK` is quarantined:
  the bitmap bit is never released, late messages are counted as unclaimed and
  EOId.  A quiesced or never-armed source releases its vector.
* A handle close wakes every `IRQ_WAIT` waiter; the waiter revalidates its
  handle and returns `EIO`.  The waiter's `Arc` keeps the object alive until it
  returns, and the wait-queue entry is removed on every path.

## 7. Usercopy and COW (C7)

`copy_to_user`/`copy_from_user` are bracketed by the assembly fixup table.  A
page fault inside a bracket:

* **write to user** (`copy_to_user`): the VMA must be logically writable.  The
  fault path calls `process::handle_page_fault`, which resolves a private COW
  page (or turns a private mapping writable) and lets the faulting store retry.
  A genuine read-only VMA or missing page falls through to the fixup label and
  the syscall returns `EFAULT`.
* **read from user** (`copy_from_user`): no COW resolution; a fault is a real
  missing/permission fault and is fixed up to `EFAULT`.
* **kernel-origin fault:** the handler is only consulted for a user address
  inside a usercopy bracket.  A kernel pointer fault or a fault outside the
  brackets never resolves COW.

Because the write path takes the process address-space lock, a caller must not
hold that lock while copying to user memory.  The current callers look up and
drop the lock before the copy; a future caller that holds it would deadlock in
the fault path.

`usercopy-cow` checks `uname`, `clock_get`, an event-queue record and a self-served
`FSTAT` reply into fresh private COW pages, then the same calls into a read-only
VMA (both `EFAULT`).  The IPC case is a same-process scheme call: the router's
self-call capture keeps the client address, so its final `copy_to_user` is the
write that resolves COW.

## 8. Event timing (C9)

`Timer::peek` is the readiness predicate: side-effect free, usable any number of
times by `EVQ_WAIT`, `has_ready` and the enqueue check.  `Timer::consume`
advances the deadline exactly once per delivered record, with a compare-exchange
so another queue observing the same period does not skip a period.  `EVQ_WAIT`
consumes every source only after its record was copied to user memory; a failed
copy leaves the period pending.

`concurrency-stress` arms a one-shot timer under a non-matching mask, probes it
with a short `EVQ_WAIT`, then changes the mask to `SIGNALED`: the event must
still arrive.  With the old consume-on-probe predicate the probe lost the
period and the test timed out.

## 9. Trigger points for a new audit

Re-open this document and audit every section above if any of these changes:

1. User threads are scheduled on application processors.
2. Kernel preemption is enabled (`preempt_count` no longer blocks a switch).
3. Device interrupts are routed to a CPU other than the BSP, or MSI destination
   selection becomes user-controlled.
4. The TSS/per-CPU `rsp0` handling changes (section 10).
5. A new ISR takes a lock (add it to section 2.1), allocates, or logs.
6. A new usercopy path holds the address-space lock.
7. A new event source is added to `readiness` or `consume_source`.

## 10. SAFETY comments corrected by S34

* `sync::IrqSpinLock`: the old “single CPU” wording is gone.  The lock is
  non-reentrant because the flag serialises and interrupts are masked on the
  holding CPU; a recursive acquisition deadlocks by contract.
* `gdt`: there is one shared TSS today.  `rsp0` is architectural per-CPU state;
  before user threads run on APs, every CPU needs its own TSS descriptor, `tr`
  load, `rsp0` slot and IST stacks.  APs currently run kernel threads only.
* `usermode::PerCpu`: this is the BSP's legacy S08 GS block.  The scheduler
  installs the real per-CPU block from `ap_start` on every CPU; APs never use
  this structure because they never enter user mode.
