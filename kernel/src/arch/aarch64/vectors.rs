// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Exception vector table, trap frame and the Rust exception reporter.
//!
//! The table is the architectural 16-entry layout: 2 KiB aligned, 128 bytes per slot. Every slot
//! saves `x0..x30`, the interrupted SP, `ELR_EL1`, `SPSR_EL1`, `ESR_EL1` and `FAR_EL1` into a
//! [`TrapFrame`] and calls [`exception_dispatch`] with the slot index. The handler never returns:
//! it prints the dump and exits the test with failure.
//!
//! The table structure follows the MIT-licensed `raspberrypi-os-tutorials` exception entry
//! (`11_exceptions_part1_groundwork/src/_arch/aarch64/exception.s`); the frame, slot dispatch and
//! recovery path are RavenBSD's.

use core::mem::size_of;

use raven_esr::{Esr, ec_name, slot_name};

use crate::kprintln;

/// Number of architectural exception vector slots.
const VECTOR_SLOTS: usize = 16;
/// Bytes of one trap frame on the stack.
const TRAP_FRAME_BYTES: usize = 288;

/// Register state captured on exception entry.
///
/// The field order and offsets are fixed by the assembly in this module; the const assertions
/// below fail the build when the Rust layout drifts from the save sequence.
#[repr(C)]
pub struct TrapFrame {
    /// `x0` to `x30`; `x30` is the link register.
    pub x: [u64; 31],
    /// The interrupted stack pointer (SP_EL0 for lower-EL slots, SP_ELx for current-EL slots).
    pub sp: u64,
    /// `ELR_EL1`: the interrupted program counter.
    pub elr: u64,
    /// `SPSR_EL1`: the interrupted program status.
    pub spsr: u64,
    /// `ESR_EL1`: the exception syndrome.
    pub esr: u64,
    /// `FAR_EL1`: the fault address.
    pub far: u64,
}

const _: () = assert!(size_of::<TrapFrame>() == TRAP_FRAME_BYTES);
const _: () = assert!(core::mem::offset_of!(TrapFrame, sp) == 248);
const _: () = assert!(core::mem::offset_of!(TrapFrame, elr) == 256);
const _: () = assert!(core::mem::offset_of!(TrapFrame, spsr) == 264);
const _: () = assert!(core::mem::offset_of!(TrapFrame, esr) == 272);
const _: () = assert!(core::mem::offset_of!(TrapFrame, far) == 280);

core::arch::global_asm!(
    r#"
    .section .text
    .balign 2048
    .global __exception_vectors
    .type __exception_vectors, %function
__exception_vectors:

    /* One vector slot. `lower` selects whether the interrupted SP is SP_EL0 (lower EL) or the
     * current SP_ELx (same EL). The slot body must fit the architectural 128-byte stride; the
     * linker does not split `.text` per slot, so the alignment directives do the spacing. */
    .macro ventry slot, lower
    .balign 128
    sub sp, sp, #288
    stp x0, x1, [sp, #16 * 0]
    stp x2, x3, [sp, #16 * 1]
    stp x4, x5, [sp, #16 * 2]
    stp x6, x7, [sp, #16 * 3]
    stp x8, x9, [sp, #16 * 4]
    stp x10, x11, [sp, #16 * 5]
    stp x12, x13, [sp, #16 * 6]
    stp x14, x15, [sp, #16 * 7]
    stp x16, x17, [sp, #16 * 8]
    stp x18, x19, [sp, #16 * 9]
    stp x20, x21, [sp, #16 * 10]
    stp x22, x23, [sp, #16 * 11]
    stp x24, x25, [sp, #16 * 12]
    stp x26, x27, [sp, #16 * 13]
    stp x28, x29, [sp, #16 * 14]
    str x30, [sp, #240]
    .if \lower
    mrs x9, sp_el0
    .else
    add x9, sp, #288
    .endif
    str x9, [sp, #248]
    mrs x9, elr_el1
    str x9, [sp, #256]
    mrs x9, spsr_el1
    str x9, [sp, #264]
    mrs x9, esr_el1
    str x9, [sp, #272]
    mrs x9, far_el1
    str x9, [sp, #280]
    mov x0, #\slot
    mov x1, sp
    bl exception_dispatch
    b __exception_restore
    .endm

    ventry 0, 0
    ventry 1, 0
    ventry 2, 0
    ventry 3, 0
    ventry 4, 0
    ventry 5, 0
    ventry 6, 0
    ventry 7, 0
    ventry 8, 1
    ventry 9, 1
    ventry 10, 1
    ventry 11, 1
    ventry 12, 1
    ventry 13, 1
    ventry 14, 1
    ventry 15, 1

    /* Shared return path. The current handler never returns, but a future one that does must be
     * able to leave the same frame. */
__exception_restore:
    ldr x9, [sp, #256]
    msr elr_el1, x9
    ldr x9, [sp, #264]
    msr spsr_el1, x9
    ldp x0, x1, [sp, #16 * 0]
    ldp x2, x3, [sp, #16 * 1]
    ldp x4, x5, [sp, #16 * 2]
    ldp x6, x7, [sp, #16 * 3]
    ldp x8, x9, [sp, #16 * 4]
    ldp x10, x11, [sp, #16 * 5]
    ldp x12, x13, [sp, #16 * 6]
    ldp x14, x15, [sp, #16 * 7]
    ldp x16, x17, [sp, #16 * 8]
    ldp x18, x19, [sp, #16 * 9]
    ldp x20, x21, [sp, #16 * 10]
    ldp x22, x23, [sp, #16 * 11]
    ldp x24, x25, [sp, #16 * 12]
    ldp x26, x27, [sp, #16 * 13]
    ldp x28, x29, [sp, #16 * 14]
    ldr x30, [sp, #240]
    add sp, sp, #288
    eret
    .size __exception_vectors, . - __exception_vectors
"#
);

/// Rust side of every vector slot.
///
/// IRQ slots dispatch through [`crate::arch::irq`] and return to the vector's restore path; every
/// other slot prints the trap frame and exits the test with failure.
#[unsafe(no_mangle)]
pub extern "C" fn exception_dispatch(slot: u64, frame: *const TrapFrame) {
    // IRQ slots: 1 (current-EL SP0), 5 (current-EL SPx), 9 (lower EL AArch64), 13 (lower EL
    // AArch32). The CPU masks IRQ on exception entry, so the handler never nests. Slots 9/13
    // interrupted EL0, which is what enables return-to-user preemption (x86-64's
    // `timer_return(from_user)` policy).
    if matches!(slot, 1 | 5 | 9 | 13) {
        crate::arch::irq::dispatch(matches!(slot, 9 | 13));
        return;
    }
    // SAFETY: `__exception_vectors` built this frame on the boot stack and passes its address.
    // The frame is valid for the whole handler and is not aliased by other Rust code.
    let frame = unsafe { &*frame };
    let esr = Esr::new(frame.esr);
    let ec = esr.ec();
    kprintln!(
        "exception: {} EC=0x{:02x} ({}) ESR=0x{:016x} ELR=0x{:016x} FAR=0x{:016x} SPSR=0x{:016x}",
        slot_name(slot as usize),
        ec,
        ec_name(ec),
        frame.esr,
        frame.elr,
        frame.far,
        frame.spsr
    );
    print_gprs(frame);
    kprintln!("exception: halted");
    crate::boot::exit_qemu(0x11)
}

fn print_gprs(frame: &TrapFrame) {
    let mut index = 0usize;
    while index.saturating_add(4) <= frame.x.len() {
        let i0 = index;
        let i1 = index.saturating_add(1);
        let i2 = index.saturating_add(2);
        let i3 = index.saturating_add(3);
        let r0 = frame.x.get(i0).copied().unwrap_or(0);
        let r1 = frame.x.get(i1).copied().unwrap_or(0);
        let r2 = frame.x.get(i2).copied().unwrap_or(0);
        let r3 = frame.x.get(i3).copied().unwrap_or(0);
        kprintln!(
            "x{i0:02}=0x{r0:016x} x{i1:02}=0x{r1:016x} x{i2:02}=0x{r2:016x} x{i3:02}=0x{r3:016x}"
        );
        index = index.saturating_add(4);
    }
    let r28 = frame.x.get(28).copied().unwrap_or(0);
    let r29 = frame.x.get(29).copied().unwrap_or(0);
    let r30 = frame.x.get(30).copied().unwrap_or(0);
    kprintln!("x28=0x{r28:016x} x29=0x{r29:016x} x30=0x{r30:016x}");
}

/// State the vector table is a 16-slot, 128-byte-stride table.
const _: () = assert!(VECTOR_SLOTS * 128 == 2048);
