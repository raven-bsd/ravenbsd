# x86-64 FP/SIMD state

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

## Why

DR-14 keeps the kernel and the `*-unknown-none` bring-up userland soft-float, but the hosted
`*-unknown-ravenbsd` libc is hard-float (CR-21/H-20). A user thread may execute x87/SSE at any
point, including when the timer preempts it or when it enters the kernel through `syscall`. The
kernel must therefore keep a complete FP/SIMD state per thread and never leak one thread's
registers into another, or one kernel path into a user thread.

## Per-thread area

`kernel/src/sched/thread.rs` embeds one `FpuArea` per thread:

* `#[repr(C, align(64))]`, 576 bytes.
* Bytes 0..512 are the architectural FXSAVE64 layout (FCW, FSW, abridged FTW, FOP, FIP, FDP,
  MXCSR, MXCSR_MASK, ST0..ST7, XMM0..XMM15, reserved).
* Bytes 512..576 are the XSAVE header (`XSTATE_BV` at 512, `XCOMP_BV` at 520).
* A fresh area carries the FINIT defaults: `FCW = 0x037f`, empty x87 tag word,
  `MXCSR = 0x1f80`, and `XSTATE_BV = 3` (x87 and SSE).

The 64-byte alignment satisfies both `FXSAVE64` (16) and `XSAVE64` (64).

## Detection and enablement

`kernel/src/arch/x86_64/fpu.rs` runs once from `sched::init`:

1. CPUID leaf 1 must advertise SSE (`EDX` bit 25).
2. CR0.EM and CR0.TS are cleared and CR0.MP is set.
3. CR4.OSFXSR and CR4.OSXMMEXCPT are set, which enables SSE and the FXSAVE/FXRSTOR instructions.
4. If CPUID advertises XSAVE (leaf 1 `ECX` bit 26), leaf 0x0D exists, leaf 0x0D:1 supports both
   x87 and SSE state, and the XCR0 = 3 area fits 576 bytes, the kernel sets CR4.OSXSAVE and
   `XCR0 = 3` and uses `XSAVE64`/`XRSTOR64`.
5. Otherwise it keeps the FXSAVE64/FXRSTOR64 baseline.

The kernel deliberately does not enable AVX/AVX-512 state (`XCR0` bit 2 and above stay clear).
The hosted userland is baseline SSE2 per H-20; a user AVX instruction faults instead of silently
using unsaved state.

QEMU TCG's CPU models do not advertise the SSE state component in CPUID.0D:1 (`-cpu max`
reports `EAX = 0x5`, `-cpu Haswell` reports `0x1`), so the current qtest exercises the FXSAVE64
baseline. The XSAVE64 path is implemented and selected only on CPUs that advertise both the x87
and SSE components; it remains UNVERIFIED on this QEMU.

## Context switch

`kernel/src/sched/context.rs::switch` runs on the scheduler critical section:

* Unless the outgoing user thread is inside a syscall, the live FP/SIMD state is `FXSAVE64`d
  (or `XSAVE64`d) into its area.
* Unless the incoming user thread is inside a syscall, its area is restored.
* A thread inside a syscall already parked its user state at syscall entry; the switch neither
  overwrites nor restores it. Its kernel-time live state is irrelevant because the kernel is
  soft-float and the user state is restored on syscall exit.
* Kernel threads use the same path, so a kernel thread's own (unused) FP state is also isolated.
* A new thread starts from the FINIT/MXCSR defaults.

## User/kernel transitions

`syscall_entry` calls `sched_syscall_enter` before `syscall_dispatch` and
`sched_syscall_exit` after it. The enter path FXSAVE64s the user state into the thread area and
sets `fpu_saved`; the exit path restores it and clears `fpu_saved`. A syscall that blocks and
reschedules keeps the parked state across the switch and restores it when the thread finally
returns to ring 3. Interrupt/trap entries that do not enter Rust through the syscall path rely on
the context switch for the same reason: the user state stays live until the switch, which saves
it, and a direct return to user leaves it untouched.

## Coverage

`tests/qtest/cases/fpu-preempt.toml` boots one CPU and runs two user threads, each executing a
20,000,000-step dependent `addsd` chain in XMM0/XMM1 and yielding every 100,000 steps. The XMM
registers remain live across every `YIELD` syscall, and timer preemption switches between the two
threads. The exact results (20,000,000.0 and 980,000,000.0 as `f64` bit patterns) must survive;
a leaked XMM or MXCSR register changes at least one result.

Negative control: with `fpu::save`/`restore` temporarily reduced to no-ops, the case fails with
`fpu-preempt: fail main result`; with the real save/restore it passes.

## Limits

* AVX/AVX-512 state is not enabled; only x87, MXCSR and XMM0..XMM15 are saved. This matches the
  baseline SSE2 hosted target (H-20/CR-21).
* Signal frames still carry `fpregs = 0`; PG-7/S22 follow-up adds the FP/SIMD state to the
  signal frame and `AT_MINSIGSTKSZ`.
* `PROC_FORK` starts the child's first thread from the FINIT defaults; it does not copy the
  parent's live FP registers. This is a documented gap for future `fork` semantics.
* The kernel itself is compiled without SSE (`x86_64-unknown-none` only advertises `fxsr`), so
  enabling the architectural controls does not change kernel code generation.
* Lazy switching (CR0.TS) is not used: the state is eager, as the ticket requires.
