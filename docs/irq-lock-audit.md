<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# IRQ-mask lock audit (S102)

Inventory and classification of every `.lock()`/`.try_lock()` site in
`kernel/src/**` that can run in an SMP or interrupt context. The machine-readable
inventory is `tests/smp/irq-lock-inventory.tsv`; the regression checker is
`tests/smp/irq_lock_audit.py --check`.

## Summary

| Class | Sites | Meaning |
|---|---:|---|
| IRQ-SAFE | 34 | `IrqSpinLock` sites; short critical section, no waiter that needs an IPI/timer |
| NEEDS-PROOF | 55 | process-context `spin::Mutex` in a fault/timer/scheduler/driver path; invariant listed below |
| SAFE | 382 | process/syscall/death paths with no vector-handler reachability |
| TEST-ONLY | 3 | `#[cfg(test)]`/selftest locks |
| **total** | **474** | all `.lock()`/`.try_lock()` calls in `kernel/src` |

There is no separate `SpinLock` type in the tree; `spin::Mutex` is the
non-interrupt-masking lock family, and `IrqSpinLock` is the interrupt-masking
family. `IrqSpinLock::lock` disables interrupts and spins (no IF while waiting);
the TLB shootdown path deliberately does **not** use it: `ActiveGuard` waits with
interrupts enabled (see below).

## IrqSpinLock sites

| Declaration | Lock | Classification | Invariant / evidence |
|---|---|---|---|
| `kernel/src/event/mod.rs:155` | `QUEUES` | IRQ-SAFE | `notify_all` (timer IRQ) copies weak refs under the lock and wakes outside it; registration/close are process context. No allocation/wait under the lock. |
| `kernel/src/sync.rs:299` | `TURNSTILES` | IRQ-SAFE | graph map update only; `Mutex::lock` releases it before `begin_block`/turnstile wait, so no scheduler dependency while held. |
| `kernel/src/sync.rs:795` | `CLIENT_THREAD` | TEST-ONLY | selftest-only lock. |
| `kernel/src/time.rs:167` | `CYCLICS` | IRQ-SAFE | timer callback only records an atomic fire id; no allocation/schedule/IPI. |
| `kernel/src/time.rs:438` | `ENTROPY_POOL` | IRQ-SAFE | pool fill/reseed only; no IPI/timer dependency. |
| `kernel/src/arch/x86_64/ioapic.rs:38` | `IOAPICS` | IRQ-SAFE | IOAPIC MMIO state; process context, no IPI wait. |
| `kernel/src/sched/selftest.rs:38` | `COUNTER_LOCK` | TEST-ONLY | selftest-only lock. |
| `kernel/src/sched/wait.rs:38` | `waiters` | IRQ-SAFE | wait-queue `Vec` mutation; the guard is dropped before `sched::wake`, preserving the wait-queue-to-scheduler order. |

All IRQ-SAFE call sites (file:line) are in the inventory. The 34 production
sites are the `IOAPICS` route/mask/unmask paths, `QUEUES` register/notify/
snapshot, `waiters` enqueue/wake/remove/len, `CYCLICS` add/cancel/fire,
`ENTROPY_POOL` fill/reseed, and the `TURNSTILES` graph operations.

## TLB shootdown pattern

`kernel/src/smp/shootdown.rs` is intentionally **not** an `IrqSpinLock`:
`ActiveGuard` masks interrupts only while publishing/clearing `ACTIVE`, then
re-enables IF while spinning and waiting for `ACKED`/`LAST`. This is the
correct pattern for any lock whose holder may need an IPI/timer to release it;
the S113/S85e TLB stress is green (3/3, `tlb_stale=0` on the S159 run). No
`IrqSpinLock` in the audit holds a lock across such a wait.

## Needs-proof sites

These are process-context `spin::Mutex` sites in page-fault, timer/scheduler,
probe, or driver/VT-d paths. They are **not proven unsafe**; each needs the
invariant or test below before it can be marked safe. No site was patched in
`crates/raven-vmem/**` because that crate has no lock use (`rg` returns none).

| Site | Lock / context | Proposed proof or fix | Owner |
|---|---|---|---|
| `kernel/src/arch/x86_64/vtd.rs:110` | `self.iova` in `iova_start` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:115` | `self.iova` in `iova_end` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:133` | `?` in `map` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:138` | `self.iova` in `map` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:148` | `self.iova` in `unmap` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:159` | `self.tables` in `map_pages` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:213` | `self.tables` in `unmap_pages` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:274` | `UNIT` in `available` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:279` | `UNIT` in `unit` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:309` | `UNIT` in `init` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:465` | `unit.by_sid` in `domain_for_bdf` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:466` | `unit.domains` in `domain_for_bdf` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:486` | `unit.domains` in `domain_for_bdf` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:487` | `unit.by_sid` in `domain_for_bdf` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:611` | `unit.domains` in `allocate_domain_id` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:622` | `self.context_tables` in `install_context` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:633` | `self.table_frames` in `install_context` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test | S83/Stuart (driver) |
| `kernel/src/driver/mod.rs:202` | `BLOBS` in `init` | driver init/DMA paths are process context; add a concurrent IRQ/DMA test | S83/Stuart (driver) |
| `kernel/src/driver/mod.rs:217` | `PLATFORM_OWNED` in `init_aarch64` | driver init/DMA paths are process context; add a concurrent IRQ/DMA test | S83/Stuart (driver) |
| `kernel/src/driver/mod.rs:236` | `DMA_DOMAIN` in `init_aarch64` | driver init/DMA paths are process context; add a concurrent IRQ/DMA test | S83/Stuart (driver) |
| `kernel/src/driver/mod.rs:243` | `BLOBS` in `init_aarch64` | driver init/DMA paths are process context; add a concurrent IRQ/DMA test | S83/Stuart (driver) |
| `kernel/src/driver/mod.rs:315` | `DEVICE_HINTS` in `create_mmio` | driver init/DMA paths are process context; add a concurrent IRQ/DMA test | S83/Stuart (driver) |
| `kernel/src/driver/mod.rs:419` | `PLATFORM_OWNED` in `refuse_kernel_owned` | driver init/DMA paths are process context; add a concurrent IRQ/DMA test | S83/Stuart (driver) |
| `kernel/src/driver/mod.rs:465` | `DEVICE_HINTS` in `dma_alloc` | driver init/DMA paths are process context; add a concurrent IRQ/DMA test | S83/Stuart (driver) |
| `kernel/src/driver/mod.rs:491` | `DMA_DOMAIN` in `dma_alloc` | driver init/DMA paths are process context; add a concurrent IRQ/DMA test | S83/Stuart (driver) |
| `kernel/src/driver/mod.rs:494` | `DMA_DOMAIN` in `dma_alloc` | driver init/DMA paths are process context; add a concurrent IRQ/DMA test | S83/Stuart (driver) |
| `kernel/src/dtrace/mod.rs:87` | `WAKE_EVENT` in `wake_consumer` | probe wake path runs in process context, or mask IRQs with a dedicated IrqSpinLock; probe/fault test | S83/Stuart (dtrace) |
| `kernel/src/dtrace/mod.rs:103` | `WAKE_EVENT` in `enable` | probe wake path runs in process context, or mask IRQs with a dedicated IrqSpinLock; probe/fault test | S83/Stuart (dtrace) |
| `kernel/src/dtrace/mod.rs:118` | `WAKE_EVENT` in `disable` | probe wake path runs in process context, or mask IRQs with a dedicated IrqSpinLock; probe/fault test | S83/Stuart (dtrace) |
| `kernel/src/dtrace/mod.rs:128` | `CONSUMER` in `drain` | probe wake path runs in process context, or mask IRQs with a dedicated IrqSpinLock; probe/fault test | S83/Stuart (dtrace) |
| `kernel/src/dtrace/mod.rs:165` | `CONSUMER` in `clear` | probe wake path runs in process context, or mask IRQs with a dedicated IrqSpinLock; probe/fault test | S83/Stuart (dtrace) |
| `kernel/src/dtrace/mod.rs:170` | `WAKE_EVENT` in `clear` | probe wake path runs in process context, or mask IRQs with a dedicated IrqSpinLock; probe/fault test | S83/Stuart (dtrace) |
| `kernel/src/memory/frame.rs:165` | `ALLOCATOR` in `init` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/frame.rs:253` | `ALLOCATOR` in `init` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/frame.rs:265` | `ALLOCATOR` in `set_percpu_enabled` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/frame.rs:277` | `ALLOCATOR` in `alloc_frame` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/frame.rs:306` | `ALLOCATOR` in `alloc_contiguous` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/frame.rs:320` | `ALLOCATOR` in `free_frame` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/frame.rs:352` | `SHARED_FRAMES` in `share_frame` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/frame.rs:362` | `SHARED_FRAMES` in `is_shared` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/frame.rs:370` | `SHARED_FRAMES` in `release_shared` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/frame.rs:385` | `ALLOCATOR` in `stats` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/frame.rs:417` | `ALLOCATOR` in `cpu_stats` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/frame.rs:447` | `ALLOCATOR` in `free_range` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/heap.rs:57` | `HEAP` in `init` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/heap.rs:66` | `HEAP` in `init` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/heap.rs:76` | `HEAP` in `stats` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/heap.rs:136` | `HEAP` in `alloc` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/memory/heap.rs:156` | `HEAP` in `dealloc` | assert allocator/heap lock is never held across a page fault, IPI or TLB wait; add a fault-injection/parallel alloc test | S83/Stuart (memory) |
| `kernel/src/sched/mod.rs:215` | `?` in `lock_cpu` | assert the per-CPU/thread-cache lock is never held across timer return, context switch or cross-CPU wake; SMP timer/preempt test | S83/Stuart (scheduler) |
| `kernel/src/sched/mod.rs:1067` | `REAP` in `push_reap` | assert the per-CPU/thread-cache lock is never held across timer return, context switch or cross-CPU wake; SMP timer/preempt test | S83/Stuart (scheduler) |
| `kernel/src/sched/mod.rs:1086` | `REAP` in `reap_dead` | assert the per-CPU/thread-cache lock is never held across timer return, context switch or cross-CPU wake; SMP timer/preempt test | S83/Stuart (scheduler) |
| `kernel/src/sched/thread.rs:530` | `STACK_CACHE` in `allocate_kernel_stack` | assert the per-CPU/thread-cache lock is never held across timer return, context switch or cross-CPU wake; SMP timer/preempt test | S83/Stuart (scheduler) |
| `kernel/src/sched/thread.rs:555` | `STACK_CACHE` in `free_kernel_stack` | assert the per-CPU/thread-cache lock is never held across timer return, context switch or cross-CPU wake; SMP timer/preempt test | S83/Stuart (scheduler) |
| `kernel/src/sched/thread.rs:568` | `STACK_CACHE` in `stack_cached` | assert the per-CPU/thread-cache lock is never held across timer return, context switch or cross-CPU wake; SMP timer/preempt test | S83/Stuart (scheduler) |

## Safe groups (summary)

The remaining 382 sites are in process/syscall/IPC/VFS/console paths and are
released before any scheduler wait; they are not reached from a vector handler.
The inventory has the exact file:line for every one, including:
`process/**`, `syscall/**`, `ipc/**`, `memory/pager.rs`, `driver/irq*.rs`,
`dtrace` syscalls, and `process/tty.rs`.

| File group | Sites |
|---|---:|
| `kernel/src/process/mod.rs` | 58 |
| `kernel/src/syscall/process_ctl.rs` | 55 |
| `kernel/src/ipc/router.rs` | 27 |
| `kernel/src/ipc/mod.rs` | 18 |
| `kernel/src/process/handle.rs` | 15 |
| `kernel/src/syscall/event.rs` | 15 |
| `kernel/src/syscall/memory_ctl.rs` | 15 |
| `kernel/src/syscall/priority.rs` | 14 |
| `kernel/src/process/duplex.rs` | 13 |
| `kernel/src/ipc/capture.rs` | 12 |
| `kernel/src/process/tty.rs` | 11 |
| `kernel/src/ipc/registry.rs` | 9 |
| `kernel/src/syscall/ipc.rs` | 9 |
| `kernel/src/syscall/linux.rs` | 9 |
| `kernel/src/driver/mod.rs` | 8 |
| `kernel/src/process/exec.rs` | 8 |
| `kernel/src/syscall/handle_xfer.rs` | 8 |
| `kernel/src/syscall/thread.rs` | 8 |
| `kernel/src/event/mod.rs` | 7 |
| `kernel/src/sync.rs` | 7 |

## Donor pattern

| Topic | Illumos (behaviour only) | BSD (permissive reference) | Linux (outcome only) | Decision |
|---|---|---|---|---|
| IRQ-safe spin lock | `x_call.c`/`hat_i86.c` design; no copy | NetBSD `sys/arch/x86/x86/x86_tlb.c`, `ipi.c`, `sys/kern/subr_xcall.c` (BSD-2); FreeBSD `sys/kern/subr_smp.c` (BSD-2) | `smp_call_function_many` outcomes | keep `IrqSpinLock` only for short, no-wait sections; use the S113 `ActiveGuard` IF-enabled pattern for shootdown waits |
| blocking mutex | `mutex_enter`/turnstile design; no copy | NetBSD `sys/kern/kern_mutex.c` (BSD-2), FreeBSD `sys/kern/kern_mutex.c` (BSD-2) | `mutex_lock` outcomes | `spin::Mutex` is process-context only; no blocking lock in a vector handler |
| page-fault allocator | `hat_i86.c` design | NetBSD `uvm_fault.c`/`vm_page.c` (BSD-2) | outcomes | needs-proof before claiming the allocator lock is safe on the fault path |

Donor revisions: FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`, NetBSD
`36eddebcc36cb0775aa2962005b9c05419a6ff01`, Illumos
`586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`. No donor code copied.

## Handoff and repair order

1. Memory page-fault allocator/heap locks: prove the invariant with a
   concurrent fault/alloc test; if the lock is held across a fault/IPI, replace
   the outer section with an IF-enabled wait pattern (S113 `ActiveGuard` style)
   or move the allocation after the lock is dropped. Owner: S83/Stuart.
2. Scheduler per-CPU `CPUS`, `REAP` and `STACK_CACHE`: prove timer/preempt
   reachability; the scheduler already releases `lock_cpu` before a context
   switch. Owner: S83/Stuart.
3. DTrace `WAKE_EVENT`/`CONSUMER`: establish that probe wake never runs in a
   vector handler, or introduce a dedicated IRQ-safe lock. Owner: S83/Stuart.
4. VT-d and driver `spin::Mutex` sites: document the process-context fault
   worker/DMA path and add a concurrent test. Owner: S83/Stuart.
5. Re-run `tests/smp/irq_lock_audit.py --check` whenever a lock is added; it
   fails if a new `IrqSpinLock` or high-risk site is undocumented.

## Acceptance evidence

| Command | Result |
|---|---|
| `python3 tests/smp/irq_lock_audit.py --check` | PASS (474 sites, 34 IRQ-safe, 55 needs-proof, 0 undocumented) |
| `cargo nextest run -p raven-vmem` | PASS (see report) |
| `sh tests/smp/memory/run.sh tcg --iterations 1` | PASS on this tree (see report; the S83 pre-fix failure is not reproducible) |
| `make check-fast` | PASSED (see report) |
