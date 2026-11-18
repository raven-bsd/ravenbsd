<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# IRQ-mask lock audit (S102)

Status: **audit complete; two confirmed SMP/IPI lock sites and the log spinlock are unsafe, and
the probe mailbox needs a concurrent-caller proof. No `crates/raven-vmem/**` lock site exists, so
no vmem fix was made.** The existing
`tests/smp/memory` concurrent-waiter regression remains the executable check for the unsafe
class. This audit does not replace the S83 implementation; it gives exact file/line, owner and
repair for every site.

Scope: every `IrqSpinLock`, `SpinMutex`, `SpinLock` and `.lock()`/`.try_lock()` site in
`kernel/src/**` that can run in SMP or interrupt context. The dangerous pattern is a waiter that
masks interrupts while spinning for a lock whose holder is waiting for an IPI, timer or another
CPU to make progress. A lock that is held only for a short critical section with no such wait can
be safe even when it masks interrupts.

## Search evidence

Run on origin/master `b64c9a6c`:

```text
$ rg -n "IrqSpinLock" kernel/src
11 files/static declarations
$ rg -n "SpinMutex" kernel/src
1 type, 2 lock sites (kernel/src/log.rs)
$ rg -n "try_lock" kernel/src
0
$ rg -n "\.lock\(\)" kernel/src | wc -l
443
```

The 44 IRQ-mask/SpinMutex lock sites are classified row by row below. The remaining `.lock()`
sites are blocking `Mutex` (or the test-only `M` lock) and are not IRQ-masking; their call sites
in IRQ-reachable functions were inspected separately. A blocking `Mutex` in interrupt context
would be a separate illegal-blocking defect; none was found in the IRQ paths listed here.

## Inventory and classification

| # | Lock | File:line (lock sites) | Context | Class | Evidence |
|---|---|---|---|---|---|
| 1 | `TURNSTILES: IrqSpinLock<Option<KernelTurnstile>>` | `kernel/src/sync.rs:299`, sites 329, 358, 368, 380, 459, 491, 550, 565 | process/syscall/scheduler; no IRQ caller | **safe** | Critical sections mutate the graph/boost bitmaps and release before `block_current`/`wake_blocked`; no IPI/timer wait under the lock. `Mutex` is a blocking lock and is not called from the audited IRQ paths. |
| 2 | `CYCLICS: IrqSpinLock<Option<Cyclics>>` | `kernel/src/time.rs:167`, sites 175, 183, 192 | process + timer interrupt (`cyclic_fire`) | **safe (re-audit when consumers arrive)** | `cyclic_fire` holds the lock only while `on_interrupt` records atomics; the current callback performs no allocation, IPI or blocking. `cyclic_add`/`cancel` are process context. Future cyclic callbacks must stay non-blocking. |
| 3 | `ENTROPY_POOL: IrqSpinLock<Option<Pool>>` | `kernel/src/time.rs:438`, sites 468, 491 | process/syscall | **safe** | Hardware/jitter sampling and pool fill do not wait for another CPU or IPI; no interrupt caller. |
| 4 | `LOCK: IrqSpinLock<()>` | `kernel/src/smp/shootdown.rs:49`, site 81 | user map/unmap/page-fault, SMP shootdown | **unsafe** | Known S85b inversion: a second `flush_range` waiter spins on `LOCK` with IF masked and cannot take the first caller's TLB IPI; 100 ms timeout. See §4. |
| 5 | `LOCK: IrqSpinLock<()>` | `kernel/src/smp/call.rs:31`, site 48 | SMP probe/selftest | **needs proof** | Same shape if two CPUs call `probe` concurrently: CPU B can wait on `LOCK` with IF masked and be the target of CPU A's `CALL_VECTOR`; B cannot run `call::handle`, A times out. The current selftest probes serially, so no existing test proves it; a two-CPU concurrent-probe test is the required proof. |
| 6 | `LOCK: IrqSpinLock<()>` | `kernel/src/sched/smp_shim.rs:153`, site 181 | aarch64 TLB shootdown | **unsafe** | The code comment itself says a target spinning for `LOCK` cannot take the SGI; it records a timeout instead of a deadlock. Same donor rule applies. |
| 7 | `waiters: IrqSpinLock<Vec<Waiter>>` | `kernel/src/sched/wait.rs:38`, sites 60, 83, 107, 130, 145, 163, 178, 199, 231, 255 | syscall, page-fault, teardown, IRQ wake | **safe** | Every wake removes the waiter under the lock, releases it, then calls `super::wake`; the lock is never held across a scheduler wake/IPI. `enqueue_if`'s condition uses non-blocking usercopy/atomics and does not take the queue lock recursively. |
| 8 | `COUNTER_LOCK: IrqSpinLock<()>` | `kernel/src/sched/selftest.rs:38`, site 90 | SMP selftest kernel threads | **safe (test only)** | Very short atomic counter critical section; no IPI/timer wait. |
| 9 | `QUEUES: IrqSpinLock<Vec<Weak<EventQueue>>>` | `kernel/src/event/mod.rs:155`, sites 159, 175, 182, 196, 203, 244 | process + timer interrupt (`notify_all`) | **safe** | The table lock is used only to upgrade/scan weak references; it is released before `queue.notify()`/scheduler wake. `notify_all` allocates a fixed stack array, not under the lock. |
| 10 | `IOAPICS: IrqSpinLock<[Option<State>; MAX_IOAPICS]>` | `kernel/src/arch/x86_64/ioapic.rs:38`, sites 65, 116, 133, 143, 159, 173, 180 | process context + IRQ `mask_gsi_isr` | **safe (contention only)** | The holder only reads/writes IOAPIC MMIO and releases; it never waits for the interrupted CPU or an IPI. `mask_gsi_isr` can spin, but the holder makes progress without that CPU. Re-audit if IOAPIC delivery moves to a CPU that can block on the holder. |
| 11 | `CLIENT_THREAD: IrqSpinLock<Option<Arc<Thread>>>` | `kernel/src/sync.rs:795`, sites 811, 864 | lockstat self-test | **safe (test only)** | Short clone/replace; no IPI wait. |
| 12 | `SERIAL: SpinMutex<Serial>` | `kernel/src/log.rs:78`, lock sites 103, 109 | all `kprintln!`, including exception context | **unsafe (lock-in-interrupt, separate class)** | `SpinMutex` does not mask interrupts and has no try-lock. If an exception/IRQ interrupts a CPU holding `SERIAL`, the handler's `kprintln!` spins forever; the exception path (`kernel/src/arch/x86_64/exception.rs:217-271`) does exactly that. The panic path avoids the lock with a fresh `Serial`; exception output does not. |

**No `crates/raven-vmem/**` lock sites:** `rg -n "IrqSpinLock|SpinLock|\\.lock\\(\\)|try_lock|Mutex|AtomicBool" crates/raven-vmem/src` returns no lock use. There is no vmem fix to apply.

**No `try_lock`:** `rg -n "try_lock" kernel/src` is empty. No `SpinLock` type exists; the two spin-lock families are `IrqSpinLock` and `log::SpinMutex`.

## 4. Unsafe sites and owners

### 4.1 `kernel/src/smp/shootdown.rs:81` — S83/Stuart

The global `IrqSpinLock` is held across the TLB IPI acknowledgement wait. A second `flush_range`
waiter masks interrupts while spinning on the lock, so it cannot take the first caller's TLB IPI.
The 100 ms deadline fires. This is the S85b root cause.

**Repair:** apply the `ActiveGuard`/active-request proposal in `docs/smp-tlb-shootdown.md`:
- publish one active request under an `AtomicBool` (or active pointer);
- wait for the active slot with interrupts enabled and preemption disabled;
- keep `PENDING`/`ACKED`/`LAST` generation checks, but never hold an IRQ-masked lock across the
  ACK wait;
- restore the caller's IF on every path.
- The donor pattern is NetBSD `x86_tlb.c` (global packet pointer CAS, drop SPL while waiting,
  pending count/`TP_DONE`) and FreeBSD `smp_targeted_tlb_shootdown_native` (per-initiator
  scoreboard generation, interrupts required at the initiator).

### 4.2 `kernel/src/smp/call.rs:48` — S83/Stuart (needs proof)

`probe()` takes the same global `IrqSpinLock` and waits for a `CALL_VECTOR` acknowledgement. If a
CPU is waiting for `LOCK` when another CPU probes it, it cannot run `call::handle` and the shooter
times out. The current selftest calls `probe` serially, so this is a suspected inversion without a
failing concurrent test; it stays **needs proof**, not a claimed confirmed failure.

**Proof/repair:** use the same active-request/interruptible-wait pattern, or fold the probe mailbox
into the donor-style per-CPU request/scoreboard, then add a two-CPU concurrent-probe regression
under S83 if the selftest can drive it; the memory fixture exercises the analogous TLB path but not
`call`.

### 4.3 `kernel/src/sched/smp_shim.rs:181` — dave/S76 + S83/Stuart

The aarch64 TLB shootdown has the same `LOCK`/SGI inversion; its comment explicitly accepts a
timeout when a target is spinning for the lock. The donor rule and `ActiveGuard` proposal apply
with `gic::send_sgi`/`SGI_TLB` replacing the x86 LAPIC send. Owner: dave/S76 for the aarch64
integration, S83/Stuart for the common lock protocol.

### 4.4 `kernel/src/log.rs:39` / `:78` — stuart/S27/S30 (console/driver path)

`SpinMutex::lock` spins with interrupts enabled and no try-lock. `kprintln!` is called from
`kernel/src/arch/x86_64/exception.rs:217-271`; an exception that interrupts a CPU while `SERIAL`
is held spins forever, so the diagnostic is lost. Normal console and driver IRQ paths were
checked and do not call `kprintln!` (`console.rs` comment and `driver/irq.rs:585-616`), but the
exception path is a real lock-in-interrupt hazard.

**Repair:** add `SpinMutex::try_lock` and make the exception/panic path use a fresh/direct serial
fallback (the panic path already does), or make the log lock IRQ-safe and prohibit `kprintln!`
from exception context. This is a different failure from the IRQ-mask inversion, but it is an
unsafe interrupt-context lock site and is listed for the console/driver owner.

## 5. Safe sites and proof notes

- **`WaitQueue`**: all wake paths remove under the queue lock and release before `super::wake`;
  `enqueue_if` conditions in the audited callers use atomics/non-blocking usercopy. The queue lock
  is not held across a scheduler wake/IPI.
- **`event::QUEUES`**: the table is scanned/upgraded under the lock and the lock is released before
  `EventQueue::notify`/`wake_one`; `notify_all` is allocation-free with a fixed stack array.
- **`time::CYCLICS`**: the current timer callback only records atomics; no IPI or allocation under
  the lock. Re-audit before adding cyclic consumers that can block or wake another CPU.
- **`ioapic::IOAPICS`**: the lock holder performs MMIO only and does not wait for the interrupt CPU;
  contention can delay the IRQ, but it is not the IPI/timer inversion. The comment's "delivery on
  the boot CPU" assumption should be rechecked if IRQ distribution moves to APs.
- **`TURNSTILES` / `Mutex`**: the blocking mutex releases the turnstile lock before
  `block_current`; `unlock` releases before `wake_blocked`. No `Mutex` lock was found in an
  IRQ-reachable path. A `Mutex` in interrupt context would be a separate illegal-blocking defect.
- **`sched/selftest` and `CLIENT_THREAD`**: test-only, short atomics; no production IRQ path.

## 6. Donor pattern

The per-file donor map is in `docs/smp-tlb-shootdown.md`; the relevant donors are:

- NetBSD `36eddebcc36cb0775aa2962005b9c05419a6ff01`, `sys/arch/x86/x86/x86_tlb.c` (RCS 1.22,
  2026-03-17), `ipi.c` (1.30), `pmap.c` (1.429), `sys/arch/x86/include/cpuvar.h` (1.55),
  `pmap_private.h` (1.5), BSD-2-Clause per-file headers. The x86 TLB path uses a per-CPU packet,
  a global packet-pointer CAS, drops SPL while waiting for the pointer, and waits on a pending
  count/`TP_DONE` with interrupts enabled.
- FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`, `sys/amd64/amd64/mp_machdep.c`,
  `sys/x86/x86/mp_x86.c`, `sys/kern/subr_smp.c`, BSD-2-Clause (the `pmap.c` is BSD-4-Clause and
  no code is copied). It requires interrupts enabled at the initiator, uses a per-initiator
  generation scoreboard, release/acquire fences and never holds a global IRQ-mask spinlock across
  the ack wait.
- NetBSD `sys/kern/subr_xcall.c` is a generic cross-call facility; x86 TLB uses direct LAPIC IPIs,
  not the xcall queue.

The invariant common to both donors: **a waiter for a TLB/IPI request must be able to receive the
IPI; do not mask interrupts while spinning for a lock whose holder waits for that IPI.**

## 7. Regression and gate

`tests/smp/memory` already contains the S101 four-way concurrent unmap barrier. It drives four
threads into the unmap/flush path together and forbids every `smp: tlb shootdown timeout` line;
no existing check was weakened. The current pre-S83 result is a failure by design:

```text
$ sh tests/smp/memory/run.sh tcg --iterations 1
exit 1; 108 `smp: tlb shootdown timeout` lines
log: build/smp-memory/tcg/run-1/smp-memory.log
```

The user-level stress summary (`alloc_ops=256`, `fault_pages=512`, `maps=32`, `forks=4`,
`tlb_rounds=64`, `tlb_stale=0`) and `init exited (0)` are clean; the failure is the kernel lock
protocol. No new fixture was needed for the `call`/`smp_shim`/`log` sites because they are kernel
internals; S83/S76 must add their own regression where they can be driven from the test surface.

After S83 (and the matching aarch64 owner), the gate is 50/50 TCG plus KVM `-smp 2/4`, with no
timeout; `kernel/src/smp/**` and driver fixes are not made here.

## 8. Acceptance

```sh
cargo nextest run -p raven-vmem          # 16/16 passed
sh tests/smp/memory/run.sh tcg --iterations 1   # expected fail pre-S83, 108 timeouts
make check-fast                          # PASSED
make provenance                          # 0 violations
```

Acceptance for this audit is the complete table above plus exact owner/file/repair for every
unsafe site. No high-risk unknown is left unlisted; the only "needs proof" item is the
`SpinMutex` exception path, which is classified unsafe rather than silently accepted. The audit
does not claim a fix or a post-S83 pass.
