<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SMP TLB shootdown: donor protocol and concrete fix proposal

Status: **design proposal for S83/stuart; no kernel source changed here.** S85b round 2 showed
that the current timeout is a lock/interrupt protocol defect, not a memory or VM data bug. DR-51
authorises reading and porting the permissive NetBSD/FreeBSD PMAP/xcall/TLB protocol. This
document records the donor revisions and the exact rule to port, then gives a minimal patch for
`kernel/src/smp/shootdown.rs` plus the concurrent-waiter test extension.

## 1. Donor map

The donor checkouts are under `/home/bo/work/ravenos-workspace/ravenbsd-resources/`
(`donors.lock`):

| Donor | Revision | Files read for this design | Per-file licence / role |
|---|---|---|---|
| NetBSD | `36eddebcc36cb0775aa2962005b9c05419a6ff01` | `sys/arch/x86/x86/x86_tlb.c` (RCS 1.22, 2026-03-17); `sys/arch/x86/x86/ipi.c` (1.30); `sys/arch/x86/x86/pmap.c` (1.429); `sys/arch/x86/include/cpuvar.h` (1.55); `sys/arch/x86/include/pmap_private.h` (1.5); `sys/kern/subr_xcall.c` (1.40); `sys/sys/xcall.h` (1.8) | BSD-2-Clause per file headers (NetBSD Foundation 2-clause; the `pmap_private.h` Cranor header is also 2-clause). The tree contains other licences elsewhere; only these files were read. |
| FreeBSD | `133344405f532bd54ff3629a77d4c4e062f01fdb` | `sys/amd64/amd64/mp_machdep.c`; `sys/x86/x86/mp_x86.c`; `sys/amd64/amd64/pmap.c`; `sys/kern/subr_smp.c`; `sys/x86/include/apicvar.h`; `sys/x86/include/x86_smp.h` | `mp_machdep.c` and `subr_smp.c` say BSD-2-Clause; `mp_x86.c` is the same 2-clause body with a no-endorsement sentence; `pmap.c` is explicitly **BSD-4-Clause**; `x86_smp.h` is Beerware. No donor code is copied into RavenBSD; this is a behavioural specification only. |

The ticket names `xcall.c`; NetBSD's x86 TLB path does not use the generic xcall queue. It uses a
per-CPU TLB packet and direct LAPIC IPIs (`x86_tlb.c`), while `subr_xcall.c` is a generic
cross-call mechanism with its own per-CPU/lock protocol. The TLB design below follows the direct
x86 TLB protocol; the xcall read is background, not the patch source.

## 2. Current Raven defect

`kernel/src/smp/shootdown.rs` serialises every `flush_range` with:

```rust
static LOCK: IrqSpinLock<()> = IrqSpinLock::new(());
...
let _guard = LOCK.lock();
```

`IrqSpinLock::lock` (`kernel/src/sync.rs:200-207`) disables interrupts **before** the
`swap(true)` spin. Two CPUs that call `flush_range` at the same time therefore behave as:

1. CPU A holds `LOCK`, publishes `PENDING`, sends TLB IPIs, enables its own interrupts and waits
   for `ACKED`.
2. CPU B spins on `LOCK` with interrupts masked. It cannot take A's TLB IPI.
3. A reaches the 100 ms deadline, clears `PENDING`, logs `smp: tlb shootdown timeout on cpu B`,
   and returns `false`.
4. B eventually acquires `LOCK`; the cycle can repeat.

S85b round 2 observed this with 50 failed flush calls / 80 target timeouts in one instrumented
run, all four CPUs as both shooter and target, with sites `unmap` (41), `fault-cow` (8) and
`fault-protect` (1). The synchronized single-controller TLB-remap rounds did not produce the
timeout. The target APs were online, the shared IDT was loaded, and the IPI gates were installed;
the missing acknowledgement is real, not a count/sequence bug. The memory data path stayed clean
(`tlb_stale=0`), which is exactly why the lock protocol must be fixed rather than the VM code.

## 3. Donor protocol

### 3.1 NetBSD `x86_tlb.c`

- Each CPU owns a pending `pmap_tlb_packet_t`; a request is copied into a stack mailbox and
  published through the global `pmap_tlb_packet` pointer with an atomic compare-and-swap
  (`x86_tlb.c:115`, `:413`).
- If the pointer is busy, the waiter **drops the IPL (`splx`) before spinning**, so interrupts
  can be taken while it waits (`:399-425`). It re-enters `splvm` only after acquiring the pointer.
- The initiator copies the packet, clears its per-CPU buffer, publishes the target mask and
  sends one direct LAPIC IPI per target (`:298-350`).
- The remote handler makes a private copy, decrements the global pending count; the last remote
  clears the active pointer **before** setting `TP_DONE` (`:507-532`). The initiator polls
  `TP_DONE` with a pause loop and no global IRQ-masked spin (`:486-492`).
- Local invalidation is done by the initiator after publishing the remote request; the target
  invalidates in its IPI handler. Packet fields are written before the pointer publish; the
  pending/DONE handshake orders the completion.
- The donor uses `splvm`/SPL, not an `IrqSpinLock` held across the acknowledgement wait. The
  critical section is short; waiting is interruptible.

### 3.2 FreeBSD `smp_targeted_tlb_shootdown_native`

- The function requires **interrupts enabled** at the initiator (`mp_machdep.c:664-672`), then
  enters a critical section with preemption disabled.
- It writes the request fields into per-CPU `pc_smp_tlb_*`, advances a per-CPU generation,
  performs a release fence, zeroes each target's invalidation-scoreboard slot, sends the IPIs,
  performs the local callback, and waits for every target's scoreboard slot to become the new
  generation (`:675-708`).
- The target handler reads the initiator's fields, performs an acquire fence, stores its
  generation in the initiator's slot, and then performs the invalidation (`:1105-1157`). The
  acknowledgement may precede the invalidation because the target cannot return to user mode
  before the handler finishes.
- Multiple initiators can have outstanding requests because the scoreboard is indexed by
  `(target CPU, initiator CPU)`; there is no single global request spinlock. `smp_ipi_mtx`
  serialises the separate rendezvous path, and its comment explicitly calls out the deadlock
  risk from busy-waiting with interrupts disabled (`subr_smp.c:151-164`).

### 3.3 The rule, independent of donor spelling

A CPU waiting for a TLB shootdown lock or acknowledgement **must be able to take the TLB IPI**.
That means the wait must have interrupts enabled (with preemption disabled so the request cannot
migrate), and the active request must remain valid until every remote acknowledgement or a
conscious timeout. An `IrqSpinLock` held across the wait violates this rule. The same rule applies
to any waiter on the active-request pointer, not just the initiator.

## 4. Required Raven invariants

| Rule | Raven implementation |
|---|---|
| Request ownership | One active request (minimal fix) or per-initiator slots (full donor form). The request lives in a stack/static slot until every ACK or timeout. |
| Trigger ordering | Write `addr`/`len`/target mask/generation, `Release` publish the active pointer or scoreboard slot; target `Acquire` reads the fields. |
| Wait state | Preemption disabled; interrupts **enabled** while waiting for the active pointer or ACKs; restore the caller's original IF on return. |
| Target action | IPI handler reads the active request, invalidates the range locally, then `Release` stores its generation/ACK. |
| Ack wait | Initiator `Acquire`-loads each target ACK; no frame is freed/reused before all targets ACK. |
| Timeout/late IPI | Keep a bounded diagnostic deadline; on timeout, stop publishing/clearing the active request, return `false`, and let callers preserve frames. A late IPI must compare the generation and must not acknowledge a new request accidentally. |
| Local TLB | Initiator invalidates locally after the request is visible; remote CPUs invalidate in the handler. |
| Preemption | The initiator is pinned by a preempt guard; the target handler runs with IRQs disabled and returns promptly. |
| Memory model | Use `Acquire`/`Release` on the active pointer and ACK slots; do not rely on x86 TSO alone in portable Rust. |

The current `SEQUENCE`/`PENDING`/`ACKED`/`LAST` mailbox already has the right late-IPI idea. It
only needs a non-IRQ-masked wait around the active request; the active request itself must not be
replaced while a target might still be handling it.

## 5. Concrete patch proposal for `kernel/src/smp/shootdown.rs`

### Option A — minimal serialized request (recommended for S83)

This preserves the current single-request/ACK model but replaces the `IrqSpinLock` wait with a
NetBSD-style active pointer plus an interruptible waiter. It is the smallest patch and fixes the
self-deadlock directly.

```rust
// Replace `LOCK: IrqSpinLock<()>` with:
static ACTIVE: AtomicBool = AtomicBool::new(false);

struct ActiveGuard {
    flags: u64,
}

impl ActiveGuard {
    fn acquire() -> Self {
        // Local helpers in shootdown.rs: save RFLAGS.IF + cli, and restore the saved IF.
        // This keeps the patch inside S83's kernel/src/smp/** Owns; do not change sync.rs.
        let flags = x86_save_and_disable_interrupts();
        percpu::preempt_inc();
        loop {
            if !ACTIVE.swap(true, Ordering::Acquire) {
                break;
            }
            // A waiter must be able to take the active request's TLB IPI.
            ap_start::enable_local_interrupts();
            while ACTIVE.load(Ordering::Acquire) {
                core::hint::spin_loop();
            }
            ap_start::disable_local_interrupts();
        }
        Self { flags }
    }
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        ACTIVE.store(false, Ordering::Release);
        percpu::preempt_dec();
        x86_restore_interrupts(self.flags);
    }
}
```

`flush_range` then changes only its ownership line:

```rust
- let _guard = LOCK.lock();
+ let _guard = ActiveGuard::acquire();
```

The existing `PENDING`/`ACKED`/`LAST`/`SEQUENCE` code and the "enable interrupts while waiting
for ACKs" loop stay; the active request is now protected by `ACTIVE`, not by an IRQ-masked spin.
`ActiveGuard` must restore the caller's IF, not blindly `sti`; add a dedicated helper in
`kernel/src/arch/x86_64/ap_start.rs` or reuse the saved-flags pattern from `sync.rs`.

If S83 wants the exact NetBSD shape, use a stack `Request { addr, len, targets, generation,
acked }` published through an `AtomicUsize` active pointer; the waiter spins with IRQs enabled
until the pointer is null. The target handler reads the pointer and request fields, flushes, and
`Release` stores the ACK. This is the same rule with a larger patch.

### Option B — full FreeBSD-style per-initiator scoreboard

For future multi-initiator throughput, add `REQUESTS[MAX_CPUS]` request slots and a
`SCOREBOARD[MAX_CPUS][MAX_CPUS]` generation matrix. An initiator fills its per-CPU slot, releases
a generation, zeroes each target's scoreboard entry, sends IPIs, and waits for each target's
generation. The target handler walks initiator slots, reads the request fields, stores its
generation, then invalidates. No global active pointer is needed; concurrent shootdowns can
overlap. This is a larger patch and should follow, not replace, the minimal fix.

### Patch boundary

- Only `kernel/src/smp/shootdown.rs` and the small IF-save helper belong to S83/stuart.
- Do **not** change `kernel/src/memory/**` or `crates/raven-vmem/**` to serialize callers around
  the broken lock; that would hide the protocol bug and leave other shootdown callers exposed.
- Keep the 100 ms deadline as a diagnostic initially; do not raise it to hide a timeout. After
  the lock fix, add a diagnostic distinction between "target never handled the IPI" (FLUSHES
  unchanged) and "target handled it late" (FLUSHES advanced). S85a observed TCG timer stalls up
  to ~229 ms; if late handling remains, that is a separate TCG scheduling measurement, not a
  reason to declare a pass.

## 6. Concurrent-waiter regression

`tests/smp/memory/src/main.rs` now has a four-way barrier before the synchronized TLB-remap
phase:

- three workers park distinct 16-page anonymous mappings and wait on `WAITER_GO`;
- the controller maps its own 16-page mapping, prints
  `smp-memory: concurrent-shootdown waiters=4`, sets the barrier, and unmaps;
- all four threads call the unmap/flush path concurrently; the qtest expects the waiter line and
  still forbids every `smp: tlb shootdown timeout` line;
- the existing alloc/fault, TLB-remap, fork/exit and p90/worst checks are unchanged.

Before S83, one TCG iteration matches the new line and still fails on 105 kernel timeout lines
(the user summary is clean). A regression that reintroduces the IRQ-masked wait is caught by the
same case; no retry is performed.

## 7. Gate

Apply the S83 patch, then run:

```sh
sh tests/smp/memory/run.sh tcg --iterations 50
sh tests/smp/memory/run.sh kvm2 --iterations 50
sh tests/smp/memory/run.sh kvm4 --iterations 50
cargo nextest run -p raven-vmem
make check-fast
```

The TCG 50/50 and KVM `-smp 2/4` campaigns must be clean; any timeout, hang, lost CPU or flake
remains a blocker. KVM AP bring-up is also S83 and currently stops at `timer: 1000 Hz`.

## 8. References

- NetBSD `sys/arch/x86/x86/x86_tlb.c`, `ipi.c`, `pmap.c`; `sys/arch/x86/include/cpuvar.h`,
  `pmap_private.h`; `sys/kern/subr_xcall.c`; `sys/sys/xcall.h` at `36eddebc`.
- FreeBSD `sys/amd64/amd64/mp_machdep.c`, `sys/x86/x86/mp_x86.c`,
  `sys/amd64/amd64/pmap.c`, `sys/kern/subr_smp.c`, `sys/x86/include/apicvar.h`,
  `sys/x86/include/x86_smp.h` at `13334440`.
- Raven `kernel/src/smp/shootdown.rs`, `kernel/src/sync.rs`, `kernel/src/arch/x86_64/ap_start.rs`,
  `tests/smp/memory/` and `tests/smp/memory/FINDING.md`.
