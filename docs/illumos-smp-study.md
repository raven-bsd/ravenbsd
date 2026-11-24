<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Illumos SMP, AP and TLB study (S100)

Status: **research / functional specification**. No kernel code changes.
Audience: S83 (KVM AP bring-up) and S85b (lost TLB acknowledgement); readable without opening
Illumos.

Method and clean-room rules:

* Illumos is **CDDL, design-only** (DR-28/DR-30, `process/CARL.md`). This document records
  *behaviour* and *invariants* from reading the sources. No Illumos code is copied, translated or
  closely paraphrased, and no Illumos code enters the tree.
* NetBSD is a permissive donor. Files are classified **COPY/PORT**, **STUDY** or **NO-COPY** from
  their own licence text; the classification is per file, not per project.
* Statements labelled **OBSERVED** are directly visible in the cited donor file and revision.
  Statements labelled **PROPOSED** or **UNVERIFIED** are Raven design hypotheses and are for S83
  and S85b to test. Nothing here claims a runtime result for Raven.

Donor revisions:

| Donor | Revision | Tree |
|---|---|---|
| illumos-gate | `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec` | `ravenbsd-resources/illumos-gate` (sparse; objects present in `.git`) |
| NetBSD | `36eddebcc36cb0775aa2962005b9c05419a6ff01` | `ravenbsd-resources/netbsd` |
| FreeBSD | `133344405f532bd54ff3629a77d4c4e062f01fdb` | `ravenbsd-resources/freebsd` (sparse; **no** `sys/x86/x86` or `sys/amd64/amd64`) |

All line numbers below are against those revisions.

---

## 1. Why this matters for Raven

Two independent suites fail today:

* KVM `-smp 2/4` resets before the APs report online.
* TCG `-smp 4` loses TLB shootdown acknowledgements:
  `tests/smp/memory/FINDING.md` documents 100 `smp: tlb shootdown timeout` lines per clean run
  while the user-level fixture itself reports `tlb_stale=0` and `smp-memory: ok`.
  S85f separately measured the aarch64 `smp-tlb` flake at 3/23 runs (10 %).

Both failures are control-plane failures: the CPUs are executing, but the bring-up/ack protocol
does not establish the state the rest of the kernel assumes. Illumos and NetBSD solved the same
class of problem; the sections below extract the parts Raven does not yet have.

---

## 2. Donor inventory and licence classification

### 2.1 Illumos (CDDL, STUDY only in this ticket)

| File | Lines | What it shows |
|---|---:|---|
| `usr/src/uts/i86pc/os/mp_startup.c` | 2183 | AP real-mode entry handshake, CPU state transitions, `CPU_READY` ordering, base SPL rule |
| `usr/src/uts/i86pc/os/mp_call.c` | 93 | pause/resume cross-call glue |
| `usr/src/uts/i86pc/os/mp_implfuncs.c` | 444 | CPU pause/start primitives |
| `usr/src/uts/i86pc/os/mp_machdep.c` | 1807 | CPU configure/unconfigure, online bookkeeping |
| `usr/src/uts/i86pc/os/x_call.c` | 708 | cross-call message state machine, priority path |
| `usr/src/uts/i86pc/sys/x_call.h` | 94 | message commands and per-CPU mailbox shape |
| `usr/src/uts/i86pc/vm/hat_i86.c` | 5006 | TLB shootdown target selection, idle delayed invalidation |
| `usr/src/uts/i86pc/vm/htable.c` | 2477 | page-table walk/invalidation call sites |
| `usr/src/uts/common/sys/cpuvar.h` | — | CPU flag semantics (`CPU_READY`, `CPU_OFFLINE`, ...) |

Classification for all Illumos files: **STUDY** (CDDL). None may be copied or translated.

### 2.2 NetBSD (per-file licence)

`$NetBSD$` revision is the exact file revision inside the donor checkout.

| File | `$NetBSD$` rev | Licence | Class |
|---|---|---|---|
| `sys/kern/subr_xcall.c` | 1.40 | 2-clause BSD (NetBSD Foundation) | **COPY/PORT** |
| `sys/sys/xcall.h` | 1.8 | 2-clause BSD | **COPY/PORT** |
| `sys/arch/x86/x86/ipi.c` | 1.30 | 2-clause BSD (NetBSD Foundation) | **COPY/PORT** |
| `sys/arch/x86/x86/x86_tlb.c` | 1.22 | 2-clause BSD (NetBSD Foundation) | **COPY/PORT** |
| `sys/arch/x86/x86/lapic.c` | 1.94 | 2-clause BSD (no advertising/endorsement clause) | **COPY/PORT** |
| `sys/arch/x86/x86/apic.c` | 1.10 | 2-clause BSD | **COPY/PORT** |
| `sys/arch/x86/x86/cpu_topology.c` | 1.21 | 2-clause BSD | **COPY/PORT** |
| `sys/arch/x86/include/cpu.h` | — | 3-clause BSD (endorsement clause) | **PORT** (keep notice; do not use names to endorse) |
| `sys/arch/x86/x86/cpu.c` | 1.214 | **4-clause** BSD (advertising + endorsement) | **NO-COPY** |
| `sys/arch/x86/x86/mpacpi.c` | 1.112 | **4-clause** BSD (Wasabi advertising + endorsement) | **NO-COPY** |
| `sys/arch/x86/x86/mp.c` | 1.7 | **4-clause** BSD (Wasabi) | **NO-COPY** |
| `sys/arch/x86/x86/pmap.c` | 1.429 | **4-clause** BSD (Wasabi) | **NO-COPY** |
| `sys/arch/amd64/amd64/mptramp.S` | 1.28 | **4-clause** BSD | **NO-COPY** |
| `sys/arch/amd64/amd64/locore.S` | 1.236 | **4-clause** BSD (Wasabi) | **NO-COPY** |

Consequence for S83/S85b:

* The **TLB/IPI/cross-call** mechanisms Raven needs most are in 2-clause files and are safe to
  port with the NetBSD notice: `x86_tlb.c`, `ipi.c`, `subr_xcall.c`, `xcall.h`, `lapic.c`,
  `apic.c`.
* The **AP trampoline and MADT enumeration** are 4-clause (`cpu.c`, `mpacpi.c`, `mp.c`,
  `mptramp.S`). Read them for behaviour, but implement from this specification, not by copying.
* FreeBSD is unusable as a donor today: the sparse checkout has `sys/x86/include`, `sys/x86/linux`
  etc., but no `sys/x86/x86` and no `sys/amd64/amd64`. Record this in the next donor update; do not
  invent FreeBSD file revisions.

---

## 3. Illumos: AP bring-up and CPU online state (OBSERVED)

### 3.1 CPU state flags and their meaning

`usr/src/uts/common/sys/cpuvar.h:341-353` defines the flags. The ones that matter here:

| Flag | Meaning (Illumos comment) |
|---|---|
| `CPU_RUNNING` | CPU is running |
| `CPU_READY` | **CPU ready for cross-calls** |
| `CPU_EXISTS` | CPU is configured |
| `CPU_ENABLE` | CPU enabled for interrupts |
| `CPU_QUIESCED` | CPU stays in the idle loop; will not run interrupt threads |
| `CPU_OFFLINE` | dispatcher must not run non-interrupt threads there |
| `CPU_POWEROFF` | CPU is powered off |

The comments at `cpuvar.h:336-339` state the powered-off combination: `CPU_QUIESCED |
CPU_OFFLINE | CPU_POWEROFF` set, and `CPU_RUNNING | CPU_READY | CPU_EXISTS | CPU_DISABLED |
CPU_ENABLE` clear. This is a real state machine, not a set of independent booleans.

Two invariants follow directly and are the core of the AP problem:

1. **`CPU_READY` means "safe to cross-call", not "the CPU executed its first instruction".**
   `x_call.c:470` and `x_call.c:495` filter cross-call targets on `CPU_READY`; a CPU without it is
   skipped rather than waited for.
2. **A CPU must not enter a path that can block on or issue a cross-call before `CPU_READY`.**
   `mp_startup.c:1694-1696` warns that `mp_startup_common()` must not call anything that can reach
   `hat_unload()` before `CPU_READY`, because `hat_unload()` issues TLB cross-calls.

### 3.2 Configure phase (control CPU, before the AP runs)

`mp_startup.c:330-488` (`mp_cpu_configure_common`), all under `cpu_lock`:

1. Build the AP's startup thread with `t_preempt = 1`, bound to the target CPU, and a fake call
   frame whose PC is `mp_startup_boot` / `mp_startup_hotplug` (`:346-359`).
2. Set `cpu_base_spl = ipltospl(LOCK_LEVEL)` (`:378`). The comment at `:368-377` is explicit:
   `cpu_base_spl` must stay at `LOCK_LEVEL` until `CPU_READY`, so a blocking operation in early
   AP code cannot drop SPL to 0 and deliver device interrupts too early.
3. Create the idle thread, bootstrap page-group data, allocate per-CPU GDT/LDT/IDT, copy the GDT
   and IDT from the running CPU (`:383-433`).
4. `xc_init_cpu(cp)` and `hat_cpu_online(cp)` (`:458-459`) allocate the cross-call mailboxes and
   register the CPU in the HAT sets **before** the CPU executes.
5. Publish `CPU_OFFLINE | CPU_QUIESCED | CPU_POWEROFF` (`:478`) and add the CPU to the unit list
   (`:485`). It is configured but not active.

### 3.3 Start phase (control CPU and AP handshake)

`mp_startup.c:1336-1358` implements two cpuset handshakes:

* `mp_startup_wait(set, cpuid)` spins on a volatile cpuset until the peer adds `cpuid`, then removes
  it.
* `mp_startup_signal(set, cpuid)` adds `cpuid` and spins until the peer removes it.

Because each side removes the bit, this is a **rendezvous**, not a flag: the control CPU cannot run
ahead and reuse state while the AP still believes it owns it. `mp_start_cpu_common()`
(`:1360-1460`) uses this repeatedly:

1. Start the AP, wait for the AP to signal the first rendezvous (`:1431`).
2. Do control-side work (microcode locate), signal the AP (`:1433`).
3. Wait again for the AP to finish CPUID probing (`:1451`), then run CPUPM/processor-group init,
   `cpu_set_state(cp)`, and signal (`:1452-1457`).

`start_cpu()` (`:1469-1515`) then waits outside `cpu_lock` until `cpu_ready_set` contains the CPU
(`:1507-1511`). **The control CPU does not treat an AP as usable until `cpu_ready_set` says so.**
`start_other_cpus()` (`:1531-1561`) puts the boot CPU in `cpu_ready_set` explicitly.

### 3.4 AP-side ordering (`mp_startup_common`, `:1699-1900`)

The AP enters with interrupts off and base SPL at `LOCK_LEVEL`. The order is:

1. CPUID prelude/identify and TSC sync rendezvous (`:1717-1743`).
2. Microcode, BASIC/EXTENDED/DYNAMIC/RESOLVE CPUID passes (`:1759-1763`, `:1852-1854`).
3. `ap_mlsetup`, PAT, `TSC_AUX`, syscall handlers (`:1771-1790`).
4. **`splx(ipltospl(LOCK_LEVEL)); sti();`** (`:1800-1801`). Interrupts are enabled, but only up to
   `LOCK_LEVEL`; the comment at `:1793-1798` says this blocks device interrupts that could reach
   the HAT and issue cross-calls before `CPU_READY`.
5. Clear `CPU_POWEROFF | CPU_QUIESCED` (`:1841`). The comment at `:1831-1840` explains that this
   must happen before anything that can context-switch: the idle thread loops forever while
   `CPU_QUIESCED` is set.
6. Publish **`CPU_RUNNING | CPU_READY | CPU_EXISTS`** (`:1863`).
7. Enable preemption (`:1877`), assert base SPL is still `LOCK_LEVEL`, then `set_base_spl()` to the
   real value (`:1879-1881`).
8. Final rendezvous with the control CPU, page-group/CMT startup (`:1889-1891`).
9. Only now, for a boot CPU: take `cpu_lock`, clear `CPU_OFFLINE`, `cpu_enable_intr(cp)`,
   `cpu_add_active(cp)` (`:1893-1898`).

**OBSERVED ordering rule:** interrupts up to `LOCK_LEVEL` → clear quiesce → publish `CPU_READY` →
raise base SPL to normal. `CPU_READY` is published *after* the CPU can take an interrupt but
*before* it is scheduled normally.

---

## 4. Illumos: cross-call mechanism (OBSERVED)

### 4.1 Data shape

`x_call.h:52-67`: each CPU has one `xc_data_t` (function + three arguments) and a queue of
small `xc_msg_t` messages (command, master, slave, next). Messages circulate between master and
slave queues; the function arguments live in the **master's** `xc_data`, not in the message.

`x_call.c:63-65`: every CPU has `xc_work_cnt`. The initiator increments it for each message it
must eventually transition to `FREE`; the target increments it for work in its mailbox.

### 4.2 State machine

`x_call.c:84-90` gives the canonical transitions (`->` in the slave, `=>` in the master):

```
FREE => ASYNC ->                       DONE => FREE
FREE => CALL  ->                       DONE => FREE
FREE => SYNC  -> WAITING => RELEASED -> DONE => FREE
```

`xc_serv()` (`:310-448`) implements it:

| Command | What the receiver does |
|---|---|
| `XC_MSG_ASYNC` | Read function/args from master, **return the message as `DONE` first**, then call the function. Bounds the master's free list. |
| `XC_MSG_SYNC` | Call the function, then send the message back as `WAITING`. |
| `XC_MSG_CALL` | Call the function, then fall through to `RELEASED`. |
| `XC_MSG_WAITING` | Master collects; when `num_waiting == xc_wait_cnt`, flips all to `RELEASED` and returns them to their slaves. |
| `XC_MSG_RELEASED` | Send `DONE` to master. |
| `XC_MSG_DONE` | Master returns the message to its own free list. |

A `FREE` message in a mailbox or an unknown command is a `panic()` (`:436-442`). This is a
closed protocol: every message has exactly one legal next state.

### 4.3 Initiation and target filtering (`xc_common`, `:454-542`)

1. If cross-calls are not initialised yet, run the function locally only if this CPU is
   `CPU_READY` (`:469-473`).
2. Raise SPL to `XC_HI_PIL` (`:476`). This prevents a lower-priority interrupt from issuing a
   nested cross-call on the same CPU.
3. Store the function/args in the **master's** `xc_data` (`:481-485`) and zero `xc_wait_cnt`
   (`:490`).
4. For each CPU in the request set: skip if not `CPU_READY` (`:495`); take a message from the
   master's free list, which panics if exhausted (`:501-503`); increment the master's and the
   target's work counters (`:514-523`); insert into the target mailbox.
5. **Send the IPI only when the target's work count transitions 0 → 1** (`cnt == 0`, `:526-528`).
   Otherwise the target already has an IPI pending and will drain the whole mailbox. This coalesces
   concurrent shootdowns and is the main defence against IPI storms.
6. Finally the master runs `xc_serv()` on its own mailbox (`:540`), so it participates in the work
   and can collect `WAITING`/`DONE` messages.

There is **no global lock and no timeout** in this path. Concurrent initiators to intersecting
CPU sets are explicitly allowed (`:72-76`); the only guarantee is that `xc_call()`/`xc_sync()`
do not return before every target has *invoked* the function. Order between concurrent calls is
unspecified.

### 4.4 Priority cross-calls (`xc_priority_common`, `:547-...`)

One high-priority message slot exists for panic/kmdb (`x_call.c:107-113`). It is deliberately
lock-free (`:108-110`). Before posting, the sender pauses up to 40,000 times waiting for the
target's priority bit to clear (`:562-575`); the comment admits this is a legacy iteration count
and is not time-based. Priority messages are checked in `xc_serv()` before normal mailbox work
(`:335-345`).

**OBSERVED rule:** normal cross-calls are best-effort/concurrent; the priority path is a single
global slot that tolerates a bounded wait, not a queue.

### 4.5 CPU offline interaction

`xc_fini_cpu()` (`:253-265`) asserts the CPU is not `CPU_READY`, its mailbox is empty and its work
count is zero, then frees the messages. `xc_flush_cpu()` (`:270-300`) explicitly **pauses all
working CPUs** around the check for in-flight work, because there is a race window between the
`CPU_READY` test and incrementing the work count in `xc_common()`. This confirms the invariant:
*the target set is only stable while the sender holds its SPL/preemption state and the target is
either `CPU_READY` or fully quiesced.*

---

## 5. Illumos: TLB shootdown (OBSERVED)

`hat_tlb_inval_range()` (`hat_i86.c:2522-2651`) is the reference path.

1. If `HAT_FREEING`, return (`:2535`). If `HAT_SHARED`, demap the whole user TLB because the
   address in the shared pagetable is unknown (`:2543-2546`).
2. If `panicstr` or single-CPU (`!flushes_require_xcalls`), do a local demap only (`:2551-2565`).
3. Disable preemption (`:2572`), so the CPU cannot migrate while the target set is built.
4. Choose the target set: `khat_cpuset` for the kernel HAT, `hat->hat_cpus` for a user HAT
   (`:2573-2576`). A user shootdown therefore goes only to CPUs that have this address space
   active, not to every CPU.
5. Walk the set word-at-a-time (`:2587-2609`). For each target:
   * if the target is `TLBIDLE_CPU_HALTED`, CAS its state to `TLBIDLE_CPU_HALTED |
     TLBIDLE_INVAL_ALL` and **do not send an IPI** (`:2596-2605`). The idle CPU flushes when it
     next wakes. This is the key "do not wait for a halted CPU" rule.
   * otherwise add it to `cpus_to_shootdown`.
6. If there are no remote targets, or the shooter is the only target, demap locally (`:2615-2629`).
7. Otherwise add the local CPU and issue one `xc_call()` with the demap function
   (`:2633-2647`).

**OBSERVED ack rule:** the shooter waits for each target to *invoke* the demap function through
the `SYNC` state machine. There is no timeout and no retry. A target that cannot run the function
is either excluded (`!CPU_READY`) or remembered for delayed invalidation (`TLBIDLE`), so the
protocol cannot "time out" in the Raven sense. The price is that a bug in target selection hangs
the system instead of logging a timeout — Illumos prefers a hard invariant to a soft recovery.

---

## 6. Illumos: barriers, priorities, memory ordering (OBSERVED)

* `x_call.c:67-70`: the implementation needs **no explicit `mfence`/`membar`**. Message passing
  uses `atomic_cas_ptr`, `atomic_cas_32` and `atomic_dec_32`; on x86 these carry `LOCK`, which is
  at least as strong as `mfence`. Ordering is therefore a property of the atomic operations, not
  of extra barriers sprinkled around the queue.
* `x_call.c:476`: the whole cross-call path runs at `XC_HI_PIL`, above device interrupts. This is
  both a priority rule and a re-entrancy rule: a device interrupt cannot start a nested cross-call
  on the same CPU.
* `x_call.c:306-308`: the sender is protected from CPU migration by a high-priority interrupt,
  disabled preemption or raised SPL. The algorithm assumes it does not move between building the
  target set and posting.
* `mp_startup.c:1793-1801`: interrupts are enabled at `LOCK_LEVEL`, not at 0, until `CPU_READY`.
  This blocks exactly the device-interrupt → HAT → cross-call path that would otherwise deadlock
  on an unready target.
* `hat_i86.c:2572`: preemption is disabled around target-set construction; without it a
  concurrent offline could invalidate the set between the walk and the send.
* Atomics in the message protocol also provide publication ordering; a target that observes the
  command also observes the master's `xc_data`.

---

## 7. Illumos: failure modes the design guards against

| Failure mode | Guard |
|---|---|
| Cross-call to a CPU that has not finished bring-up | `CPU_READY` filter (`x_call.c:470,495`) |
| AP takes a device interrupt and issues a HAT cross-call before it can serve one | base SPL held at `LOCK_LEVEL` until `CPU_READY` (`mp_startup.c:368-378,1793-1801`) |
| Idle/halted CPU never answers a TLB IPI | `TLBIDLE_CPU_HALTED` delayed-invalidation CAS (`hat_i86.c:2596-2605`) |
| IPI storm from N concurrent shootdowns | one IPI per target per work-count 0→1 transition (`x_call.c:526-534`) |
| Control CPU reuses AP state too early | paired cpuset rendezvous (`mp_startup.c:1336-1358`) |
| CPU offlined while messages are in flight | pause-all + work-count drain + assertions (`x_call.c:270-300`) |
| Nested cross-call from a device interrupt | run at `XC_HI_PIL` (`x_call.c:476`) |
| Panic/debugger needs a call while normal mailboxes are inconsistent | separate lock-free priority slot (`x_call.c:107-113`) |
| Sender migrates mid-protocol | SPL/preemption held (`x_call.c:306-308`) |
| Shared pagetable demap at an unknown address | demap all user TLBs (`hat_i86.c:2543-2546`) |

---

## 8. NetBSD: cross-call donor (OBSERVED, COPY/PORT)

`sys/kern/subr_xcall.c` (rev 1.40) and `sys/sys/xcall.h` (rev 1.8) provide a simpler model than
Illumos, split by priority:

* **Low priority** (`xc_lowpri`, `:427-476`): a per-CPU bound kthread drains a single global
  request. The sender takes `xc_lock`, waits for the previous request to drain, fills the global
  function/args, increments `xc_headp` once per target, sets `cpu_xcall_pending` on each target and
  signals its condvar. `xc_wait()` (`:395-419`) blocks on `xc_donep` reaching the ticket. This is
  heavyweight and may block; the header comment (`:56-70`) says it must not be used often.
* **High priority** (`XC_HIGHPRI`, `xcall.h:39-40`): runs in a softint at the requested IPL,
  expected to be short and non-blocking.
* Only CPUs with `SPCF_RUNNING` are targeted (`subr_xcall.c:455-461`). This is the NetBSD analogue
  of `CPU_READY` gating.
* Completion is a **ticket counter**, not per-target acknowledgements: one global `xc_donep`
  advances once per target invocation, and the waiter blocks until it passes its ticket. A missed
  invocation blocks forever; there is no timeout.

For Raven this is attractive as a **shape**, not as code to lift verbatim: per-CPU request state,
a monotonically increasing ticket, and "only running CPUs are targets". It confirms that "never
wait for a non-running CPU" is a design rule shared by both donors.

`sys/arch/x86/x86/ipi.c` (rev 1.30, 2-clause) shows the IPI side:

* a fixed `ipifunc[X86_NIPI]` dispatch table (`:82-93`), one entry per IPI reason (halt, ast,
  generic, synch FPU, MTRR, xcall, kpreempt);
* `x86_send_ipi()` and `x86_broadcast_ipi()` (`:100-149`) build the LAPIC destination shorthand and
  delivery mode;
* `x86_ipi_handler()` (`:153-...)` reads the pending bitmap and dispatches through the table;
* the xcall entry (`x86_ipi_xcall`, `:227-231`) is a thin hook into `subr_xcall.c`.

This is the cleanest donor for a *reason-coded* IPI gate: Raven currently has separate
`send_tlb`/`send_call`/`send_reschedule` paths; a table makes the reason explicit and makes it
possible to assert that every reason has a handler.

---

## 9. NetBSD: TLB shootdown donor (OBSERVED, COPY/PORT)

`sys/arch/x86/x86/x86_tlb.c` (rev 1.22, 2-clause) is the closest functional match to Raven's
problem.

* Each CPU owns a `pmap_tlb_packet_t` (`:84-113`), which batches up to 16 virtual addresses or a
  "flush all" marker. Control bits are overlaid in the low 12 bits of the first four entries.
* `pmap_tlb_shootdown()` (`:240-296`) runs at `splvm()`, appends the operation to the **current
  CPU's** packet, and merges the pmap's CPU set into `ci->ci_tlb_cpuset`. It does **not** send an
  IPI. Sending is deferred.
* `pmap_tlb_shootnow()` (`:360-...`) is called later with preemption already disabled. It publishes
  the packet into a mailbox on the initiator's kernel stack, then either sends one IPI per target
  (`x86_ipi(LAPIC_TLB_VECTOR, ...)`, `:331-340`) or one broadcast.
* The shared packet stays read-mostly: "once the copy is made, no further updates to the mailbox
  are made until the request is completed" (`:75-80`). This keeps the cache line shared and avoids
  ping-pong.
* `pmap_tlb_intr()` (`:507-...`) copies the packet privately, decrements the global
  `pmap_tlb_pendcount`, and the **last** CPU to arrive clears the active pointer and sets
  `TP_DONE`; the others just invalidate. The comment at `:512-520` explains the order: clear the
  pointer *before* `TP_DONE`, so an initiator cannot re-enter and see a stale active packet.
* A CPU in `TLBSTATE_LAZY` that receives a user-pmap shootdown clears itself from the pmap's CPU
  set and moves to `TLBSTATE_STALE` (`:536-544`). This is the lazy-TLB analogue of Illumos's idle
  delayed invalidation.
* Batching means concurrent shootdowns mostly collapse into one IPI per target, and the packet
  protocol is a **single in-flight request per initiator**, not a global lock.

Differences from Raven worth noting:

* NetBSD has **one packet per CPU**, not a global `ADDRESS`/`LENGTH` pair shared by all shooters.
* NetBSD's acknowledgement is a decrement of a shared `pendcount`, not per-CPU sequence slots.
* NetBSD's initiator may be interrupted and can flush its own pending packet from the interrupt
  handler; the code re-checks `TP_GET_COUNT(tp)` after `splvm()` (`:371-374`). Raven has no
  equivalent "interrupt flushed my mailbox" path.

---

## 10. NetBSD: AP bring-up donor (OBSERVED; files are NO-COPY)

Read for behaviour only; `cpu.c` is 4-clause and must not be copied.

* `sys/arch/x86/include/cpu.h:366-375` defines the state bits: `CPUF_BSP`, `CPUF_AP`,
  `CPUF_PRIMARY`, `CPUF_PRESENT`, `CPUF_RUNNING`, `CPUF_PAUSE`, `CPUF_GO`.
* `cpu_start_secondary()` (`cpu.c:821-869`):
  1. mark `CPUF_AP`, point `ci_curlwp` at the idle LWP;
  2. `CPU_STARTUP(ci, trampoline_paddr)` sends the AP into the trampoline;
  3. poll up to 100,000 × 10 µs for `CPUF_PRESENT`;
  4. on success, disable interrupts and synchronise the TSC; on failure, print and continue with a
     debugger hook;
  5. `CPU_START_CLEANUP(ci)`.
* `cpu_hatch()` (`cpu.c:909-1010`) is the AP side:
  1. `cpu_init_msrs`, `cpu_probe`, speculation init; from here `curcpu()`/`curlwp` work;
  2. **publish `CPUF_PRESENT`** and sync the TSC (`:944-951`);
  3. spin on `CPUF_GO`, using MONITOR/MWAIT or PAUSE, **with interrupts still off**
     (`:965-987`);
  4. after `CPUF_GO`, invalidate caches/TLB (`wbinvd`, `x86_flush`, `tlbflushg`) and load the
     kernel page tables, then publish `CPUF_RUNNING`.
* `cpu_boot_secondary()` (`cpu.c:872-907`) sets `CPUF_GO`, polls for `CPUF_RUNNING`, then
  re-synchronises the TSC and records the skew.

**OBSERVED handshake rule:** `PRESENT` means "the AP can execute AP-side code and its per-CPU
state is addressable"; `GO` is the permission to become a full running CPU; `RUNNING` means the
AP has switched to kernel page tables/TLB state. The BSP never reuses the trampoline while a
`PRESENT` AP is still waiting for `GO`; the AP is parked on its own stack. This is exactly the
kind of intermediate state Raven lacks (`online` is a single boolean).

`sys/arch/x86/x86/lapic.c` (rev 1.94, 2-clause) and `apic.c` (rev 1.10, 2-clause) are the safe
donors for LAPIC timer calibration, IPI delivery and EOI handling.

`sys/arch/x86/x86/mpacpi.c` (rev 1.112, **4-clause**) shows MADT walking: `acpi_madt_walk()` with
`mpacpi_count` / `mpacpi_config_cpu` callbacks (`:448-460`). Use the ACPI spec, not this file, if
the enumeration logic needs to be ported.

---

## 11. Raven mapping (current implementation)

### 11.1 x86_64 AP bring-up — `kernel/src/smp/bringup.rs`

| Aspect | Raven today | Donor invariant | Gap |
|---|---|---|---|
| State | one `online` counter, published once after the loop (`:100-107`); per-CPU `online` flag in `ap_start::block` | `PRESENT` → `GO` → `RUNNING` → `CPU_READY` | no intermediate state; "online" covers both "started" and "safe to IPI" |
| Start | `prepare_ap` fills a shared low trampoline, `start_ap`, wait up to 200 ms, up to 3 attempts (`:74-98`) | `CPU_STARTUP` + rendezvous, no retry | retry can hide a real bring-up bug; the ticket forbids hiding flakes |
| Trampoline reuse | BSP waits for `online` before reusing the page (`:85-98`, comment `:5-8`) | AP parked on its own stack until `GO`; control CPU rendezvous | a late AP that sets `online` after timeout is not distinguishable from a fresh one |
| Timer gates | all APs spin with interrupts off until `start_ap_timers()` (`:100-104`) | interrupts enabled at `LOCK_LEVEL` before `CPU_READY` | AP cannot serve an IPI until the BSP finishes the whole loop |
| Failure | `smp: cpu {apic_id} did not start`, then `set_online(cpu)` and continue | `CPU_READY` is never published for a failed AP | acceptable, but the rest of the kernel must not count the failed CPU as a target |

**OBSERVED mismatch:** `set_online(cpu)` publishes the *count of successfully started CPUs*, and
`online_cpus()` is the only target filter used by `shootdown.rs` and `call.rs`. There is no
`CPU_READY` equivalent, so any CPU counted online is assumed able to take an IPI. During the
bring-up window the comment says APs spin with interrupts off; if an IPI is sent after
`set_online()` but before an AP reaches its interrupt-enabled idle loop, the 100 ms deadline
fires. This is a **PROPOSED** contributor to both the KVM reset and the TCG timeout; S83 should
verify it with state-transition instrumentation.

### 11.2 x86_64 TLB shootdown — `kernel/src/smp/shootdown.rs`

| Aspect | Raven today | Donor invariant | Gap |
|---|---|---|---|
| Serialisation | one global `IrqSpinLock` (`:81`) around the whole operation | no global lock; per-CPU mailbox / concurrent calls allowed | a second shooter spins with interrupts masked |
| Interrupts while waiting | holder re-enables interrupts during the ack wait (`:131-137`) | sender holds SPL but targets can always take the IPI | only the holder is safe; waiters are not |
| Mailbox | one global `ADDRESS`/`LENGTH` + per-CPU `PENDING`/`ACKED` sequence | per-CPU packet, batched, read-mostly | no batching, no "interrupt flushed my mailbox" path |
| Target filter | `cpu < online_cpus()` | `CPU_READY` / `SPCF_RUNNING` | online ≠ ready |
| Timeout | 100 ms, clear `PENDING`, log `smp: tlb shootdown timeout on cpu N`, return false (`:138-156`) | no timeout; delayed invalidation for idle CPUs | a lost ack is logged and survived instead of prevented |
| Idle CPU | still sent an IPI | `TLBIDLE_CPU_HALTED` CAS, no IPI | a halted CPU can miss the IPI until it wakes |
| Local flush | always before publishing (`:92`) | local fast path only when no remote target | extra work, not a correctness bug |

**OBSERVED root cause already recorded by S85b** (`tests/smp/memory/FINDING.md`): `LOCK` is an
`IrqSpinLock`; `kernel/src/sync.rs:200-207` disables interrupts before spinning for the lock. If
CPU A holds `LOCK` and waits for an ack from CPU B, and CPU B is spinning on `LOCK` with interrupts
masked, B cannot take A's TLB IPI. A times out. This matches the instrumentation: all four CPUs
appeared as both shooter and target, and 41 of 50 failed sites were `unmap` with `len=0x10000`.
This is exactly the failure Illumos avoids by having **no global cross-call lock**.

### 11.3 aarch64 TLB shootdown — `kernel/src/sched/smp_shim.rs`

The aarch64 path is a parallel implementation, not a reuse of `smp/shootdown.rs`:

* same global `LOCK`, `ADDRESS`, `LENGTH`, `SEQUENCE`, `PENDING`, `ACKED`, `LAST` shape
  (`:153-166`);
* SGI `SGI_TLB` instead of an x86 vector (`:18-22`, `:205-213`);
* **the wait loop does not re-enable interrupts** (contrast `shootdown.rs:131-137`), so the
  lock-holder inversion is strictly worse: a concurrent shooter on another CPU spins with
  interrupts masked and cannot take the SGI at all;
* the timeout logs `aarch64-smp: tlb shootdown timeout on cpu {cpu}` (`:225`) and increments
  `TIMEOUTS`;
* S85f measured 3/23 failures (10 %) under TCG `-smp 4`, with the fixture still reporting
  `rounds=200 stale=0` / `smp-tlb: ok`.

**PROPOSED:** the aarch64 flake has the same root cause as the x86 one, amplified because the
aarch64 wait never opens the interrupt window. S85b should test the x86 fix shape on aarch64 too;
do not patch the two implementations independently.

### 11.4 Boot path and reset (KVM)

The KVM `-smp 2/4` symptom (QEMU exits 0 at `timer: 1000 Hz` with no SMP/user output, or resets
before APs come online) has no Raven equivalent of `CPUF_PRESENT`/`CPUF_GO`. **PROPOSED**
hypotheses for S83:

1. The BSP reuses the single trampoline page after its 200 ms/3-attempt wait while an AP is still
   executing the real-mode entry; the AP then jumps into a torn-down page table or stack.
2. The AP starts with the BSP's IDT/GDT but takes an NMI/timer before its per-CPU GS/TSS is
   installed, faults, and triple-faults. Illumos keeps SPL at `LOCK_LEVEL` and NetBSD keeps
   interrupts off until `CPUF_GO` specifically to prevent this.
3. The AP's TSC/LAPIC timer is not calibrated or not masked, so the first timer interrupt arrives
   before the AP has a valid interrupt stack.
4. KVM delivers INIT/SIPI faster than TCG, exposing a race that TCG's timing hides; the reset is a
   triple fault, not an AP "failure to start".

None of these is verified; Section 13 gives the experiments.

---

## 12. Divergences and failure modes (Raven vs donors)

Label: **OBSERVED** = visible in current Raven code or committed evidence; **PROPOSED** =
hypothesis for S83/S85b.

| # | Divergence | Evidence | Consequence | Label |
|---|---|---|---|---|
| D1 | Global `IrqSpinLock` serialises shootdowns; waiters spin with IF masked | `shootdown.rs:81`, `sync.rs:200-207`, `tests/smp/memory/FINDING.md` | concurrent shooters lose each other's acks; 100 timeouts/run | **OBSERVED** |
| D2 | `online_cpus()` is the only target filter; no `CPU_READY` | `smp/mod.rs:29-39`, `shootdown.rs:89` | IPI to a started-but-not-ready AP times out | **PROPOSED** |
| D3 | No delayed invalidation for idle/halted CPUs | `shootdown.rs:138-156` vs `hat_i86.c:2596-2605` | a halted target can miss the IPI until it wakes | **PROPOSED** |
| D4 | Single global mailbox vs per-CPU packet | `shootdown.rs:49-56` vs `x86_tlb.c:84-113` | no batching, no interrupt-flush path, one in-flight request globally | **OBSERVED** |
| D5 | Timeout is treated as recoverable and the flush returns false; callers may continue with a stale TLB | `shootdown.rs:150-156` | correctness depends on every caller checking the bool; a false negative is a stale mapping | **OBSERVED** |
| D6 | x86 AP bring-up has no `PRESENT`/`GO`/`RUNNING` handshake and retries 3× | `bringup.rs:74-98` | retry can hide a race; late AP indistinguishable from fresh | **PROPOSED** |
| D7 | aarch64 wait never re-enables interrupts | `smp_shim.rs:215-231` vs `shootdown.rs:131-137` | same inversion, strictly worse; 10 % flake measured | **OBSERVED** |
| D8 | No panic/debugger priority cross-call path | no equivalent in `smp/**` | a panic cannot reliably stop/invalidate other CPUs | **OBSERVED** |
| D9 | IPI reasons are separate functions, not a table | `ipi` module vs `ipi.c:82-93` | harder to prove every reason has a handler | **OBSERVED** |
| D10 | No TSC sync/drift record at AP start | `bringup.rs` vs `cpu.c:872-907` | timekeeping skew after AP online is unmeasured | **PROPOSED** |

---

## 13. What to test next (experiments, not conclusions)

Every experiment below is for S83 or S85b. None is a Raven result yet. Record the exact commit,
QEMU mode, iteration count and serial log; a flake stays a failure.

### 13.1 S85b — lost TLB acknowledgement

**E1. Two-shooter reproducer (smallest signal).**
Two CPUs, each alternately mapping/unmapping its own page and calling `flush_range` in a tight
loop, with no user workload. Hypothesis D1: timeouts appear only when both CPUs shoot concurrently.
Success: 0 timeouts in 50 runs after the fix; before the fix, record the timeout rate as the
baseline. Instrument shooter/target pairs in the log, not in the fast path.

**E2. Interrupt state at lock acquisition.**
Add a debug-only counter for "spins on `LOCK` with IF=0" and "timeout while a peer holds `LOCK`".
Hypothesis: every timeout has a peer inside `flush_range` or `call::probe`. Success: the two
counts correlate 1:1 on the failing runs.

**E3. Remove the global lock from the wait.**
Try a per-CPU mailbox with a target work counter (Illumos `xc_work_cnt` shape) or a NetBSD-style
per-CPU packet. The shooter publishes its own mailbox, sends one IPI per target, and waits on its
own ticket. No CPU ever spins on a lock that a peer needs in order to ack. Success criterion is
E1's 50 clean runs, plus no regression in `tests/smp/memory` TCG `-smp 4`.

**E4. Delayed invalidation for an idle target.**
Add a per-CPU state (`RUNNING`/`IDLE`/`HALTED`). If the target is halted, mark "invalidate all on
wake" and do not send an IPI (Illumos `TLBIDLE_CPU_HALTED`). Success: no timeout when one CPU is
parked in `hlt` during a shootdown, and the parked CPU's first post-wake mapping is correct.

**E5. Publish a real ready state.**
Add `ready` to the per-CPU block, set it only after the AP has loaded the IDT, installed its
per-CPU state and enabled interrupts; use it (not `online`) as the shootdown/IPI target filter.
Success: a boot-time shootdown never targets a CPU that has not set `ready`; no timeout in the
first second after `smp: N cpus online`.

**E6. aarch64 parity.**
Apply the same mailbox shape to `smp_shim.rs`, including the interrupt window during the wait.
Hypothesis D7. Success: `smp-tlb-aarch64` 50/50 under TCG `-smp 4` (current baseline 20/23 in
S85f).

**E7. Regression gate.**
`tests/smp/memory/run.sh tcg --iterations 50` and
`tests/smp/memory/run.sh kvm4 --iterations 1` after S83; first failure stops the campaign. No
retry-to-pass.

### 13.2 S83 — KVM AP bring-up

**E1. Capture the reset.**
Run `qemu ... -smp 2 -enable-kvm -d int,cpu_reset -serial file:...` and keep the last serial line
and the exception log. Hypothesis: a triple fault on the AP before it reaches C code, or a reset
caused by the BSP reusing the trampoline. Success: the faulting vector and CPU are identified.

**E2. Add `PRESENT`/`GO`/`RUNNING` transitions.**
Print one line per transition (`ap {id}: present`, `ap {id}: go`, `ap {id}: running`) and compare
KVM vs TCG ordering. Hypothesis: under KVM the BSP reaches trampoline reuse before the AP prints
`present`, or the AP never prints `present`.

**E3. Park APs on their own stacks.**
After the AP reaches long mode, park it with interrupts off on its own stack until the BSP sets
`GO`; do not reuse the trampoline page until every AP is `PRESENT`. This is the NetBSD
`cpu_hatch` shape. Success: KVM `-smp 2` reaches `smp: 2 cpus online` and the user fixture.

**E4. Verify the pre-ready interrupt invariant.**
Assert that the AP has interrupts masked (or at `LOCK_LEVEL`) until its IDT, TSS, GS and interrupt
stack are installed; mirror Illumos's `cpu_base_spl` rule. Success: no fault in the first timer
tick on any AP.

**E5. TSC and LAPIC timer.**
Synchronise the TSC at `PRESENT` and again at `RUNNING`, record drift; calibrate or mask the LAPIC
timer until `RUNNING`. Success: no timer interrupt before `RUNNING`; drift within the TCG
baseline.

**E6. KVM matrix.**
`-smp 2` and `-smp 4`, 50 boots each, `-accel kvm -cpu host`; record QEMU exit status and serial.
A reset is a failure; do not retry. Success: 50/50 both sizes.

---

## 14. What S83 / S85b / S98 should port

| Consumer | Donor | Class | Action |
|---|---|---|---|
| S85b (TLB ack) | NetBSD `x86_tlb.c` rev 1.22, `ipi.c` rev 1.30 | COPY/PORT (2-clause) | Port the per-CPU packet shape, the single in-flight request, the last-arriver `pendcount`/`DONE` rule, and the lazy/stale state. Keep the NetBSD notice. |
| S85b (cross-call) | NetBSD `subr_xcall.c` rev 1.40, `xcall.h` rev 1.8 | COPY/PORT (2-clause) | Port the ticket-counter + "only running CPUs" target rule if a general cross-call API is wanted; do not port the heavyweight kthread path for TLB. |
| S83 (AP bring-up) | NetBSD `cpu.c` rev 1.214, `mptramp.S` rev 1.28, `mpacpi.c` rev 1.112 | **NO-COPY (4-clause)** | Implement the `PRESENT`/`GO`/`RUNNING` behaviour from this spec; no code copying. |
| S83 (LAPIC/IPI) | NetBSD `lapic.c` rev 1.94, `apic.c` rev 1.10, `cpu_topology.c` rev 1.21 | COPY/PORT (2-clause) | Safe to port with notice for timer calibration, EOI, IPI delivery. |
| S83/S85b (design) | Illumos `x_call.c`, `mp_startup.c`, `hat_i86.c` | STUDY (CDDL) | Use the invariants only: `CPU_READY` gating, no global lock, idle delayed invalidation, base-SPL-until-ready. No code. |
| S98 (disk tools) | NetBSD `sbin/{newfs,fsck_ffs,gpart,disklabel,dkctl}`, `usr.sbin/{dumpfs,tunefs}` | per-file; check each | Out of scope here; record per-file licence before any port. |

FreeBSD: do not cite file revisions until a donor update actually fetches `sys/x86/x86` and
`sys/amd64/amd64`. The current sparse tree cannot support the ticket's FreeBSD comparison.

---

## 15. Summary (one page)

1. **Illumos gates cross-calls on `CPU_READY`, not on "started".** A CPU becomes ready only after
   it can take interrupts at `LOCK_LEVEL`, has cleared `CPU_QUIESCED`, and has published
   `CPU_RUNNING|CPU_READY|CPU_EXISTS`; the base SPL is raised only after that. Raven has one
   `online` counter and filters on it.
2. **Illumos has no global cross-call lock and no timeout.** Messages form a closed
   `FREE→ASYNC/CALL/SYNC→…→DONE→FREE` state machine; one IPI is sent per target per 0→1 work-count
   transition; concurrent shootdowns are allowed. Raven serialises everything behind one
   `IrqSpinLock`, and a waiter spins with interrupts masked, which is the documented lost-ack
   cause.
3. **Illumos never waits for a halted CPU.** `TLBIDLE_CPU_HALTED` is CAS'd to a delayed-invalidate
   state and skipped. Raven sends the IPI and waits 100 ms.
4. **NetBSD's TLB path is the best copy/port donor** (2-clause): per-CPU batched packet, one
   in-flight request, last-arriver `pendcount`/`DONE`, lazy/stale state. Its AP files are 4-clause
   and must be implemented from behaviour, not copied.
5. **Raven's immediate experiments** are the two-shooter reproducer, interrupt-state correlation,
   a per-CPU mailbox without the global lock, a real `ready` state, and aarch64 parity. For KVM,
   capture the reset, add `PRESENT`/`GO`/`RUNNING` transitions, and park APs until `GO`.

Nothing in this study is a Raven pass claim. The two suites remain red until S83/S85b land the
fixes and their own gates pass.
