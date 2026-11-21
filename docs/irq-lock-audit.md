<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# IRQ-mask lock audit (S102)

Status: **audit complete on `origin/master` `978899da`.** The tree has **479** lock/try-lock
call sites and **66** lock declarations. **31 sites are unsafe**, **69 need proof**, **31 are
IRQ-safe (short, no IPI/timer wait)**, **340 are process-context safe**, and **8 are test-only**.
No `crates/raven-vmem/**` lock site exists, so no vmem fix was made. The S83 `ActiveGuard` TLB
fix is already merged and is re-verified here, not reimplemented. This document gives the exact
file/line, lock type, interrupt state, classification, donor rule and owner for every site.

The machine-readable inventory is `tests/smp/irq-lock-audit/inventory.tsv`; the regression
checker is `tests/smp/irq-lock-audit/check.py`. The checker fails on any new lock, any new or
stale call site, and any unsafe/needs-proof row that is not cited here.

## 1. Lock primitives and interrupt semantics

| Primitive | Where | Interrupt state while held | Blocking behaviour |
|---|---|---|---|
| `IrqSpinLock<T>` | `kernel/src/sync.rs:180`, `lock` at `:200` | Saves IF/DAIF and masks interrupts **before** the spin; increments the preemption counter | Spins; may not be taken recursively from an interrupt; an interrupt-reachable user must never sleep or allocate under it |
| `IrqGuard` | `kernel/src/sync.rs:211` | Restores the saved IF/DAIF on drop | Critical section must be short and allocation-free |
| `critical()` | `kernel/src/sync.rs:175` | Masks interrupts and increments preemption, no data lock | Used around scheduler state; same no-sleep rule |
| `log::SpinMutex<T>` | `kernel/src/log.rs:20`, `lock` at `:39`, `try_lock` at `:55` | **Does not mask interrupts** | Spins; a holder interrupted on the same CPU deadlocks the handler unless the handler uses `try_lock` |
| blocking `Mutex<T>` | `kernel/src/sync.rs:414`, `lock` at `:453` | Leaves interrupts enabled; may block on a turnstile | May allocate and sleep; **not legal in interrupt context** (NetBSD asserts `!cpu_intr_p()` for the equivalent) |
| `spin::Mutex<T>` | architecture signal scratch | Does not mask interrupts | Short spin; cannot be held across a fault-capable usercopy |

There is **no `SpinLock` type** in the tree; the two spin families are `IrqSpinLock` and
`log::SpinMutex` (plus the external `spin::Mutex` signal scratch). The dangerous pattern is not
"spinlock" by itself: it is a waiter that masks interrupts while the holder needs an IPI, timer,
fault, or another CPU to release the lock.

## 2. Interrupt and exception entry points

| Entry | Path | Locks reachable in that context |
|---|---|---|
| x86 timer IRQ | `kernel/src/arch/x86_64/timer.rs:260` `time::on_tick`, `:261` `sched::tick` | `CYCLICS`, `QUEUES`, `waiters` wake paths, `CPUS`/`REAP` |
| x86 device IRQ | `kernel/src/arch/x86_64/exception.rs:216` `driver::irq::handle_interrupt` | `IOAPICS` (`mask_gsi_isr`), `waiters` wake, `QUEUES` (`notify_all`), `CPUS` via wake |
| x86 IPI | `kernel/src/arch/x86_64/ipi.rs:141` `sched::tick`; TLB/probe handlers | `CPUS`, `REAP`; TLB/probe handlers are atomics only |
| x86 page fault | `kernel/src/process/mod.rs:1952` `resolve_page_fault` | blocking `address_space`, `critical()`, `ALLOCATOR`, `SHARED_FRAMES`, TLB shootdown |
| aarch64 IRQ | `kernel/src/arch/aarch64/irq.rs:74` `dispatch`, `:124` `sched::tick` | same scheduler/event locks; GIC/SPI driver path |
| aarch64 timer | `kernel/src/arch/aarch64/timer.rs:94` `time::on_tick` | `CYCLICS`, `QUEUES` |
| aarch64 exception | `kernel/src/arch/aarch64/vectors.rs:249` `exception_dispatch` | `SIGNAL_CONTEXT`, `SERIAL` via `kprintln!` |
| syscall/process context | `kernel/src/syscall/**` | every process-context lock; may be preempted at any point |

The audit uses this table to decide reachability. A lock used only from `syscall/**` or a
process-context helper is `SAFE` only when no vector handler can enter the same function and no
IRQ-mask lock is held around it.

## 3. Lock declarations

### 3.1 `IrqSpinLock` (8 declarations)

| Declaration | Sites | Class | Invariant |
|---|---|---|---|
| `kernel/src/event/mod.rs:155` `QUEUES` | `159`, `175`, `182`, `196`, `203`, `244` | IRQ-SAFE except `register_queue` (needs proof) | The timer path (`notify_all`, sites `196`/`203`) upgrades weak refs under the lock and wakes outside it; no allocation. `register_queue` at `159` does `retain`/`push` under the lock, so it is listed under needs-proof. |
| `kernel/src/sync.rs:299` `TURNSTILES` | `329`, `358`, `368`, `380`, `459`, `491`, `550`, `565` | IRQ-SAFE except the allocating sites | Graph lookups (`329`, `550`) and the explicitly allocation-free `donation_end_call` (`380`) are short. `register_thread` (`358`), `donation_begin_call` (`368`), `Mutex::lock` (`459`), `Mutex::unlock` (`491`) and `lockstat` (`565`) mutate `BTreeMap`/`Vec` under the lock and are needs-proof. |
| `kernel/src/sync.rs:795` `CLIENT_THREAD` | `811`, `864` | IRQ-SAFE | Test-only `Arc` clone/replace; no allocation or IPI wait. |
| `kernel/src/time.rs:167` `CYCLICS` | `175`, `183`, `192` | IRQ-SAFE except `cyclic_add` | The timer callback (`192`) only records atomics. `cyclic_cancel` (`183`) is allocation-free. `cyclic_add` (`175`) does `Vec::push` and is needs-proof. |
| `kernel/src/time.rs:438` `ENTROPY_POOL` | `468`, `491` | IRQ-SAFE | Pool fill/reseed only; the pool is built before the lock and assigned; no IPI/timer wait. |
| `kernel/src/arch/x86_64/ioapic.rs:38` `IOAPICS` | `65`, `116`, `133`, `143`, `159`, `173`, `180` | IRQ-SAFE | The holder reads/writes IOAPIC MMIO and releases; `mask_gsi_isr` (`159`) runs in IRQ context but the holder never waits for the interrupted CPU or an IPI. |
| `kernel/src/sched/selftest.rs:38` `COUNTER_LOCK` | `90` | IRQ-SAFE (test) | Very short atomic counter critical section. |
| `kernel/src/sched/wait.rs:38` `waiters` | `61`, `99`, `126`, `149`, `164`, `182`, `197`, `218`, `250`, `274` | UNSAFE at `enqueue_if`; IRQ-SAFE on every wake/remove path | `wake*` removes under the lock and releases before `sched::wake` (`99`-`250`). `enqueue_if` at `61` runs a caller closure and `Vec::push` under the lock; see F1. |

### 3.2 `log::SpinMutex` (4 declarations)

| Declaration | Sites | Class | Why |
|---|---|---|---|
| `kernel/src/log.rs:95` `SERIAL` | `106` (test), `130`, `136`, `148` (try-lock) | **UNSAFE** at `130`/`136`; SAFE at `148`; test at `106` | `_print`/`_print_line` spin with interrupts enabled. Exception/fault paths must use `kprintln_emergency!`; aarch64 has no try-lock/emergency variant. See F4. |
| `kernel/src/memory/frame.rs:134` `ALLOCATOR` | `165`, `253`, `265`, `277`, `306`, `320`, `385`, `417`, `447` | NEEDS-PROOF | Reached from the page-fault path via `alloc_zeroed_frame` under `critical()`; the holder must never fault, allocate, or wait for an IPI. |
| `kernel/src/sched/mod.rs:119` `CPUS` | physical site `215`; wrapper sites below | NEEDS-PROOF | Every local-slot acquisition must be under `sync::critical()`; remote slots never wait for that CPU. The wrapper table is in §5.2. |
| `kernel/src/sched/mod.rs:1064` `REAP` | `1067`, `1086` | **UNSAFE** | `push_reap` is reachable from the IRQ-return scheduling path and from process context; the `Vec::push` may allocate. See F2. |

### 3.3 `spin::Mutex` signal scratch (2 declarations)

`kernel/src/arch/x86_64/syscall.rs:189` and `kernel/src/arch/aarch64/vectors.rs:332`
`SIGNAL_CONTEXT` are external spin mutexes used to format an `MContext` in the signal and
fault-return paths. The lock sites are x86 `250`, `290`, `301`, `322` and aarch64 `356`,
`411`, `422`, `443`. They are **NEEDS-PROOF** because `signal_commit`/`sigreturn` perform
fault-capable `copy_to_user`/`copy_from_user` under the lock. See §5.3.

### 3.4 Blocking `Mutex` declarations (52; 49 production, 3 test)

These are process-context locks unless a vector handler or an IRQ-mask lock reaches them. The
full declaration list is below; the unsafe uses are the `enqueue_if` conditions (F1), the
page-fault `SHARED_FRAMES` use (F3), and the nested readiness/pid-registry locks in F1.

| File | Declarations (`file:line name`) |
|---|---|
| `kernel/src/arch/x86_64/vtd.rs` | `98 iova`, `99 tables`, `263 context_tables`, `264 domains`, `265 by_sid`, `266 table_frames`, `270 UNIT` |
| `kernel/src/driver/mod.rs` | `79 DMA_DOMAIN`, `88 DEVICE_HINTS`, `184 BLOBS`, `252 PLATFORM_OWNED` |
| `kernel/src/dtrace/mod.rs` | `40 WAKE_EVENT`, `42 CONSUMER` |
| `kernel/src/event/mod.rs` | `29 state` |
| `kernel/src/ipc/mod.rs` | `165 packet`, `167 buffers`, `169 response` |
| `kernel/src/ipc/registry.rs` | `18 REGISTRY` |
| `kernel/src/ipc/router.rs` | `72 CLOSE_NOTICES` |
| `kernel/src/memory/frame.rs` | `143 SHARED_FRAMES` |
| `kernel/src/memory/heap.rs` | `47 HEAP` |
| `kernel/src/memory/mmio.rs` | `34 CURSOR` |
| `kernel/src/process/duplex.rs` | `287 state` |
| `kernel/src/process/handle.rs` | `58 state`, `442 LINEAGE` |
| `kernel/src/process/mod.rs` | `80 PID_REGISTRY`, `86 ZOMBIES`, `236 ids`, `240 command`, `244 ctty`, `246 address_space`, `248 handles`, `263 state`, `267 namespace`, `269 root`, `271 schemes`, `273 scheme_queue`, `277 active_requests`, `279 client_requests`, `281 capture_slots`, `283 personality`, `285 pending_signals`, `287 pending_exec`, `301 limits` |
| `kernel/src/process/pipe.rs` | `160 state` |
| `kernel/src/process/robust.rs` | `36 LISTS` |
| `kernel/src/process/tty.rs` | `50 REGISTRY` |
| `kernel/src/sched/thread.rs` | `519 STACK_CACHE` |
| `kernel/src/sync.rs` | `580 M`, `650 M`, `794 M` (test-only) |
| `kernel/src/syscall/record_lock.rs` | `38 TABLE` |

## 4. Unsafe sites

### F1. `WaitQueue::enqueue_if` runs a blocking condition under an `IrqSpinLock` — 24 sites

`kernel/src/sched/wait.rs:61` takes the wait-queue `IrqSpinLock`, then calls the caller's
`condition()` and may `Vec::push` the waiter. `IrqSpinLock` masks interrupts and increments
preemption. If the condition takes a blocking `Mutex`, that mutex can block; `block_current`
context-switches with the wait-queue lock still held. A waker (including the timer or a driver
IRQ) then spins on `waiters` with interrupts masked. The same applies to the `Vec::push` when it
needs to grow: it can take `HEAP` while interrupts are masked.

The API contract says "the condition runs while the queue lock is held"; it does **not** say the
condition may block or allocate. Fifteen call sites were inspected:

| Call site | Condition body | Verdict |
|---|---|---|
| `kernel/src/syscall/event.rs:236` | `queue.state.lock().has_ready(readiness)` — blocking `Mutex` plus the nested `readiness` locks at `332`, `341`, `349`, `356` | **UNSAFE** |
| `kernel/src/syscall/record_lock.rs:347` | `TABLE.lock().conflict(...)` — blocking `Mutex` at `348` | **UNSAFE** |
| `kernel/src/syscall/process_ctl.rs:1094` | `children_for(...)` allocates a `Vec` and takes `PID_REGISTRY` at `391`/`405`, `ids` at `1000`/`1007`/`1014`/`1021`, and `state` at `1029` | **UNSAFE** |
| `kernel/src/syscall/process_ctl.rs:1242` | `caller.has_pending_signal()` takes `pending_signals` at `kernel/src/process/mod.rs:733` | **UNSAFE** |
| `kernel/src/syscall/futex.rs:102` | `copy_from_user` — may fault and enter the pager while the queue lock is held | **UNSAFE** (proof required even if the page is resident) |
| `kernel/src/process/pipe.rs:381` (condition at `234`/`298`) | `end.pipe.state.lock()` — blocking `Mutex` | **UNSAFE** |
| `kernel/src/process/duplex.rs:988` (conditions at `563`, `660`, `789`, `896`) | `end.pair.state.lock()` — blocking `Mutex` | **UNSAFE** |
| `kernel/src/memory/pager.rs:57` | `object.page_ready(page)` takes `MemObject::state` at `kernel/src/process/handle.rs:209` | **UNSAFE** |
| `kernel/src/driver/irq.rs:532` | `irq.pending() == 0 && !irq.cancelled(handle)` — atomics only | SAFE |
| `kernel/src/syscall/linux.rs:294` | `!request.done` — atomic | SAFE |
| `kernel/src/syscall/debug_read.rs:65` | `!console::available()` — ring atomics | SAFE |
| `kernel/src/ipc/router.rs:655` | `Ok(true)` | SAFE |
| `kernel/src/ipc/router.rs:1101` | `!request.done` — atomic | SAFE |
| `kernel/src/syscall/process_ctl.rs:950` | `Ok(true)` | SAFE |

The unsafe rows in the inventory are `kernel/src/sched/wait.rs:61`; the `state` condition at
`kernel/src/syscall/event.rs:239`; `kernel/src/syscall/record_lock.rs:348`; the five
`process_ctl` condition rows (`1000`, `1007`, `1014`, `1021`, `1029`); the nested
`kernel/src/process/mod.rs:733`, `:391`, `:405`; `kernel/src/process/handle.rs:209`; the pipe
rows `kernel/src/process/pipe.rs:87`, `:234`, `:298`; the duplex rows
`kernel/src/process/duplex.rs:238`, `:563`, `:660`, `:789`, `:896`; the readiness rows
`kernel/src/syscall/event.rs:332`, `:341`, `:349`, `:356`.

**Repair pattern (S83/Stuart, scheduler/IPC):** never execute a caller closure under the
wait-queue lock. Either (a) evaluate the predicate before enqueue and publish an
`AtomicU64` generation that a waker increments under the same lock, then re-check the generation
under the lock; or (b) split `enqueue_if` into `check` + `enqueue` with a generation argument and
make every waker bump the generation. The waiter `Vec` must reserve capacity before the lock or
come from a fixed pool, so the push cannot allocate. Donor: NetBSD
`sys/kern/kern_sleepq.c` `sleepq_enqueue` requires the sleep queue locked and the interlock
released, and `sleepq_wake` removes waiters under a spin interlock and exits it without calling
a blocking function. No donor runs a blocking mutex acquisition under the sleep-queue spinlock.

### F2. `REAP` push from the scheduling/IRQ path — 2 sites

`kernel/src/sched/mod.rs:1067` `push_reap` takes `REAP` (a plain `SpinMutex`, interrupts not
masked) and does `Vec::push`. It is called from `kernel/src/sched/mod.rs:473` inside
`schedule_locked` while the per-CPU `CPUS` slot is held; `schedule_locked` is reached from
`timer_return`/`irq_return` (IRQ context). It is also called from
`kernel/src/sched/mod.rs:997` (`exit_thread`), from `kernel/src/sched/mod.rs:1074`
(`reap_thread`) and from the syscall rollback paths `kernel/src/syscall/thread.rs:122`, `:136`,
`:142`, all with interrupts enabled. A timer IRQ that arrives during the process-context
`Vec::push` can re-enter `schedule_locked`, reach `push_reap`, and spin on `REAP` with
interrupts masked; the interrupted context cannot resume. The `Vec::push` also takes `HEAP`,
a blocking mutex, in the IRQ path. `kernel/src/sched/mod.rs:1086` `reap_dead` shares the same
lock; it drains the queue and frees stacks after the switch.

**Repair (S83/Stuart, scheduler):** make `REAP` an `IrqSpinLock` (masks interrupts, no
allocation) and preallocate a fixed-capacity ring so `push_reap` never touches `HEAP`; or move
the terminal-thread `push_reap` out of `schedule_locked` to a process-context drain. Donor:
NetBSD spin mutexes are taken at IPL and never allocate/sleep under the lock
(`sys/kern/kern_mutex.c` `mutex_vector_enter`, `MUTEX_ASSERT(mtx, !cpu_intr_p())` at `:520`).

### F3. `SHARED_FRAMES` blocking mutex under `sync::critical` in the page-fault COW path — 3 sites

`kernel/src/memory/frame.rs:352` `share_frame`, `:362` `is_shared`, `:370` `release_shared`
take a blocking `Mutex`. `kernel/src/process/mod.rs:1952` `resolve_page_fault` takes
`address_space.lock()`, then `sync::critical()` (`:1972`), then calls
`address_space.handle_fault`; `kernel/src/memory/uspace.rs:1111` calls `frame::is_shared` and
`:1149` calls `frame::release_shared` inside that critical section. A contended
`SHARED_FRAMES` `Mutex::lock` can context-switch with interrupts masked and the address-space
mutex held. That is the lock-in-interrupt/blocking-under-IRQ-off class.

**Repair (S83/Stuart, memory):** make the shared-frame decision lock-free (atomic per-frame
refcount/`PG_SHARED` bit) or move the `SHARED_FRAMES` check/update before `critical()` and
retry the fault if the frame changed. A blocking mutex must not be taken while `critical()` is
live. Donor: NetBSD `sys/uvm/uvm_fault.c` `uvmfault_amapcopy` unlocks the maps
(`uvmfault_unlockmaps`) before `uvm_wait`; the COW path never sleeps under the map lock.

### F4. `SERIAL` normal print in exception/fault context — 2 production sites

`kernel/src/log.rs:130` `_print` and `:136` `_print_line` spin on `SERIAL` with interrupts
enabled. The x86 exception handler correctly uses `kprintln_emergency!`
(`kernel/src/arch/x86_64/exception.rs:233`-`:341`), and `SpinMutex::try_lock` is used by
`_print_line_emergency` at `kernel/src/log.rs:148`. However:

* aarch64 has **no** `try_lock`/emergency variant: `kernel/src/arch/aarch64/vectors.rs:461`,
  `:466`, `:477`, `:492`, `:500` call `kprintln!` from `terminate_fault`/`fatal`/`print_gprs`;
* the x86 default fault action at `kernel/src/arch/x86_64/syscall.rs:340` calls `kprintln!`, not
  the emergency macro;
* an aarch64 IRQ/fault handler that interrupts a CPU holding `SERIAL` spins on that CPU's
  process-context lock while the interrupted context cannot run.

**Repair (owner: Stuart/S83 console/arch; S76 for aarch64):** port `SpinMutex::try_lock` and
`_print_line_emergency` to aarch64 (remove the `cfg(target_arch = "x86_64")` guard), and use
`kprintln_emergency!` in every vector/fault path on both architectures. The console lock must
have a non-blocking fallback for interrupt context. Donor: NetBSD
`sys/kern/subr_prf.c` `kprintf_init`/`kprintf_lock` (`:146`-`:174`) uses an IPL_HIGH spin
mutex, and `sys/dev/ic/com.c` takes `sc_lock` with `mutex_spin_enter` at IPL_HIGH (`:303`-`:314`).

## 5. Needs-proof sites

Each row names the invariant/test that would settle it. None is claimed safe. All 69 inventory
rows are cited here.

### 5.1 Memory and page-fault allocator (10)

| Sites | Lock / context | Proof or repair | Owner |
|---|---|---|---|
| `kernel/src/memory/frame.rs:165`, `:253`, `:265`, `:277`, `:306`, `:320`, `:385`, `:417`, `:447` | `ALLOCATOR` in init, `set_percpu_enabled`, `alloc_frame`, `alloc_contiguous`, `free_frame`, `stats`, `cpu_stats`, `free_range` | Prove no holder takes a page fault, allocates from `HEAP`, sends an IPI, or calls `flush_range`; `alloc_zeroed_frame` must keep the zeroing outside the lock. Add a concurrent fault+alloc test. | S83/Stuart (memory) |
| `kernel/src/process/mod.rs:1967` | blocking `address_space` in `resolve_page_fault` | Page faults may block, but the pager path must release the address-space lock before sleeping and no IRQ-off section may take it. Prove the `critical()` at `1972` covers only `handle_fault`. | S83/Stuart (memory) |

### 5.2 Scheduler slot lock (1 physical site + 23 wrapper sites)

`kernel/src/sched/mod.rs:215` is the only acquisition of `CPUS`; every logical user goes through
`lock_cpu`. The wrapper sites are `kernel/src/sched/mod.rs:188`, `:213`, `:223`, `:241`, `:269`,
`:317`, `:447`, `:524`, `:537`, `:560`, `:609`, `:614`, `:637`, `:696`, `:723`, `:767`, `:793`,
`:840`, `:878`, `:940`, `:958`, `:1001`, `:1109`.
Invariant: a **local** slot is only taken while `sync::critical()` is live (or during
single-CPU/boot init at `:188`/`:223`/`:241`); a **remote** slot may be taken with interrupts
enabled (`:614` `set_need_resched` from `request_reschedule_all`, `:878`/`:940`/`:958` wake/steal)
but the holder never waits for that CPU's IPI/timer and releases before `send_reschedule`.
Proof/test: a debug assertion or host model for the critical-section invariant, plus a
timer/preempt/cross-CPU wake test. Owner: S83/Stuart (scheduler).

### 5.3 Signal scratch (8)

`kernel/src/arch/x86_64/syscall.rs:250`, `:290`, `:301`, `:322` and
`kernel/src/arch/aarch64/vectors.rs:356`, `:411`, `:422`, `:443` take `SIGNAL_CONTEXT` around
`signal_commit`/`sigreturn`; those call `copy_to_user`/`copy_from_user`, which can fault. The
proof is that the signal frame pages are resident/pre-faulted so the copy cannot fault, or the
copy moves outside the lock. Add a fault-injection test for the signal frame. Owner: S83
(arch/signal).

### 5.4 Allocating `IrqSpinLock` sections (7)

| Sites | Why needs proof | Owner |
|---|---|---|
| `kernel/src/event/mod.rs:159` | `register_queue` does `retain`/`push` under `QUEUES`; prove capacity or reserve before the lock | S83 (event) |
| `kernel/src/sync.rs:358` | `register_thread` inserts into a `BTreeMap` under `TURNSTILES` | S83 (sync/turnstile) |
| `kernel/src/sync.rs:368` | `donation_begin_call` registers/allocates under `TURNSTILES` | S83 |
| `kernel/src/sync.rs:459` | `Mutex::lock` graph `acquire` can allocate under `TURNSTILES` | S83 |
| `kernel/src/sync.rs:491` | `Mutex::unlock` `wake_one` path under `TURNSTILES` — prove allocation-free | S83 |
| `kernel/src/sync.rs:565` | `lockstat` collects a `Vec` under `TURNSTILES` | S83 |
| `kernel/src/time.rs:175` | `cyclic_add` does `Vec::push` under `CYCLICS`; reserve or move the allocation out | S83 (time) |

The repair pattern for all seven is the same donor rule as F2: no allocator-backed container
growth under an IRQ-mask lock. NetBSD spin mutexes at IPL do not allocate; containers are
preallocated or use a non-blocking pool.

### 5.5 Heap, stack cache, VT-d and driver locks (43)

| Sites | Lock / context | Proof or repair | Owner |
|---|---|---|---|
| `kernel/src/memory/heap.rs:57`, `:66`, `:76`, `:136`, `:156` | `HEAP` blocking mutex | Prove no IRQ/exception path allocates; F1/F2 currently violate this. Keep the heap out of every IRQ-mask section. | S83/Stuart (memory) |
| `kernel/src/sched/thread.rs:530`, `:555`, `:568` | `STACK_CACHE` blocking mutex | Process-context only; prove `reap_dead` never runs in IRQ context after F2 is fixed. | S83/Stuart (scheduler) |
| `kernel/src/arch/x86_64/vtd.rs:110`, `:115`, `:133`, `:138`, `:148` | `iova` | VT-d fault worker is process context; add a concurrent map/unmap + fault-worker test. | S83/Stuart (driver) |
| `kernel/src/arch/x86_64/vtd.rs:159`, `:213` | `tables` | same VT-d invariant/test. | S83/Stuart |
| `kernel/src/arch/x86_64/vtd.rs:274`, `:279`, `:309` | `UNIT` | same VT-d invariant/test. | S83/Stuart |
| `kernel/src/arch/x86_64/vtd.rs:465`, `:487` | `by_sid` | same VT-d invariant/test. | S83/Stuart |
| `kernel/src/arch/x86_64/vtd.rs:466`, `:486`, `:611` | `domains` | same VT-d invariant/test. | S83/Stuart |
| `kernel/src/arch/x86_64/vtd.rs:622` | `context_tables` | same VT-d invariant/test. | S83/Stuart |
| `kernel/src/arch/x86_64/vtd.rs:633` | `table_frames` | same VT-d invariant/test. | S83/Stuart |
| `kernel/src/driver/mod.rs:202`, `:243`, `:338`, `:343`, `:348` | `BLOBS` | Driver init/DMA paths are process context; add a concurrent IRQ/DMA test or document the init-only invariant. | S83/Stuart (driver) |
| `kernel/src/driver/mod.rs:217`, `:419` | `PLATFORM_OWNED` | same driver invariant/test. | S83/Stuart |
| `kernel/src/driver/mod.rs:236`, `:491`, `:494` | `DMA_DOMAIN` | same driver invariant/test. | S83/Stuart |
| `kernel/src/driver/mod.rs:315`, `:465` | `DEVICE_HINTS` | same driver invariant/test. | S83/Stuart |
| `kernel/src/dtrace/mod.rs:87`, `:103`, `:118`, `:170` | `WAKE_EVENT` | Probe wake path must be process context or use an IRQ-safe publication; add a probe/fault test. | S83/Stuart (dtrace) |
| `kernel/src/dtrace/mod.rs:128`, `:165` | `CONSUMER` | same dtrace invariant/test. | S83/Stuart |

## 6. Prior S85b/S113/S83 fixes re-verified

| Prior site | Current state | Evidence |
|---|---|---|
| `kernel/src/smp/shootdown.rs` global `IrqSpinLock` held across the TLB ACK wait | fixed: `ActiveGuard` (`:54`-`:89`) publishes `ACTIVE` under an IF mask, waits with interrupts enabled (`:150`-`:184`), restores the caller IF | source + `sh tests/smp/memory/run.sh tcg --iterations 1` |
| `kernel/src/smp/call.rs` probe lock wait | fixed: `ActiveGuard` (`:35`-`:70`), interrupts enabled during the ACK wait | source + SMP probe selftest |
| `kernel/src/sched/smp_shim.rs` aarch64 TLB wait | fixed: `ActiveGuard` (`:239`-`:267`), DAIF restored on drop, interrupts enabled during the SGI wait | source |
| `kernel/src/log.rs` log lock in interrupt | x86 fixed: `try_lock` at `:55`, emergency path at `:148`, exception users at `kernel/src/arch/x86_64/exception.rs:233`-`:341`. **aarch64 still unsafe** (F4). | source + F4 |

No current `IrqSpinLock` is held across a TLB/probe acknowledgement wait. The remaining unsafe
sites are the wait-queue condition class (F1), `REAP` (F2), `SHARED_FRAMES` (F3), and the
aarch64/x86 fault-print paths (F4).

## 7. Donor reconciliation

The donor revisions are `netbsd 36eddebcc36cb0775aa2962005b9c05419a6ff01` and `freebsd
133344405f532bd54ff3629a77d4c4e062f01fdb`, from `ravenbsd-resources/donors.lock`. This is an
audit; **no donor code is copied**.

| Topic | Donor (permissive, exact file/function) | Licence | Raven rule |
|---|---|---|---|
| Shootdown active request / ack wait | NetBSD `sys/arch/x86/x86/x86_tlb.c` (RCS 1.22), `sys/arch/x86/x86/ipi.c` (1.30), `sys/kern/subr_xcall.c` (1.40); FreeBSD `sys/amd64/amd64/mp_machdep.c` `smp_targeted_tlb_shootdown_native`, `sys/x86/x86/mp_x86.c`, `sys/kern/subr_smp.c` | NetBSD BSD-2; FreeBSD `mp_machdep.c`/`subr_smp.c` BSD-2, `mp_x86.c` 2-clause+no-endorsement; `sys/amd64/amd64/pmap.c` is BSD-4 and is not copied | Waiter for an IPI request must be able to take the IPI; active request + IF-enabled wait; S83 implemented this. |
| Wait queue / sleep queue | NetBSD `sys/kern/kern_sleepq.c` `sleepq_enqueue`, `sleepq_wake`; `sys/kern/kern_condvar.c`; `sys/kern/kern_mutex.c` `mutex_vector_enter` | BSD-2 | The queue lock never runs a blocking condition. `sleepq_wake` asserts the spin interlock and exits it without a blocking call. F1 repair. |
| Blocking mutex in interrupt context | NetBSD `sys/kern/kern_mutex.c` `mutex_vector_enter`, `MUTEX_ASSERT(mtx, !cpu_intr_p())` at `:520` | BSD-2 | A blocking `Mutex` is illegal in interrupt context; F1/F3 must not hold one under an IRQ-mask/IRQ-off section. |
| Page-fault COW allocator | NetBSD `sys/uvm/uvm_fault.c` `uvmfault_amapcopy` (unlock maps before `uvm_wait`), `sys/uvm/uvm_page.c` `uvm_pagealloc_strat` | BSD-2 | Do not sleep/allocate under the address-space or a spin lock; check, unlock, wait, retry. F3 repair. |
| Console in exception context | NetBSD `sys/kern/subr_prf.c` `kprintf_init`/`kprintf_lock` (`:146`-`:174`), `sys/dev/ic/com.c` `sc_lock` (`:303`-`:314`) | BSD-2 | The console lock is an IPL_HIGH spin mutex; an interrupt-context writer must not wait on a process-context lock. F4 repair. |
| Generic cross-call | NetBSD `sys/kern/subr_xcall.c`; FreeBSD `sys/kern/subr_smp.c` | BSD-2 | Background only; x86 TLB uses direct LAPIC IPIs. |
| Illumos xcall/hat | Illumos `usr/src/uts/i86pc/os/x_call.c`, `usr/src/uts/i86pc/vm/hat_i86.c` | CDDL | Behaviour read only, clean-room; no code copied. |

## 8. Owner and repair summary

| Finding | Sites | Owner | Repair | Donor rule |
|---|---|---|---|---|
| F1 wait-queue blocking condition | 24 | S83/Stuart (scheduler/IPC) | generation/re-check API; reserve the waiter `Vec`; no blocking mutex under `waiters` | NetBSD `sleepq_enqueue`/`sleepq_wake` |
| F2 `REAP` IRQ-path push | 2 | S83/Stuart (scheduler) | `IrqSpinLock` + fixed ring, or process-context drain | NetBSD IPL spin mutex, no allocation |
| F3 `SHARED_FRAMES` under `critical()` | 3 | S83/Stuart (memory) | atomic per-frame COW state or check/retry before `critical()` | NetBSD `uvmfault_amapcopy` |
| F4 `SERIAL` in fault/exception print | 2 (+ arch call sites) | Stuart/S83 (console), S76 (aarch64) | port `try_lock`/emergency to aarch64; use emergency in `terminate_fault` | NetBSD `kprintf_lock` |
| `ALLOCATOR`/`CPUS`/`SIGNAL_CONTEXT`/VT-d/driver | 69 | S83/Stuart per table | listed invariant/test | NetBSD IPL/uvm rules |

## 9. Regression and gate

The static regression is `tests/smp/irq-lock-audit/check.py`:

```sh
python3 tests/smp/irq-lock-audit/check.py --check
```

It verifies (a) every one of the 66 lock declarations is in this document, (b) every one of the
479 call sites is in `inventory.tsv` with the exact file/line/receiver/function/call, (c) every
`NEEDS-PROOF`/`UNSAFE` row is cited here, and (d) every `lock_cpu(`, `enqueue_if(`,
`push_reap(` and `reap_thread(` wrapper site is cited. A new lock or a shifted line fails until
the inventory is regenerated and reviewed.

The negative fixture in `tests/smp/irq-lock-audit/run.sh` builds a temporary fake tree with an
undocumented `IrqSpinLock` and a new `.lock()` site; `--check` must reject both, and accept the
unchanged fixture. The fixture never edits `kernel/src`.

`SAFE` in this document means **process-context only**: the lock is a blocking `Mutex` or a leaf
spin lock, and the §2 vector-entry walk found no edge from an IRQ/exception handler to it. It is
not a claim that the lock is contention-free or free of priority inversion. The checker catches a
new lock or call site, but it cannot see a new **call edge** from a vector handler; the §2 table
must be re-walked whenever a new IRQ callback or driver ISR is added.

The runtime regression for the fixed TLB class remains `tests/smp/memory` (the S101 four-way
concurrent unmap barrier; every `smp: tlb shootdown timeout` line is forbidden). No existing TLB
or memory check was weakened.

## 10. `crates/raven-vmem/**`

`rg -n "IrqSpinLock|SpinLock|SpinMutex|Mutex|\\.lock\\(\\)|try_lock" crates/raven-vmem/src`
returns no lock use. There is no vmem fix in this ticket; the TLB protocol is owned by
`kernel/src/smp/**` (S83) and the aarch64 path by S76/dave.

## 11. Acceptance

```sh
python3 tests/smp/irq-lock-audit/check.py --check
sh tests/smp/irq-lock-audit/run.sh
cargo nextest run -p raven-vmem
sh tests/smp/memory/run.sh tcg --iterations 1
make check-fast
```

The first four are the audit/regression gates; `make check-fast` is the ticket gate. Results are
recorded in `tickets/mark/reports/S102.md`. The audit does not claim a kernel fix for F1-F4; those
are handed to S83/Stuart and the named owners.

## Appendix A. Declaration citation index

Every lock declaration scanned by `check.py` is listed here with its exact source line. The
families are `irq-spinlock`, `spin-mutex` (the kernel `log::SpinMutex`), `external-spin`
(`spin::Mutex`) and `blocking-mutex` (the kernel turnstile `Mutex`).

- `kernel/src/arch/aarch64/vectors.rs:332` `SIGNAL_CONTEXT` (external-spin)
- `kernel/src/arch/x86_64/ioapic.rs:38` `IOAPICS` (irq-spinlock)
- `kernel/src/arch/x86_64/syscall.rs:189` `SIGNAL_CONTEXT` (external-spin)
- `kernel/src/arch/x86_64/vtd.rs:98` `iova` (blocking-mutex)
- `kernel/src/arch/x86_64/vtd.rs:99` `tables` (blocking-mutex)
- `kernel/src/arch/x86_64/vtd.rs:263` `context_tables` (blocking-mutex)
- `kernel/src/arch/x86_64/vtd.rs:264` `domains` (blocking-mutex)
- `kernel/src/arch/x86_64/vtd.rs:265` `by_sid` (blocking-mutex)
- `kernel/src/arch/x86_64/vtd.rs:266` `table_frames` (blocking-mutex)
- `kernel/src/arch/x86_64/vtd.rs:270` `UNIT` (blocking-mutex)
- `kernel/src/driver/mod.rs:79` `DMA_DOMAIN` (blocking-mutex)
- `kernel/src/driver/mod.rs:88` `DEVICE_HINTS` (blocking-mutex)
- `kernel/src/driver/mod.rs:184` `BLOBS` (blocking-mutex)
- `kernel/src/driver/mod.rs:252` `PLATFORM_OWNED` (blocking-mutex)
- `kernel/src/dtrace/mod.rs:40` `WAKE_EVENT` (blocking-mutex)
- `kernel/src/dtrace/mod.rs:42` `CONSUMER` (blocking-mutex)
- `kernel/src/event/mod.rs:29` `state` (blocking-mutex)
- `kernel/src/event/mod.rs:155` `QUEUES` (irq-spinlock)
- `kernel/src/ipc/mod.rs:165` `packet` (blocking-mutex)
- `kernel/src/ipc/mod.rs:167` `buffers` (blocking-mutex)
- `kernel/src/ipc/mod.rs:169` `response` (blocking-mutex)
- `kernel/src/ipc/registry.rs:18` `REGISTRY` (blocking-mutex)
- `kernel/src/ipc/router.rs:72` `CLOSE_NOTICES` (blocking-mutex)
- `kernel/src/log.rs:95` `SERIAL` (spin-mutex)
- `kernel/src/memory/frame.rs:134` `ALLOCATOR` (spin-mutex)
- `kernel/src/memory/frame.rs:143` `SHARED_FRAMES` (blocking-mutex)
- `kernel/src/memory/heap.rs:47` `HEAP` (blocking-mutex)
- `kernel/src/memory/mmio.rs:34` `CURSOR` (blocking-mutex)
- `kernel/src/process/duplex.rs:287` `state` (blocking-mutex)
- `kernel/src/process/handle.rs:58` `state` (blocking-mutex)
- `kernel/src/process/handle.rs:442` `LINEAGE` (blocking-mutex)
- `kernel/src/process/mod.rs:80` `PID_REGISTRY` (blocking-mutex)
- `kernel/src/process/mod.rs:86` `ZOMBIES` (blocking-mutex)
- `kernel/src/process/mod.rs:236` `ids` (blocking-mutex)
- `kernel/src/process/mod.rs:240` `command` (blocking-mutex)
- `kernel/src/process/mod.rs:244` `ctty` (blocking-mutex)
- `kernel/src/process/mod.rs:246` `address_space` (blocking-mutex)
- `kernel/src/process/mod.rs:248` `handles` (blocking-mutex)
- `kernel/src/process/mod.rs:263` `state` (blocking-mutex)
- `kernel/src/process/mod.rs:267` `namespace` (blocking-mutex)
- `kernel/src/process/mod.rs:269` `root` (blocking-mutex)
- `kernel/src/process/mod.rs:271` `schemes` (blocking-mutex)
- `kernel/src/process/mod.rs:273` `scheme_queue` (blocking-mutex)
- `kernel/src/process/mod.rs:277` `active_requests` (blocking-mutex)
- `kernel/src/process/mod.rs:279` `client_requests` (blocking-mutex)
- `kernel/src/process/mod.rs:281` `capture_slots` (blocking-mutex)
- `kernel/src/process/mod.rs:283` `personality` (blocking-mutex)
- `kernel/src/process/mod.rs:285` `pending_signals` (blocking-mutex)
- `kernel/src/process/mod.rs:287` `pending_exec` (blocking-mutex)
- `kernel/src/process/mod.rs:301` `limits` (blocking-mutex)
- `kernel/src/process/pipe.rs:160` `state` (blocking-mutex)
- `kernel/src/process/robust.rs:36` `LISTS` (blocking-mutex)
- `kernel/src/process/tty.rs:50` `REGISTRY` (blocking-mutex)
- `kernel/src/sched/mod.rs:119` `CPUS` (spin-mutex)
- `kernel/src/sched/mod.rs:1064` `REAP` (spin-mutex)
- `kernel/src/sched/selftest.rs:38` `COUNTER_LOCK` (irq-spinlock)
- `kernel/src/sched/thread.rs:519` `STACK_CACHE` (blocking-mutex)
- `kernel/src/sched/wait.rs:38` `waiters` (irq-spinlock)
- `kernel/src/sync.rs:299` `TURNSTILES` (irq-spinlock)
- `kernel/src/sync.rs:580` `M` (blocking-mutex, test)
- `kernel/src/sync.rs:650` `M` (blocking-mutex, test)
- `kernel/src/sync.rs:794` `M` (blocking-mutex, test)
- `kernel/src/sync.rs:795` `CLIENT_THREAD` (irq-spinlock, test)
- `kernel/src/syscall/record_lock.rs:38` `TABLE` (blocking-mutex)
- `kernel/src/time.rs:167` `CYCLICS` (irq-spinlock)
- `kernel/src/time.rs:438` `ENTROPY_POOL` (irq-spinlock)

## Appendix B. Wrapper citation index

`check.py` also requires every wrapper call site to be cited. The `lock_cpu` sites are
`kernel/src/sched/mod.rs:188`, `kernel/src/sched/mod.rs:213` (definition),
`kernel/src/sched/mod.rs:223`, `kernel/src/sched/mod.rs:241`, `kernel/src/sched/mod.rs:269`,
`kernel/src/sched/mod.rs:317`, `kernel/src/sched/mod.rs:447`, `kernel/src/sched/mod.rs:524`,
`kernel/src/sched/mod.rs:537`, `kernel/src/sched/mod.rs:560`, `kernel/src/sched/mod.rs:609`,
`kernel/src/sched/mod.rs:614`, `kernel/src/sched/mod.rs:637`, `kernel/src/sched/mod.rs:696`,
`kernel/src/sched/mod.rs:723`, `kernel/src/sched/mod.rs:767`, `kernel/src/sched/mod.rs:793`,
`kernel/src/sched/mod.rs:840`, `kernel/src/sched/mod.rs:878`, `kernel/src/sched/mod.rs:940`,
`kernel/src/sched/mod.rs:958`, `kernel/src/sched/mod.rs:1001`, `kernel/src/sched/mod.rs:1109`.

The `enqueue_if` sites are `kernel/src/sched/wait.rs:53` (definition),
`kernel/src/syscall/event.rs:236`, `kernel/src/syscall/process_ctl.rs:950`,
`kernel/src/syscall/process_ctl.rs:1094`, `kernel/src/syscall/process_ctl.rs:1242`,
`kernel/src/syscall/debug_read.rs:65`, `kernel/src/syscall/record_lock.rs:347`,
`kernel/src/syscall/linux.rs:294`, `kernel/src/syscall/futex.rs:102`,
`kernel/src/process/pipe.rs:381`, `kernel/src/process/duplex.rs:988`,
`kernel/src/memory/pager.rs:57`, `kernel/src/ipc/router.rs:655`,
`kernel/src/ipc/router.rs:1101`, `kernel/src/driver/irq.rs:532`.

The reap sites are `kernel/src/sched/mod.rs:473`, `kernel/src/sched/mod.rs:997`,
`kernel/src/sched/mod.rs:1066` (definition of `push_reap`),
`kernel/src/sched/mod.rs:1073` (definition of `reap_thread`),
`kernel/src/sched/mod.rs:1074`, and the rollback calls in
`kernel/src/syscall/thread.rs:122`, `kernel/src/syscall/thread.rs:136`,
`kernel/src/syscall/thread.rs:142`.

## Appendix C. Unsafe and needs-proof citation index

Every inventory row classified `UNSAFE` or `NEEDS-PROOF` is repeated here with its exact
file/line, receiver and function. This is the checklist the regression checker enforces.

- `kernel/src/arch/aarch64/vectors.rs:356` `SIGNAL_CONTEXT` `sigreturn_frame` — NEEDS-PROOF
- `kernel/src/arch/aarch64/vectors.rs:411` `SIGNAL_CONTEXT` `deliver_signal` — NEEDS-PROOF
- `kernel/src/arch/aarch64/vectors.rs:422` `SIGNAL_CONTEXT` `deliver_signal` — NEEDS-PROOF
- `kernel/src/arch/aarch64/vectors.rs:443` `SIGNAL_CONTEXT` `deliver_fault` — NEEDS-PROOF
- `kernel/src/arch/x86_64/syscall.rs:250` `SIGNAL_CONTEXT` `sigreturn_frame` — NEEDS-PROOF
- `kernel/src/arch/x86_64/syscall.rs:290` `SIGNAL_CONTEXT` `deliver_signal` — NEEDS-PROOF
- `kernel/src/arch/x86_64/syscall.rs:301` `SIGNAL_CONTEXT` `deliver_signal` — NEEDS-PROOF
- `kernel/src/arch/x86_64/syscall.rs:322` `SIGNAL_CONTEXT` `deliver_fault` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:110` `iova` `iova_start` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:115` `iova` `iova_end` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:133` `iova` `map` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:138` `iova` `map` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:148` `iova` `unmap` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:159` `tables` `map_pages` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:213` `tables` `unmap_pages` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:274` `UNIT` `available` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:279` `UNIT` `unit` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:309` `UNIT` `init` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:465` `by_sid` `domain_for_bdf` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:466` `domains` `domain_for_bdf` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:486` `domains` `domain_for_bdf` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:487` `by_sid` `domain_for_bdf` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:611` `domains` `allocate_domain_id` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:622` `context_tables` `install_context` — NEEDS-PROOF
- `kernel/src/arch/x86_64/vtd.rs:633` `table_frames` `install_context` — NEEDS-PROOF
- `kernel/src/driver/mod.rs:202` `BLOBS` `init` — NEEDS-PROOF
- `kernel/src/driver/mod.rs:217` `PLATFORM_OWNED` `init_aarch64` — NEEDS-PROOF
- `kernel/src/driver/mod.rs:236` `DMA_DOMAIN` `init_aarch64` — NEEDS-PROOF
- `kernel/src/driver/mod.rs:243` `BLOBS` `init_aarch64` — NEEDS-PROOF
- `kernel/src/driver/mod.rs:315` `DEVICE_HINTS` `create_mmio` — NEEDS-PROOF
- `kernel/src/driver/mod.rs:338` `BLOBS` `boot_blob` — NEEDS-PROOF
- `kernel/src/driver/mod.rs:343` `BLOBS` `boot_blob` — NEEDS-PROOF
- `kernel/src/driver/mod.rs:348` `BLOBS` `boot_blob` — NEEDS-PROOF
- `kernel/src/driver/mod.rs:419` `PLATFORM_OWNED` `refuse_kernel_owned` — NEEDS-PROOF
- `kernel/src/driver/mod.rs:465` `DEVICE_HINTS` `dma_alloc` — NEEDS-PROOF
- `kernel/src/driver/mod.rs:491` `DMA_DOMAIN` `dma_alloc` — NEEDS-PROOF
- `kernel/src/driver/mod.rs:494` `DMA_DOMAIN` `dma_alloc` — NEEDS-PROOF
- `kernel/src/dtrace/mod.rs:87` `WAKE_EVENT` `wake_consumer` — NEEDS-PROOF
- `kernel/src/dtrace/mod.rs:103` `WAKE_EVENT` `enable` — NEEDS-PROOF
- `kernel/src/dtrace/mod.rs:118` `WAKE_EVENT` `disable` — NEEDS-PROOF
- `kernel/src/dtrace/mod.rs:128` `CONSUMER` `drain` — NEEDS-PROOF
- `kernel/src/dtrace/mod.rs:165` `CONSUMER` `clear` — NEEDS-PROOF
- `kernel/src/dtrace/mod.rs:170` `WAKE_EVENT` `clear` — NEEDS-PROOF
- `kernel/src/event/mod.rs:159` `QUEUES` `register_queue` — NEEDS-PROOF
- `kernel/src/log.rs:130` `SERIAL` `_print` — UNSAFE
- `kernel/src/log.rs:136` `SERIAL` `_print_line` — UNSAFE
- `kernel/src/memory/frame.rs:165` `ALLOCATOR` `init` — NEEDS-PROOF
- `kernel/src/memory/frame.rs:253` `ALLOCATOR` `init` — NEEDS-PROOF
- `kernel/src/memory/frame.rs:265` `ALLOCATOR` `set_percpu_enabled` — NEEDS-PROOF
- `kernel/src/memory/frame.rs:277` `ALLOCATOR` `alloc_frame` — NEEDS-PROOF
- `kernel/src/memory/frame.rs:306` `ALLOCATOR` `alloc_contiguous` — NEEDS-PROOF
- `kernel/src/memory/frame.rs:320` `ALLOCATOR` `free_frame` — NEEDS-PROOF
- `kernel/src/memory/frame.rs:352` `SHARED_FRAMES` `share_frame` — UNSAFE
- `kernel/src/memory/frame.rs:362` `SHARED_FRAMES` `is_shared` — UNSAFE
- `kernel/src/memory/frame.rs:370` `SHARED_FRAMES` `release_shared` — UNSAFE
- `kernel/src/memory/frame.rs:385` `ALLOCATOR` `stats` — NEEDS-PROOF
- `kernel/src/memory/frame.rs:417` `ALLOCATOR` `cpu_stats` — NEEDS-PROOF
- `kernel/src/memory/frame.rs:447` `ALLOCATOR` `free_range` — NEEDS-PROOF
- `kernel/src/memory/heap.rs:57` `HEAP` `init` — NEEDS-PROOF
- `kernel/src/memory/heap.rs:66` `HEAP` `init` — NEEDS-PROOF
- `kernel/src/memory/heap.rs:76` `HEAP` `stats` — NEEDS-PROOF
- `kernel/src/memory/heap.rs:136` `HEAP` `alloc` — NEEDS-PROOF
- `kernel/src/memory/heap.rs:156` `HEAP` `dealloc` — NEEDS-PROOF
- `kernel/src/process/duplex.rs:238` `state` `readiness` — UNSAFE
- `kernel/src/process/duplex.rs:563` `state` `read` — UNSAFE
- `kernel/src/process/duplex.rs:660` `state` `recv` — UNSAFE
- `kernel/src/process/duplex.rs:789` `state` `write_stream` — UNSAFE
- `kernel/src/process/duplex.rs:896` `state` `write_datagram` — UNSAFE
- `kernel/src/process/handle.rs:209` `state` `page_ready` — UNSAFE
- `kernel/src/process/mod.rs:391` `PID_REGISTRY` `lookup_pid` — UNSAFE
- `kernel/src/process/mod.rs:405` `PID_REGISTRY` `processes` — UNSAFE
- `kernel/src/process/mod.rs:733` `pending_signals` `has_pending_signal` — UNSAFE
- `kernel/src/process/mod.rs:1967` `address_space` `resolve_page_fault` — NEEDS-PROOF
- `kernel/src/process/pipe.rs:87` `state` `readiness` — UNSAFE
- `kernel/src/process/pipe.rs:234` `state` `read` — UNSAFE
- `kernel/src/process/pipe.rs:298` `state` `write` — UNSAFE
- `kernel/src/sched/mod.rs:215` `CPUS` `lock_cpu` — NEEDS-PROOF
- `kernel/src/sched/mod.rs:1067` `REAP` `push_reap` — UNSAFE
- `kernel/src/sched/mod.rs:1086` `REAP` `reap_dead` — UNSAFE
- `kernel/src/sched/thread.rs:530` `STACK_CACHE` `allocate_kernel_stack` — NEEDS-PROOF
- `kernel/src/sched/thread.rs:555` `STACK_CACHE` `free_kernel_stack` — NEEDS-PROOF
- `kernel/src/sched/thread.rs:568` `STACK_CACHE` `stack_cached` — NEEDS-PROOF
- `kernel/src/sched/wait.rs:61` `waiters` `enqueue_if` — UNSAFE
- `kernel/src/sync.rs:358` `TURNSTILES` `register_thread` — NEEDS-PROOF
- `kernel/src/sync.rs:368` `TURNSTILES` `donation_begin_call` — NEEDS-PROOF
- `kernel/src/sync.rs:459` `TURNSTILES` `lock` — NEEDS-PROOF
- `kernel/src/sync.rs:491` `TURNSTILES` `unlock` — NEEDS-PROOF
- `kernel/src/sync.rs:565` `TURNSTILES` `lockstat` — NEEDS-PROOF
- `kernel/src/syscall/event.rs:239` `state` `wait_once` — UNSAFE
- `kernel/src/syscall/event.rs:332` `handles` `readiness` — UNSAFE
- `kernel/src/syscall/event.rs:341` `state` `readiness` — UNSAFE
- `kernel/src/syscall/event.rs:349` `scheme_queue` `readiness` — UNSAFE
- `kernel/src/syscall/event.rs:356` `state` `readiness` — UNSAFE
- `kernel/src/syscall/process_ctl.rs:1000` `ids` `children_for` — UNSAFE
- `kernel/src/syscall/process_ctl.rs:1007` `ids` `children_for` — UNSAFE
- `kernel/src/syscall/process_ctl.rs:1014` `ids` `children_for` — UNSAFE
- `kernel/src/syscall/process_ctl.rs:1021` `ids` `children_for` — UNSAFE
- `kernel/src/syscall/process_ctl.rs:1029` `state` `peek_event` — UNSAFE
- `kernel/src/syscall/record_lock.rs:348` `TABLE` `record_lock` — UNSAFE
- `kernel/src/time.rs:175` `CYCLICS` `cyclic_add` — NEEDS-PROOF
