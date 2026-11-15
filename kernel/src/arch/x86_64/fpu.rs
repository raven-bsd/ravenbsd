// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! x86-64 floating-point/SIMD state.
//!
//! The kernel is soft-float, but a user thread may execute x87/SSE at any time, including across a
//! timer preemption. Every thread owns a 64-byte-aligned save area. `FXSAVE64`/`FXRSTOR64` is the
//! baseline; when the CPU advertises XSAVE and the x87+SSE components fit in [`XSAVE_BYTES`], the
//! kernel enables `CR4.OSXSAVE` with `XCR0 = 3` and uses `XSAVE64`/`XRSTOR64` instead. AVX state
//! is deliberately not enabled: the hosted userland is baseline SSE2 (DR-14/CR-21).
//!
//! The area is in the architectural FXSAVE layout; the XSAVE header occupies the last 64 bytes.
//! A fresh area carries the FINIT defaults (`FCW = 0x037f`, empty x87 stack, `MXCSR = 0x1f80`) and
//! `XSTATE_BV = 3` so `XRSTOR` restores both legacy components.

use core::arch::x86_64::{__cpuid, __cpuid_count};
use core::sync::atomic::{AtomicBool, Ordering};

/// XSAVE area bytes for `XCR0 = 3` (512-byte legacy area + 64-byte header).
pub const XSAVE_BYTES: usize = 576;

/// Per-thread architectural save area.
#[repr(C, align(64))]
pub struct FpuArea {
    bytes: [u8; XSAVE_BYTES],
}

impl FpuArea {
    /// A fresh area with the FINIT/MXCSR defaults and an empty XSAVE header.
    ///
    /// The writes are to fixed offsets in the architectural FXSAVE layout.
    #[allow(clippy::indexing_slicing)] // fixed architectural offsets, not user data
    pub const fn new() -> Self {
        let mut bytes = [0u8; XSAVE_BYTES];
        bytes[0] = 0x7f; // FCW low
        bytes[1] = 0x03; // FCW high: 0x037f
        bytes[4] = 0xff; // abridged FTW: all x87 registers empty
        bytes[24] = 0x80; // MXCSR low
        bytes[25] = 0x1f; // MXCSR high: 0x1f80
        bytes[512] = 0x03; // XSTATE_BV: x87 | SSE
        Self { bytes }
    }
}

/// True when `XSAVE64`/`XRSTOR64` is active instead of the FXSAVE baseline.
static USE_XSAVE: AtomicBool = AtomicBool::new(false);

/// Configure the architectural FP/SIMD controls.
///
/// Returns false when the CPU has no SSE support; the scheduler refuses to start in that case.
/// Idempotent.
pub fn init() -> bool {
    let leaf1 = __cpuid(1);
    if leaf1.edx & (1 << 25) == 0 {
        return false; // no SSE
    }

    // CR0: clear EM and TS, set MP. The boot path leaves EM/TS clear, but the scheduler owns the
    // policy from here on.
    let mut cr0 = read_cr0();
    cr0 &= !(1 << 2);
    cr0 &= !(1 << 3);
    cr0 |= 1 << 1;
    write_cr0(cr0);

    // CR4: enable FXSAVE and the SSE unmasked-exception model.
    let mut cr4 = read_cr4();
    cr4 |= (1 << 9) | (1 << 10);

    // XSAVE is optional. Enable it only when x87 and SSE are both supported and the resulting
    // area fits the per-thread buffer.
    let max_leaf = __cpuid(0).eax;
    let xsave = leaf1.ecx & (1 << 26) != 0 && max_leaf >= 0x0d;
    if xsave {
        let leaf_d0 = __cpuid_count(0x0d, 0);
        let leaf_d1 = __cpuid_count(0x0d, 1);
        let size = usize::try_from(leaf_d0.ebx).unwrap_or(usize::MAX);
        if leaf_d1.eax & 0b11 == 0b11 && size <= XSAVE_BYTES {
            cr4 |= 1 << 18; // OSXSAVE
            write_cr4(cr4);
            // SAFETY: CPUID advertised XSAVE, CR4.OSXSAVE is set and XCR0 bit 0/1 are supported.
            unsafe {
                core::arch::asm!(
                    "xsetbv",
                    in("ecx") 0u32,
                    in("eax") 0b11u32,
                    in("edx") 0u32,
                    options(nostack),
                );
            }
            USE_XSAVE.store(true, Ordering::Release);
            crate::kprintln!("fpu: xsave {} bytes", XSAVE_BYTES);
            return true;
        }
    }

    write_cr4(cr4);
    USE_XSAVE.store(false, Ordering::Release);
    crate::kprintln!("fpu: fxsave 512 bytes");
    true
}

/// Save the live FP/SIMD state into `area`.
///
/// # Safety
///
/// `area` must point to at least [`XSAVE_BYTES`] writable bytes aligned to 64 bytes, and the
/// scheduler must own the CPU's FP/SIMD state (interrupts masked or the thread is current).
pub unsafe fn save(area: *mut u8) {
    if USE_XSAVE.load(Ordering::Relaxed) {
        // SAFETY: the caller guarantees the area and ownership; the mask selects all enabled
        // components and fits the 576-byte buffer (XCR0 = 3).
        unsafe {
            core::arch::asm!(
                "xsave64 [{}]",
                in(reg) area,
                in("eax") 0b11u32,
                in("edx") 0u32,
                options(nostack),
            );
        }
    } else {
        // SAFETY: same ownership; FXSAVE64 writes exactly 512 bytes.
        unsafe {
            core::arch::asm!(
                "fxsave64 [{}]",
                in(reg) area,
                options(nostack),
            );
        }
    }
}

/// Restore the FP/SIMD state from `area`.
///
/// # Safety
///
/// `area` must have been written by [`save`] (or initialised by [`FpuArea::new`]) and be aligned
/// to 64 bytes; the scheduler must own the CPU's FP/SIMD state.
pub unsafe fn restore(area: *const u8) {
    if USE_XSAVE.load(Ordering::Relaxed) {
        // SAFETY: the caller guarantees the area and ownership; XCR0 = 3 limits the state.
        unsafe {
            core::arch::asm!(
                "xrstor64 [{}]",
                in(reg) area,
                in("eax") 0b11u32,
                in("edx") 0u32,
                options(nostack),
            );
        }
    } else {
        // SAFETY: same ownership; FXSAVE64/FXRSTOR64 use the legacy 512-byte layout.
        unsafe {
            core::arch::asm!(
                "fxrstor64 [{}]",
                in(reg) area,
                options(nostack),
            );
        }
    }
}

fn read_cr0() -> u64 {
    let value: u64;
    // SAFETY: ring-0 CR0 read.
    unsafe {
        core::arch::asm!(
            "mov {}, cr0",
            out(reg) value,
            options(nomem, nostack, preserves_flags),
        );
    }
    value
}

fn write_cr0(value: u64) {
    // SAFETY: the value keeps the required kernel bits and only toggles the FPU policy bits.
    unsafe {
        core::arch::asm!(
            "mov cr0, {}",
            in(reg) value,
            options(nostack, preserves_flags),
        );
    }
}

fn read_cr4() -> u64 {
    let value: u64;
    // SAFETY: ring-0 CR4 read.
    unsafe {
        core::arch::asm!(
            "mov {}, cr4",
            out(reg) value,
            options(nomem, nostack, preserves_flags),
        );
    }
    value
}

fn write_cr4(value: u64) {
    // SAFETY: the value only adds OSFXSR/OSXMMEXCPT/OSXSAVE, features checked via CPUID.
    unsafe {
        core::arch::asm!(
            "mov cr4, {}",
            in(reg) value,
            options(nostack, preserves_flags),
        );
    }
}
