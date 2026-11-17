// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-thread architectural FP/SIMD state (aarch64).
//!
//! The kernel is built for `aarch64-unknown-none-softfloat`, so kernel code never executes
//! FP/SIMD instructions. User threads do, and they may migrate between CPUs, so the scheduler
//! saves the live `Q0..Q31`, `FPSR` and `FPCR` of the outgoing thread and restores the incoming
//! thread's copy on every switch (the x86-64 DR-14/PG-12 policy, without the syscall parking
//! optimisation: the soft-float kernel has no reason to touch the registers in between).
//!
//! `CPACR_EL1.FPEN` is per-CPU; [`init`] must run on every CPU before it may run a user thread.

use core::arch::asm;

/// Bytes of one thread's FP/SIMD save area: `Q0..Q31` (512) plus `FPSR` and `FPCR`.
pub const FPU_BYTES: usize = 32 * 16 + 16;
/// Offset of `FPSR` inside [`FpuArea::bytes`].
const FPSR_OFFSET: usize = 32 * 16;
/// Offset of `FPCR` inside [`FpuArea::bytes`].
const FPCR_OFFSET: usize = 32 * 16 + 8;

/// Architectural FP/SIMD save area for one thread.
///
/// `repr(C)` with the byte array first means the area address is the struct address, which is
/// what [`save`] and [`restore`] operate on.
#[repr(C, align(16))]
pub struct FpuArea {
    bytes: [u8; FPU_BYTES],
}

impl FpuArea {
    /// A fresh area with the reset FPCR/FPSR (round-to-nearest, no traps, no flags).
    pub const fn new() -> Self {
        Self {
            bytes: [0u8; FPU_BYTES],
        }
    }
}

/// Enable FP/SIMD at EL0/EL1 on the calling CPU.
///
/// Sets `CPACR_EL1.FPEN = 0b11`, which stops EL0 and EL1 FP/SIMD instructions from trapping to
/// EL1 (`EC 0x07`). Idempotent and per-CPU. ARMv8-A makes FP/SIMD mandatory, so this reports
/// success unconditionally; a CPU without it faults user FP code into the `EC_FP` signal path.
pub fn init() -> bool {
    let mut cpacr: u64;
    // SAFETY: CPACR_EL1 is the architectural EL1 trap control register.
    unsafe {
        asm!("mrs {}, cpacr_el1", out(reg) cpacr, options(nomem, nostack, preserves_flags));
        cpacr |= 0b11 << 20; // FPEN: no FP/SIMD trap at EL0 or EL1
        asm!("msr cpacr_el1, {}", "isb", in(reg) cpacr, options(nomem, nostack, preserves_flags));
    }
    true
}

/// Save the live FP/SIMD state into `area`.
///
/// # Safety
///
/// `area` must point to a writable [`FpuArea`] owned by the caller, and the caller must own this
/// CPU's FP/SIMD state (the scheduler critical section).
pub unsafe fn save(area: *mut u8) {
    // SAFETY: the caller guarantees a writable 528-byte area; the offsets are the architectural
    // Q0..Q31/FPSR/FPCR layout and every store is a 16-byte-aligned pair.
    unsafe {
        asm!(
            ".arch armv8-a+fp+simd",
            "stp q0, q1, [{p}, #0]",
            "stp q2, q3, [{p}, #32]",
            "stp q4, q5, [{p}, #64]",
            "stp q6, q7, [{p}, #96]",
            "stp q8, q9, [{p}, #128]",
            "stp q10, q11, [{p}, #160]",
            "stp q12, q13, [{p}, #192]",
            "stp q14, q15, [{p}, #224]",
            "stp q16, q17, [{p}, #256]",
            "stp q18, q19, [{p}, #288]",
            "stp q20, q21, [{p}, #320]",
            "stp q22, q23, [{p}, #352]",
            "stp q24, q25, [{p}, #384]",
            "stp q26, q27, [{p}, #416]",
            "stp q28, q29, [{p}, #448]",
            "stp q30, q31, [{p}, #480]",
            "mrs {tmp}, fpsr",
            "str {tmp}, [{p}, {fpsr}]",
            "mrs {tmp}, fpcr",
            "str {tmp}, [{p}, {fpcr}]",
            p = in(reg) area,
            fpsr = const FPSR_OFFSET,
            fpcr = const FPCR_OFFSET,
            tmp = out(reg) _,
            options(nostack, preserves_flags),
        );
    }
}

/// Restore the FP/SIMD state from `area`.
///
/// # Safety
///
/// `area` must point to a readable [`FpuArea`] written by [`save`] (or [`FpuArea::new`]), and the
/// caller must own this CPU's FP/SIMD state.
pub unsafe fn restore(area: *const u8) {
    // SAFETY: the caller guarantees a readable 528-byte area; the offsets mirror [`save`].
    unsafe {
        asm!(
            ".arch armv8-a+fp+simd",
            "ldp q0, q1, [{p}, #0]",
            "ldp q2, q3, [{p}, #32]",
            "ldp q4, q5, [{p}, #64]",
            "ldp q6, q7, [{p}, #96]",
            "ldp q8, q9, [{p}, #128]",
            "ldp q10, q11, [{p}, #160]",
            "ldp q12, q13, [{p}, #192]",
            "ldp q14, q15, [{p}, #224]",
            "ldp q16, q17, [{p}, #256]",
            "ldp q18, q19, [{p}, #288]",
            "ldp q20, q21, [{p}, #320]",
            "ldp q22, q23, [{p}, #352]",
            "ldp q24, q25, [{p}, #384]",
            "ldp q26, q27, [{p}, #416]",
            "ldp q28, q29, [{p}, #448]",
            "ldp q30, q31, [{p}, #480]",
            "ldr {tmp}, [{p}, {fpsr}]",
            "msr fpsr, {tmp}",
            "ldr {tmp}, [{p}, {fpcr}]",
            "msr fpcr, {tmp}",
            p = in(reg) area,
            fpsr = const FPSR_OFFSET,
            fpcr = const FPCR_OFFSET,
            tmp = out(reg) _,
            options(nostack, preserves_flags),
        );
    }
}
