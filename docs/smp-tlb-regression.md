<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# SMP TLB shootdown regression (S159)

Status: **OBSERVED**. A TCG `-smp 4` target in a long `IF=0` kernel section could not take the
fixed TLB shootdown IPI before the 100 ms sender deadline. The shootdown now uses NMI delivery,
so the invalidation is taken even with interrupts disabled; the deadline and the test
expectations are unchanged.

## Symptom

`posix-fs-smp` and the S113 `tlb-repro` failed with one forbidden line while the fixture itself
was green:

```text
smp: tlb shootdown timeout on cpu 1 seq=7314 last=7314 flushes=6401 state=1 syscall=1 pf=0 pfp=0 wq=0 nr=0x62 user_if=1 irq_age_ms=0 state_age_ms=182
smp: tlb shootdown timeout on cpu 0 seq=3 last=2 flushes=2 state=1 syscall=1 pf=0 pfp=0 wq=0 nr=0x1 user_if=1 irq_age_ms=127 state_age_ms=180 ack_after_ms=18446744073709551615 ack_age_ms=218
```

The second line is the S113 EXIT-path class: `irq_age_ms=127` means the target had not taken a
timer interrupt for 127 ms, and `ack_after_ms=MAX` means the shootdown was not acknowledged in
the window. The target was in a long `IF=0` section (EXIT teardown, usercopy fault resolution, or
the scheduler block handoff), not spinning on the address-space lock (the S128 deferral covers
that case).

## Fix

* `kernel/src/arch/x86_64/apic.rs`: `send_nmi` for xAPIC and x2APIC.
* `kernel/src/arch/x86_64/ipi.rs`: `nmi_tlb_isr` on vector 2 (the IDT gives vector 2 `IST2`);
  the handler calls `shootdown::handle_ipi()` and sends no LAPIC EOI. `send_tlb` uses NMI.
  The NMI entry is **paranoid about `swapgs`**: it reads the privileged `KERNEL_GS_BASE` MSR and
  swaps when that value is still a kernel address, so an NMI landing inside the syscall
  entry/exit `swapgs` window still reaches the kernel per-CPU block. The first cut used the
  `CS&3` check only and faulted in `read_gs_self` (`KERNEL_GS_BASE` was `0`) under KVM4; the MSR
  check fixed it.
* `kernel/src/smp/shootdown.rs`: `handle_ipi` returns whether a pending shootdown was handled;
  the timeout line records `ack_after_ms`/`ack_age_ms`. The 100 ms deadline and `ActiveGuard`
  serialization are unchanged.
* `kernel/src/smp/selftest.rs`: a deterministic `selftest=smp` check pins a kernel thread on
  CPU 1, clears IF for 150 ms, and shoots down from CPU 0. It prints
  `selftest: smp tlb if0 ok elapsed_ms=N`.

## Test matrix

| Test | Before | After |
|---|---|---|
| `cargo xtask qtest --case smp-boot` (new IF=0 check) | `selftest: smp tlb if0 failed: IF=0 shootdown timed out` | `selftest: smp tlb if0 ok elapsed_ms=0..4` |
| `cargo xtask qtest --case smp-boot` (K393 edges) | - | `gs ok absent=1 invalid=1`, `swapgs ok`, `nested ok rounds=32`; the three mutations fail |
| `5x sh tests/smp/lock-time/run.sh repro` | run 3: `tlb shootdown timeout ... nr=0x1 irq_age_ms=127` | 5/5 PASS, no TLB line |
| `posix-fs-smp` TCG4 | `tlb shootdown timeout ... nr=0x62` on the pre-fix kernel | no TLB line; 11/14 executed low-load runs passed, 3 hit the S85e fixture worker deadline (one at host load 1.33), preserved |
| `posix-fs-smp-kvm4` | pending | **pass** at host load 1.69 on the merged tree, race 50/50, done 8, init 0; the earlier NMI `swapgs` page fault is gone |
| `make check-fast` | - | `check-fast: PASSED` |

## Low-load TCG4 campaign

After merging current `origin/master`, four batches of five TCG4 `posix-fs-smp` runs were attempted
at host load 1.3-19.9. The batches stop on the first failure:

| Batch | Runs executed | Pass | Fail | Failure |
|---|---|---:|---:|---|
| 1 | 1 | 0 | 1 | fixture worker deadline, load 19.95 |
| 2 | 5 | 4 | 1 | fixture worker deadline, load 13.51 |
| 3 | 5 | 5 | 0 | - |
| 4 | 3 | 2 | 1 | fixture worker deadline, load 1.33 |

No run produced a `smp: tlb shootdown timeout` line. The batch-4 failure at **host load 1.33**
shows the fixture deadline is not purely host load: the S85e mount latency is about 1.6 s per
mount and the 220 s worker deadline is marginal on this TCG host. That is an S85e/S139 fixture
issue, outside S159's Owns; the S159 TLB primitive is covered by the deterministic IF=0 test and
the KVM4 run.

The mutation check reverts `send_tlb` to the fixed IPI; `smp-boot` fails the IF=0 check again.
No test expectation was weakened and no deadline was raised.

## NMI entry contract (K393)

The vector-2 entry has three observable states:

| State | `GS_BASE` | `KERNEL_GS_BASE` | Entry action |
|---|---|---|---|
| kernel GS active | per-CPU block | user base (0 after a syscall) or the same block at early boot | no swap; handler uses `GS_BASE` |
| user GS active | 0 (user base) | per-CPU block | swap on entry, swap back on exit |
| syscall `swapgs` window | 0 (user base) | per-CPU block | swap on entry, swap back on exit |

The entry must not use `CS` to decide: an NMI can land after the syscall instruction has loaded
kernel `CS` but before `swapgs` has exchanged the bases. It reads the privileged
`KERNEL_GS_BASE` (`0xC0000102`); a kernel address there (`value >> 47 == 0x1ffff`) means the
bases are not swapped yet, so it executes `swapgs` and sets the `r15` swap flag. A zero or
non-kernel value means the kernel GS base is already active (or absent by design), so it does
not swap and must leave both MSRs unchanged. `r15` is saved on the IST stack and restored after
the optional exit `swapgs`.

The handler runs on vector 2 with IST2, sends no LAPIC EOI, and touches only the shootdown
atomics and `invlpg`; it takes no lock. NMI is blocked until `iretq`, so a second NMI is latched
and delivered afterwards. `shootdown::handle_ipi` returns false when `PENDING == 0` or
`LAST == sequence`, which makes a duplicate or stale NMI a no-op (no double acknowledgement).
Concurrent shooters serialize on `ActiveGuard`; a CPU that is itself shooting down may receive
an NMI while waiting for or holding the guard, and the handler still acks without taking it.

The deterministic `selftest=smp` edges:

* `selftest: smp tlb gs ok absent=1 invalid=1`: CPU 1 has `KERNEL_GS_BASE=0` and then a
  non-kernel nonzero value while `GS_BASE` is the per-CPU block; the NMI must not swap and must
  leave both MSRs unchanged.
* `selftest: smp tlb swapgs ok elapsed_ms=N`: CPU 1 simulates the pre-`swapgs` window
  (`GS_BASE=0`, `KERNEL_GS_BASE=per-CPU`, IF=0); the NMI must swap, ack the shootdown and
  restore the pair.
* `selftest: smp tlb nested ok rounds=32`: two pinned shooters race 16 shootdowns each; all
  must acknowledge, and a following bare NMI with no pending sequence must not change the
  target's flush count.

The mutation checks are: unconditional `swapgs` fails the GS-pattern test; the old CS-only
entry fails the `swapgs`-window test; dropping the `LAST == sequence` guard fails the nested
test with `duplicate NMI acknowledged a stale shootdown`. The merged entry itself is unchanged;
the tests are the K393 deliverable. The pinned targets are settled briefly before the next
target is queued on the same CPU; that is test sequencing, not product behaviour.

## KVM4 10x sample (K407)

`posix-fs-smp-kvm4` was run ten times on `origin/master` `b04e71ad` (NMI fix + K393 tests) at
host load 9.8-59: **8 pass, 2 fail**. The serials and per-run summary are in
`tests/evidence/smp-tlb/`.

| Run | Result | Failure |
|---:|---|---|
| 2 | FAIL | six `smp: tlb shootdown timeout` lines on CPU 0; the fixture worker deadline follows the stall; two stale-NMI diagnostics |
| 8 | FAIL | six `smp: tlb shootdown timeout` lines on CPU 0; fixture green |

Both failures show the same CPU 0 pattern: `irq_age_ms` 100, 203, 307, 410, 514, 617, 720 with
`ack_after_ms=MAX` and `last` stuck at the previous sequence. The target vCPU was not scheduled
for up to 720 ms; this is a host KVM stall, not a guest IF=0/TLB delivery defect. The 100 ms
deadline is unchanged. The KVM4 10x sample is therefore **8/10, with two TLB-stall failures**;
it is not a clean TLB gate. K391 owns the fixture stall and a quiet-host rerun is required before
the release gate.

## Donor reconciliation

The TLB completion protocol is from NetBSD `sys/arch/x86/x86/x86_tlb.c`
`pmap_tlb_shootnow` (BSD-2): the shooter publishes the packet, enables interrupts, and waits for
`TP_GET_DONE`; the donor waits without a timeout. Illumos `usr/src/uts/i86pc/os/x_call.c` and
`hat_i86.c` are the cross-call design reference (CDDL, behaviour only; the sparse donor checkout
has no `usr/src/uts`). Linux `arch/x86/mm/tlb.c` is outcome-only.

Raven divergence: the shootdown keeps the S113 bounded 100 ms diagnostic rather than an
unbounded wait, and delivers the request as an NMI so an `IF=0` target does not delay the
invalidation. No donor code was copied or translated.
