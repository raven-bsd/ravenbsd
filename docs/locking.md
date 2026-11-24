<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Locking: turnstiles and priority inheritance

Status: OBSERVED (model) + OBSERVED (kernel glue). Implements the pure half of `kernel-next/04`
R-LCK-5 and R-LCK-6 in `crates/raven-turnstile` (L1). The kernel glue is live: the blocking
`Mutex` in `sync.rs` (L3) and the IPC call-edge donation in `ipc/mod.rs` + `ipc/router.rs` (L6).
`RwLock`/`Condvar` glue remains PROPOSED.

## What a turnstile is

A turnstile is the queue of threads blocked on one lock, keyed by the lock/object id, ordered by
effective priority then arrival, plus the inheritance link to the owner. The owner is the thread
that currently holds the lock; waiters are blocked until the owner wakes them. Release hands the
lock directly to the highest-priority waiter (direct handoff), so a newly woken high-priority
waiter does not race a lower-priority one.

## Priority convention

Priorities follow `raven-sched-core`: `0` is the highest, larger numbers are lower. A thread's
**effective** priority is the most urgent of:

1. its base priority, and
2. the effective priorities of every thread waiting on a lock it owns, transitively.

This is the least fixed point of `eff(t) = min(base(t), { eff(w) | w waits on a lock owned by t })`.
Because priorities only rise toward `0`, the fixed point is unique and is reached by iterating to
stability (OBSERVED in the crate's `compute_effective`). Transitive inheritance satisfies R-LCK-6:
if `A` blocks on a lock owned by `B`, and `B` blocks on a lock owned by `C`, then `C` inherits
`A`'s priority.

## Operations

| Operation | Effect |
|---|---|
| `acquire(object, thread)` | Set `thread` as owner; fails if already owned or thread unknown. |
| `block(object, thread)` | Enqueue `thread` at its effective priority; may raise the owner. |
| `wake_one(object)` | Hand the lock to the highest-priority waiter; release if none. |
| `wake_all(object)` | Release and wake every waiter, priority first. |
| `cancel(object, thread)` | Remove a waiter without waking it; recompute the owner. |
| `boost(object)` | Recompute and report the owner's inheritance effect (idempotent). |
| `unboost(object)` | Drop the contribution; requires no waiters; reports restoration. |

`block`/`wake`/`cancel` maintain inheritance automatically; `boost`/`unboost` are the explicit
recomputation points the kernel calls at the owner boundary and report an `InheritEffect`.

## Invariants

The crate's `check_invariants` verifies, and the exhaustive model check exercises:

1. An owner never appears in its own waiter list.
2. A waiter list has no duplicate thread and is sorted by `(priority, arrival)`.
3. A thread waits on at most one object.
4. The owner is boosted to the highest waiter (its effective priority is the most urgent of its
   base and all waiters on locks it owns).
5. The owner is restored to its base priority when the last waiter on its last held lock leaves.
6. Cancellation removes exactly one waiter and loses no other waiter.
7. A waiter is woken at most once.
8. No priority inversion: while a high-priority thread waits, the owner runs at a priority at least
   as urgent as every merely-runnable lower/equal thread, so none of them preempts the owner.

## Model-check bounds

Exhaustive breadth-first search over the real transition functions:

* 2 threads, 2 objects, 2 priority levels, depth 6;
* 3 threads, 1 object, 2 priority levels, depth 7.

The search reports a reproducible state count and fails on the first violated invariant with a
counterexample trace. Two seeded-bug tests prove the checker flags a planted `EffectiveMismatch`
and a planted `WaiterOrdering` (R-ADD-28 style).

A deterministic multi-million-operation soak (`storm_millions_of_mixed_operations`, 3M mixed
wait/wake-one/wake-all/cancel/acquire/boost/unboost with object and thread reuse plus stale ids)
asserts, at every step, the conservation invariant `blocks == wakes + cancels + queued` (no lost or
double-counted wake) and re-runs the full invariant check periodically. Two targeted tests cover a
32-owner deep inheritance chain (the deepest owner inherits the highest priority transitively, and
every intermediate owner is boosted to the same value) and equal-priority fairness (64 waiters hand
off in arrival order).

## Lock statistics (`lockstat` feature)

`crates/raven-turnstile` has an off-by-default `lockstat` feature. When enabled, each turnstile
carries per-object counters (`Turnstiles::stats(object) -> LockStats`) that the kernel `lockstat`
probes publish (L5). The crate's default build has no counters and no extra cost or layout.

Kernel build mechanism (L5, OBSERVED): `kernel/Cargo.toml` declares a `lockstat` feature that is
**on by default** (`default = ["lockstat"]`). The kernel includes the pure model by `#[path]`, so
that feature cfg reaches the same `#[cfg(feature = "lockstat")]` counters the host suite tests;
no second flag and no `raven-turnstile` dependency edge is needed. `--no-default-features` drops
the counters, `sync::lockstat()` and the `selftest=lockstat` arm (both arches compile clean that
way). Cost: with the feature on, each model object carries eight `u64` saturating counters and each
lock operation performs one saturating add plus, for `block`, the queue/depth maximum update. The
byte and cycle overhead is **UNMEASURED** (no timed kernel benchmark in this ticket).

`sync::lockstat()` returns `Vec<(ObjectId, LockStats)>` over every object the graph has touched.
`selftest=lockstat` runs a bounded workload (one owner, two waiters at higher priority, one
inherited boost, two handoffs) and prints one machine-readable line; the `lockstat` qtest asserts
it. Observed QEMU line:

```
lockstat: ok object=3 wait_entries=2 wakes=2 cancels=0 timeouts=0 boosts=1 max_queue=2 owner_boost=0 restored=ok
```

`boosts` is `inherited_priority_changes` (how often an operation changed an owner's effective
priority), `max_queue` is `max_queue_depth`; `timeouts` is always `0` because the pure model has no
timeout path. The counter counts the blocking edge that actually raised the owner and any handoff
that changes the incoming owner's priority; the bounded two-waiter run above observes `1` (the
second waiter has lower priority than the first and does not raise the owner again).

| Counter | Meaning |
|---|---|
| `wait_entries` | successful `block` calls |
| `immediate_acquisitions` | successful `acquire` calls |
| `wakes` | threads woken (`wake_one` + `wake_all`) |
| `cancels` | waiters removed by `cancel` |
| `timeouts` | reserved for the kernel timeout path; always `0` here |
| `max_queue_depth` | largest number of simultaneous waiters |
| `max_inheritance_depth` | longest inheritance chain observed (the block hop plus the owner's own wait-for chain) |
| `inherited_priority_changes` | number of times an operation changed an owner's effective priority |

Every counter is saturating (never wraps silently). Equality/hashing of the model state ignores the
counters, so the exhaustive model check still deduplicates on the logical state; the same model
additionally asserts `timeouts == 0`, `max_queue_depth >= current waiters` and
`wait_entries >= current waiters` on every reachable state under the feature.

## IPC priority donation (`Donation`)

`Donation` (the `donation` module) layers IPC call edges (`client -> server`) on top of a
`Turnstiles` registry. A client blocked on a server reply donates its priority to the server, and
the boost flows through any lock the server then waits on. Each thread's effective priority is the
least fixed point over three edge kinds:

`eff(t) = min(base(t), { eff(w) | w waits on a lock owned by t }, { eff(c) | c has a call to t })`

* `begin_call` / `end_call` / `cancel_call` / `client_death` / `server_death` manage the call
  edges; completion, cancellation and death remove exactly the right edges with typed errors
  (`UnknownThread`, `DuplicateCall`, `StaleCall`).
* `effective_priority` is an allocation-free cached lookup; lock operations are delegated to the
  turnstile and every mutation recomputes the fixed point.
* `check_invariants` verifies: no dangling call edge, the cached effective priority equals the
  least fixed point, and a thread with no donor (call or lock) is back at its base priority.

The host model runs 1,000,000 mixed operations (3 servers, 64 clients, 16 locks) and asserts the
invariants every 512 steps; a hand-built inversion case proves a low-priority lock owner runs at a
high-priority client's priority while the server waits on its lock, and returns to base once the
lock is released and the call completes.

## Kernel integration

Landed (OBSERVED):

* `kernel/src/sync.rs` hosts one global `Donation` graph (lock wait edges plus IPC call edges) and
  a `tid -> Weak<Thread>` map. `Mutex::lock` registers the caller, blocks in the turnstile and
  applies the returned `InheritEffect`; `Mutex::unlock` hands the lock to the highest-priority
  waiter and re-syncs the releaser and the new owner. `sync_thread_boost` writes the graph's
  effective priority to `Thread::boost` through `Thread::set_boost`/`clear_boost`, which the
  scheduler's `effective_priority` reads (`AtomicU8`, cross-CPU safe).
* `kernel/src/ipc/router.rs` opens the call edge in `deliver`, where the receiving server thread is
  known (`PendingRequest::begin_donation`). The client tid is captured when the request is created.
  `PendingRequest::complete` closes the edge, so a reply, cancellation, client death and server
  death all remove it exactly once (`server_tid` is a swap-to-zero marker). One-way CLOSE/DUP
  notifications get no edge because no client blocks on them.
* Composition: the graph's least fixed point already folds call edges into lock inheritance, so a
  client -> server -> lock-owner chain runs the owner at the client's priority. After every edge
  change the kernel re-applies all registered threads' effective priorities
  (`sync_all_boosts`), which also restores base priorities when an edge disappears.
* Teardown (`L6b`): `donation_end_call` uses `Donation::end_call_no_alloc`. It removes the edge
  immediately and recomputes only the server's dependent closure (lock owners it waits on, call
  servers it calls) with a local fixed point over a fixed 64-thread array; outside that closure
  the cached priorities are stable, so the result is exact. A wider closure marks the graph dirty
  and the next allocating mutation refreshes it wholesale. Nothing allocates or sorts on the
  process-death, fault or cancel path, which is what `proc-death-cleanup` exercises.
* Nested calls compose recursively: the boosted server thread is itself the client of the next
  call, and its effective priority (not its base) is the donation value.
* `selftest=locking`, `selftest=lockstat` and `selftest=ipc-donation` are QEMU qtests (`locking`,
  `lockstat`, `ipc-donation`); both assert the observed boost and the restoration.
* Wake protocol (`L6b`): a thread that blocks for a selftest sets its "asleep" flag *after*
  `begin_block`; the waker waits for that flag and calls `sync::wake_blocked`, which retries until
  `sched::wake` accepts the wake (it returns `false` for a `Ready` thread without leaving a
  pending wake). `Mutex::unlock` uses the same retrying wake for a handed-off waiter, closing the
  preemption window between `block` and `block_current`. The lockstat owner sleeps while both
  waiters queue, so no spin gate depends on a particular CPU placement.

The global recomputation may be replaced by incremental propagation along the wait-for chain
(bounded depth 8) as long as it yields the same effective priorities (PROPOSED). Deadlock
detection and the remaining R-LCK-7 consumers are separate tickets; `RwLock`/`Condvar` glue and
the user-visible priority syscalls remain PROPOSED.

## Lock order and IPI waits (S85a rework 3)

Status: **OBSERVED** on `origin/master` `eb3ac161` (merged into `lance/S85a` at `192108c3`),
pre-S113. This section is the lock-order audit required before the 50/50 TCG/KVM retry.

### The failure class

S106 (aarch64), S118 (x86-64) and S113 describe one signature: the shooter logs
`smp: tlb shootdown timeout ... flushes=0`, and the target is a peer thread spinning on the
process `address_space` `spin::Mutex` with interrupts masked. On x86-64 `SYSCALL_FMASK` clears
`RFLAGS.IF` (`kernel/src/arch/x86_64/syscall.rs:24`) and `sched_syscall_enter`
(`kernel/src/sched/context.rs:595-608`) does not re-enable it. On aarch64 the SVC vector enters
with `PSTATE.DAIF` masked (`kernel/src/arch/aarch64/vectors.rs:255-272`) and `svc_dispatch`
(`kernel/src/syscall/mod.rs:414-445`) never unmasks it. The peer therefore cannot take the TLB
IPI until it leaves the syscall or reaches the interruptible wait; the shooter holds the
address-space lock the peer is spinning on, so the two CPUs wait for each other until the
100 ms deadline.

### The rule

**No spinlock across an IPI wait.** A CPU that waits for another CPU's acknowledgement must not
hold a lock that the target may need to reach the interrupt handler or to make progress.

1. A raw `spin::Mutex` (the address-space lock is `Process::address_space`,
   `kernel/src/process/mod.rs:214`) must never be held across `shootdown::flush_range` or any
   other IPI-wait call. Mutate the page tables under the lock, record the range, release the
   lock, then flush. If the caller cannot release the lock, the shootdown must be asynchronous
   (publish and return; acknowledge later) rather than wait under the lock.
2. The shooter-side serialisation may be an `ActiveGuard`/`IrqSpinLock` only when the wait
   re-enables interrupts and the IPI handler never takes that lock. Both current implementations
   do this: x86-64 `ActiveGuard` (`kernel/src/smp/shootdown.rs:48-95,140-167`) and aarch64
   `ActiveGuard` (`kernel/src/sched/smp_shim.rs:195-227,285-312`) enable interrupts around the
   wait and keep `preempt_count` raised.
3. A `raven-turnstile` blocking `Mutex` must not be taken in interrupt context, with interrupts
   masked, or while holding an `IrqSpinLock`; it may sleep and needs the scheduler. An
   `IrqSpinLock` is non-blocking, masks interrupts and must not be held across a wait.
4. The IPI handler must not take a lock the target may be spinning on. The TLB handler only
   touches the mailbox atomics and executes `invlpg`/`tlbi`; the process address-space lock is
   never taken in the handler.

### Inventory: every flush_range / IPI-wait call site

`flush_range` is architecture-selected: x86-64 `kernel/src/smp/shootdown.rs:102`, aarch64
`kernel/src/sched/smp_shim.rs:245`. All architecture-independent callers below use the same
`crate::sched::smp::shootdown::flush_range` name and therefore apply to both ISAs.

| Site | Path | Lock held at the call | Wait interruptible? | Verdict |
|---|---|---|---|---|
| `kernel/src/memory/uspace.rs:625` | `fork_clone_from` ← `process::fork` (`process/mod.rs:543-547`) | source **and** child `address_space` `spin::Mutex` | no (syscall entry masks IF/DAIF) | **violation** |
| `kernel/src/memory/uspace.rs:909` | `unmap` ← `memory_ctl::unmap` (`memory_ctl.rs:259-266`), `unmap_current` (`:325-328`), `map(REPLACE)` (`:188-197`) | `address_space` `spin::Mutex` | no | **violation** (3 callers) |
| `kernel/src/memory/uspace.rs:979` | `protect` ← `memory_ctl::protect` (`memory_ctl.rs:344-346`) | `address_space` `spin::Mutex` | no | **violation** |
| `kernel/src/memory/uspace.rs:1065` | `handle_fault` COW fast path ← `process::resolve_page_fault` (`process/mod.rs:1728-1732`) | `address_space` `spin::Mutex` | no | **violation** |
| `kernel/src/memory/uspace.rs:1100` | `handle_fault` COW copy path ← `resolve_page_fault` | `address_space` `spin::Mutex` | no | **violation** |
| `kernel/src/memory/uspace.rs:1140` | `map_pager_page` ← `memory/pager.rs:77-81`, and `uspace.rs:1019` from `handle_fault` | `address_space` `spin::Mutex` | no | **violation** |
| `kernel/src/ipc/capture.rs:527` | `unmap_partial` ← `Capture` map failure (`capture.rs:459,466,484`) | `server.address_space` `spin::Mutex` (`:445`) | no | **violation** |
| `kernel/src/ipc/capture.rs:282` | `Drop for Capture` | none: the guard at `:258` ends before the flush | yes | **clean** |
| `kernel/src/smp/selftest.rs:282` | boot SMP selftest | none (boot selftest; no address-space lock) | yes (ActiveGuard) | **clean** (diagnostic) |
| `kernel/src/sched/smp_shim.rs:542` | aarch64 `tlb_shootdown_proof` | none (boot selftest; no address-space lock) | yes (ActiveGuard) | **clean** (diagnostic) |
| `kernel/src/smp/call.rs:72` | probe wait ← `smp/selftest.rs:174,253,287` | none (boot selftest; no address-space lock) | yes (ActiveGuard) | **clean** (diagnostic) |

The shooter-side implementations themselves are clean:
`kernel/src/smp/shootdown.rs:102-169` and `kernel/src/sched/smp_shim.rs:245-314` use
`ActiveGuard`, re-enable interrupts during the acknowledgement wait, and keep preemption
disabled. The violation is the caller's `address_space` lock, not the shootdown mailbox.

### S113 design check

S113's report proposes enabling interrupts in `sched_syscall_enter`
(`kernel/src/sched/context.rs:595-608`) and masking them again in `sched_syscall_exit`
(`:613-626`). Against the turnstile/IRQ-state rules:

* **Compatible as a delivery fix.** A syscall is process context, not interrupt context, and the
  kernel's preemption policy is cooperative: `timer_return`/`irq_return` only preempt a return
  to user mode (`kernel/src/sched/mod.rs:568-573,1036-1047`), so enabling IF does not preempt a
  kernel-mode holder of the address-space raw spinlock. IRQ handlers do not take
  `Process::address_space`; the TLB handler does not either.
* **Not a lock-order fix.** The address-space `spin::Mutex` is still held across the wait at all
  the sites above. It works only because the target can now take the IPI while spinning. A
  target in an `IrqSpinLock`/`cli` section, a nested exception, or any future preemptible-kernel
  path still times out. The rule in this document remains unsatisfied until the lock is released
  before the flush (or the shootdown is asynchronous).
* **Ordering requirement for S113.** `sched_syscall_exit` must mask interrupts *before*
  restoring the FP/SIMD state, and `sched_syscall_enter` should enable them only after the FPU
  save, so the interrupt handler cannot observe a half-saved FPU area. No other contract change
  is implied.
* **aarch64 S103-r2.** S103's `ActiveGuard` fixed the shooter side
  (`kernel/src/sched/smp_shim.rs:195-227,285-312`), but the SVC entry still masks `DAIF` and
  `svc_dispatch` (`kernel/src/syscall/mod.rs:414-445`) never unmasks it. S106 still sees six
  timeouts with S103 applied; Dave/S103-r2 must add the same process-context interrupt window or,
  preferably, move the flush out of the address-space lock.

### Exact hand-off

| Owner | File:line | Required change |
|---|---|---|
| **Stuart/S113** | `kernel/src/sched/context.rs:595-626`; `kernel/src/arch/x86_64/syscall.rs:24,61-64` | Enable IF in `sched_syscall_enter` (after FPU save), mask before the FPU restore in `sched_syscall_exit`; keep the syscall return path's `preempt_count` semantics. |
| **Stuart/S113** | `kernel/src/syscall/memory_ctl.rs:188-197,259-266,325-328,344-346`; `kernel/src/memory/pager.rs:77-81`; `kernel/src/process/mod.rs:543-547,1728-1732`; `kernel/src/memory/uspace.rs:625,909,979,1065,1100,1140`; `kernel/src/ipc/capture.rs:445,527` | Durable fix: split the address-space mutation from `flush_range`; release the `spin::Mutex` before the wait, or make the shootdown asynchronous. Do not hold a raw spinlock across the wait. |
| **Dave/S103-r2** | `kernel/src/arch/aarch64/vectors.rs:255-272`; `kernel/src/syscall/mod.rs:414-445`; `kernel/src/sched/smp_shim.rs:195-227,245-314` | Open `PSTATE.DAIF` for the SVC process context (or the equivalent interrupt window) and apply the same split-phase lock release; S103's ActiveGuard alone is not the target-side fix. |
| **S85a/lance** | this document; `crates/raven-turnstile/src/ipi_order.rs` + `ipi_order_ut.rs` | Keep the host-side rule model and rerun the 50/50 TCG/KVM matrix only after S113 merges and the host is exclusive. |

No kernel source is edited by S85a rework 3. The 50/50 TCG/KVM retry is deferred as directed;
the previous BLOCKED evidence stands until S113 lands.

### DR-52 reconciliation

Donor revisions: NetBSD `36eddebcc36cb0775aa2962005b9c05419a6ff01`, FreeBSD
`133344405f532bd54ff3629a77d4c4e062f01fdb`, Illumos
`586678ac1bcbd77644a6ec4dac7f79d676cfb4ec`, Linux
`adc218676eef25575469234709c2d87185ca223a` (read-only).

| Topic | Illumos finding | BSD file / licence | Linux finding | Decision |
|---|---|---|---|---|
| TLB shootdown wait vs. lock | `usr/src/uts/common/vm/hat.h` (CDDL): S118's Illumos study records HAT invalidation targeting a CPU set, no global lock, and a wait through xcall SYNC. Design only. | NetBSD `sys/arch/x86/x86/x86_tlb.c` RCS 1.22 (2-clause BSD): `pmap_tlb_shootnow` drops `splvm` while spinning on the global packet and reacquires it; `pmap_tlb_shootdown` only queues. `sys/kern/subr_xcall.c` RCS 1.40 (2-clause BSD): `xc_wait` requires `!cpu_intr_p()`/sleepable context and uses a mutex+CV. FreeBSD `sys/amd64/amd64/pmap.c` 4-clause NO-COPY; `mp_machdep.c` 2-clause scoreboard. | Sparse checkout has no MM sources; S118 records `arch/x86/mm/tlb.c` / `kernel/smp.c` GPL read-only: `smp_call_function_many` documents "must not be called with disabled interrupts" and uses `SCF_WAIT`. | **Rule confirmed**: no IRQ-masked spinlock across the wait; the target must be interruptible. No donor code copied. |
| Target interrupt state | HAT/xcall design expects the target to take the cross-call; no syscall-entry mask. | NetBSD `pmap_tlb_shootnow` explicitly drops `splvm` while spinning; `xc_wait` asserts sleepable context and rejects interrupt context. | S118 records Linux's synchronous `smp_call_function_many` requirement: not with disabled interrupts. | **S113 IF enable is the right delivery shape**; aarch64 still needs S103-r2. |
| Lock order | No global HAT lock; xcall is sleepable. | NetBSD drops the IPL while spinning; `xc_wait` asserts sleepable. | `SCF_WAIT` not with disabled interrupts. | **Split the address-space lock from the flush**; the inventory above lists every site. |

No donor code was copied or translated, so `PROVENANCE.toml` is unchanged and there is no new
import.
