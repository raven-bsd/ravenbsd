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
| `5x sh tests/smp/lock-time/run.sh repro` | run 3: `tlb shootdown timeout ... nr=0x1 irq_age_ms=127` | 5/5 PASS, no TLB line |
| `posix-fs-smp` TCG4 | `tlb shootdown timeout ... nr=0x62` on the pre-fix kernel | no TLB line; the long TCG4 acceptance is host-load blocked (fixture worker deadline under load 100, preserved) |
| `posix-fs-smp-kvm4` | pending | **pass** at host load 95-123, race 50/50, done 8, init 0; the earlier NMI `swapgs` page fault is gone |
| `make check-fast` | - | `check-fast: PASSED` |

The mutation check reverts `send_tlb` to the fixed IPI; `smp-boot` fails the IF=0 check again.
No test expectation was weakened and no deadline was raised.

## Donor reconciliation

The TLB completion protocol is from NetBSD `sys/arch/x86/x86/x86_tlb.c`
`pmap_tlb_shootnow` (BSD-2): the shooter publishes the packet, enables interrupts, and waits for
`TP_GET_DONE`; the donor waits without a timeout. Illumos `usr/src/uts/i86pc/os/x_call.c` and
`hat_i86.c` are the cross-call design reference (CDDL, behaviour only; the sparse donor checkout
has no `usr/src/uts`). Linux `arch/x86/mm/tlb.c` is outcome-only.

Raven divergence: the shootdown keeps the S113 bounded 100 ms diagnostic rather than an
unbounded wait, and delivers the request as an NMI so an `IF=0` target does not delay the
invalidation. No donor code was copied or translated.
