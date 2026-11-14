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

use raven_abi::errno::Errno;
use raven_abi::signal::{self, ENTRY_SP, FaultRecord, MContext};
use raven_esr::{Esr, data_abort, ec_name, slot_name};
use raven_usercopy::is_user_address;

use crate::kprintln;
use crate::process;
use crate::syscall::thread;

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
    mov x2, #\lower
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

    /* Shared return path. A lower-EL frame carries the interrupted SP_EL0; it must be restored
     * before `eret`, because a context switch to another user thread changes the single SP_EL0
     * register. Current-EL frames carry SP_ELx there and must leave SP_EL0 alone. */
__exception_restore:
    cbz x2, 1f
    ldr x9, [sp, #248]
    msr sp_el0, x9
1:
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

/// Exception class: unknown reason (typically an undefined instruction).
const EC_UNKNOWN: u8 = 0x00;
/// Exception class: FP/SIMD access trapped.
const EC_FP: u8 = 0x07;
/// Exception class: illegal execution state.
const EC_ILLEGAL: u8 = 0x0e;
/// Exception class: SVC from AArch64.
const EC_SVC64: u8 = 0x15;
/// Exception class: instruction abort from a lower exception level.
const EC_IABT_LOWER: u8 = 0x20;
/// Exception class: PC alignment fault.
const EC_PC_ALIGN: u8 = 0x22;
/// Exception class: data abort from a lower exception level.
const EC_DABT_LOWER: u8 = 0x24;
/// Exception class: data abort taken without a change in exception level.
const EC_DABT_SAME: u8 = 0x25;
/// Exception class: SP alignment fault.
const EC_SP_ALIGN: u8 = 0x26;
/// Exception class: breakpoint from a lower exception level.
const EC_BREAKPOINT_LOWER: u8 = 0x30;
/// Exception class: software step from a lower exception level.
const EC_SOFTWARE_STEP: u8 = 0x32;
/// Exception class: watchpoint from a lower exception level.
const EC_WATCHPOINT: u8 = 0x34;
/// Exception class: BRK instruction.
const EC_BRK: u8 = 0x3c;

/// Rust side of every vector slot.
///
/// IRQ slots dispatch through [`crate::arch::irq`] and return to the vector's restore path. The
/// lower-EL synchronous slot handles `svc #0` (EC 0x15) and user faults; the current-EL
/// synchronous slot checks the usercopy fixup table before treating a data abort as fatal. Every
/// unhandled slot prints the trap frame and exits the test with failure.
#[unsafe(no_mangle)]
pub extern "C" fn exception_dispatch(slot: u64, frame: *mut TrapFrame) {
    // IRQ slots: 1 (current-EL SP0), 5 (current-EL SPx), 9 (lower EL AArch64), 13 (lower EL
    // AArch32). The CPU masks IRQ on exception entry, so the handler never nests. Slots 9/13
    // interrupted EL0, which is what enables return-to-user preemption (x86-64's
    // `timer_return(from_user)` policy).
    if matches!(slot, 1 | 5 | 9 | 13) {
        crate::arch::irq::dispatch(matches!(slot, 9 | 13));
        return;
    }
    // SAFETY: `__exception_vectors` built this frame on the current kernel stack and passes its
    // address; it is exclusively owned by this handler for the duration of the call.
    let frame = unsafe { &mut *frame };
    let esr = Esr::new(frame.esr);
    let ec = esr.ec();

    match slot {
        // Lower EL, AArch64.
        8 => match ec {
            EC_SVC64 => crate::syscall::svc_dispatch(frame),
            EC_DABT_LOWER => {
                let kind = abort_kind(data_abort(esr.iss()).dfsc);
                // Translation and permission faults may be COW/pager-resolvable; an
                // alignment fault must go straight to the signal/default path.
                if kind != signal::FAULT_MISALIGN && crate::process::handle_page_fault(frame.far) {
                    return;
                }
                if deliver_fault(frame, kind, frame.far) {
                    return;
                }
                crate::process::page_fault(frame.far)
            }
            EC_IABT_LOWER => {
                let kind = abort_kind(data_abort(esr.iss()).dfsc);
                if deliver_fault(frame, kind, frame.far) {
                    return;
                }
                crate::process::page_fault(frame.far)
            }
            EC_BREAKPOINT_LOWER | EC_SOFTWARE_STEP | EC_WATCHPOINT | EC_BRK => {
                if deliver_fault(frame, signal::FAULT_BREAKPOINT, frame.elr) {
                    return;
                }
                terminate_fault(signal::FAULT_BREAKPOINT)
            }
            EC_UNKNOWN | EC_ILLEGAL | EC_FP => {
                if deliver_fault(frame, signal::FAULT_ILLEGAL, frame.elr) {
                    return;
                }
                terminate_fault(signal::FAULT_ILLEGAL)
            }
            EC_PC_ALIGN | EC_SP_ALIGN => {
                if deliver_fault(frame, signal::FAULT_MISALIGN, frame.elr) {
                    return;
                }
                terminate_fault(signal::FAULT_MISALIGN)
            }
            _ => crate::process::user_exception(usize::from(ec)),
        },
        // Lower EL, AArch32: no 32-bit userland exists.
        12 => crate::process::user_exception(usize::from(ec)),
        // Current EL, SP0 (0) or SPx (4): only a usercopy fault at an instruction in the fixup
        // table may recover. Anything else is a kernel bug.
        0 | 4 => {
            if ec == EC_DABT_SAME && crate::usercopy::handle_fault_aarch64(frame) {
                return;
            }
            fatal(slot, frame, esr, ec)
        }
        _ => fatal(slot, frame, esr, ec),
    }
}

/// PSTATE bits a `SIGRETURN` context may restore: NZCV and DAIF. EL, SP and
/// the reserved fields stay as the kernel left them for an EL0t return.
const PSTATE_USER_MASK: u64 = 0xf000_0000_0000_03c0;

/// Scratch context for one delivery; a 4384-byte local per call would overflow a 20 KiB kernel
/// stack once the SVC and delivery frames are nested.
static SIGNAL_CONTEXT: spin::Mutex<MContext> = spin::Mutex::new(MContext::zero());

/// Snapshot the interrupted user state in the C36 `mcontext_t` layout.
pub fn context_from_frame(frame: &TrapFrame, context: &mut MContext) {
    context.fault_address = 0;
    context.regs.copy_from_slice(&frame.x);
    context.sp = frame.sp;
    context.pc = frame.elr;
    context.pstate = frame.spsr;
    context.reserved.fill(0);
}

/// Apply a `SIGRETURN` context to the EL0 frame.
pub fn restore_signal_context(frame: &mut TrapFrame, context: &MContext) {
    frame.x.copy_from_slice(&context.regs);
    frame.sp = context.sp;
    frame.elr = context.pc;
    frame.spsr = context.pstate & PSTATE_USER_MASK;
    frame.esr = 0;
    frame.far = 0;
}

/// Restore a `SIGRETURN` context from the registered user area into the frame.
pub fn sigreturn_frame(frame: &mut TrapFrame) -> Result<(), Errno> {
    let mut context = SIGNAL_CONTEXT.lock();
    thread::sigreturn(&mut context)?;
    restore_signal_context(frame, &context);
    Ok(())
}

/// Map a data/instruction abort status code to the C58 fault kind.
fn abort_kind(dfsc: u8) -> u32 {
    match dfsc {
        0x09..=0x0b | 0x0d..=0x0f => signal::FAULT_PAGE_PERM,
        0x21 => signal::FAULT_MISALIGN,
        _ => signal::FAULT_PAGE_NOMAP,
    }
}

/// Enter the registered entry with `bits` in `x0` and the optional fault
/// record pointer in `x1`.
fn enter_signal(frame: &mut TrapFrame, due: &thread::SignalEntry, bits: u64, second: u64) {
    let stack = if due.flags & ENTRY_SP != 0 {
        frame.sp
    } else {
        match frame.sp.checked_sub(128) {
            Some(stack) => stack & !0xf,
            None => process::exit_process(Errno::EFAULT.to_ret() as i32),
        }
    };
    if !is_user_address(stack) {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    }
    if let Some(argument) = frame.x.get_mut(0) {
        *argument = bits;
    }
    if let Some(argument) = frame.x.get_mut(1) {
        *argument = second;
    }
    frame.elr = due.entry;
    frame.sp = stack;
}

/// Deliver a pending kick to the registered user entry, if one is ready.
///
/// Called after the `svc` result is in `x0` and before the vector restores the
/// EL0 frame, so the saved context is the interrupted user state.
pub fn deliver_signal(frame: &mut TrapFrame) {
    let Some(due) = thread::signal_due() else {
        return;
    };
    let fault_pointer = {
        let mut context = SIGNAL_CONTEXT.lock();
        context_from_frame(frame, &mut context);
        thread::signal_commit(&due, &context, None)
    };
    enter_signal(frame, &due, u64::from(due.bits), fault_pointer);
}

/// Deliver a synchronous user fault to the signal entry.
///
/// Returns false when the thread has no usable `SIG_SETUP` (no registration,
/// already inside a signal); the caller then takes the default-action path.
pub fn deliver_fault(frame: &mut TrapFrame, kind: u32, addr: u64) -> bool {
    let Some(due) = thread::signal_fault_due() else {
        return false;
    };
    let record = FaultRecord {
        kind,
        addr: addr as usize,
        pc: frame.elr as usize,
    };
    let fault_pointer = {
        let mut context = SIGNAL_CONTEXT.lock();
        context_from_frame(frame, &mut context);
        thread::signal_commit(&due, &context, Some(&record))
    };
    enter_signal(frame, &due, 0, fault_pointer);
    true
}

/// Default action for a user fault with no signal entry: terminate with the
/// negative signal number used by the exit-status convention.
pub fn terminate_fault(kind: u32) -> ! {
    let status = match kind {
        signal::FAULT_DIVIDE => -8,
        signal::FAULT_ILLEGAL => -4,
        signal::FAULT_MISALIGN => -7,
        signal::FAULT_BREAKPOINT => -5,
        _ => -11,
    };
    kprintln!("process: user fault kind={kind} status={status}");
    process::exit_current(status)
}

fn fatal(slot: u64, frame: &TrapFrame, esr: Esr, ec: u8) -> ! {
    kprintln!(
        "exception: {} EC=0x{:02x} ({}) ESR=0x{:016x} ELR=0x{:016x} FAR=0x{:016x} SPSR=0x{:016x}",
        slot_name(slot as usize),
        ec,
        ec_name(ec),
        esr.raw(),
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
