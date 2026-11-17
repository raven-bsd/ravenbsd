<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# x86-64 user SMP

Status: OBSERVED for the code as of S75.

## 1. Model

With `smp_user=on`, user threads, process-context syscalls and user page faults may run on any
online CPU; with `smp_user=off` (the default) they stay pinned to CPU 0, exactly as before S75.
Application processors always run the scheduler, so a user thread on an AP is entered through the
same `sched_thread_start`/`enter_user` path as CPU 0.

Every CPU has, independently:

* its own GDT copy whose TSS descriptor points at that CPU's TSS;
* its own TSS (`rsp0`, three IST slots) and three IST stacks;
* its own `syscall` MSRs (`STAR`, `LSTAR`, `FMASK`, `EFER.SCE`) and FP/SIMD controls;
* its own GS block (`ap_start::PerCpuBlock`) holding the kernel stack top and current thread.

`context::switch` writes the incoming thread's `kernel_stack_top` into the **current** CPU's
`rsp0`, so a ring-3-to-ring-0 transition on any CPU uses the right stack. The GS pair is installed
per CPU by `ap_start::install_gs`; `load_gdt` deliberately leaves FS/GS alone so it never resets
their MSR-provided bases.

## 2. Select-CPU policy

`enqueue` chooses a CPU for a new user thread with a round-robin counter across the online CPUs;
a blocked thread resumes on its `last_cpu`. `target_cpu` honours `last_cpu` for user threads only
when `smp_user=on`. Kernel threads stay local and are never stolen across CPUs.

Cross-CPU wake: `sched::wake` pushes a blocked thread onto its `last_cpu`'s run queue and sends a
reschedule IPI. Process teardown (`terminate_once`) asks every other online CPU to reschedule so a
user thread still running there observes `process.exiting` at its next scheduling point.

## 3. TLB shootdown

`shootdown::flush_range` already broadcasts to every online CPU and waits for acknowledgements;
with user threads on APs the same path invalidates user mappings on every CPU that may have them.
The kernel's own user page-table mutation calls this before recycling frames.

## 4. External IRQ distribution (S75a)

Each new external interrupt object (IOAPIC legacy and MSI/MSI-X) chooses a destination CPU at
creation: with `smp_user=on` the destination round-robins over the application processors; with
`smp_user=off` it stays on the BSP. The IOAPIC redirection entry and the MSI message address carry
that CPU's APIC id, and `handle_interrupt` records the delivery CPU. The kernel timer and the
serial console stay on the BSP. MSI/MSI-X on the Pi 4 platform is not covered by this ticket.

## 5. Known limits

* The select-CPU policy for both user threads and external IRQs is round-robin, not load-aware or
  affinity-aware. A real affinity API is a follow-up.
* MSI/MSI-X destination selection is x86-64 only; the Pi 4 platform work is separate.

## 6. Boot flag

`smp_user=off|on` is parsed once in `sched::init`. Off is the default until the soak gate passes.
`smp-user-trace` prints the first user thread per CPU and cross-CPU wakeups; it is test-only
evidence.
