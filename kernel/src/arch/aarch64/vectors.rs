// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Exception vector table, trap frame and the Rust exception reporter.
//!
//! The table is the architectural 16-entry layout: 2 KiB aligned, 128 bytes per slot. Every slot
//! saves `x0..x30`, the interrupted SP, `ELR_EL1`, `SPSR_EL1`, `ESR_EL1` and `FAR_EL1` into a
//! [`TrapFrame`] and calls [`exception_dispatch`] with the slot index. The handler never returns:
//! it prints the dump and exits the test with failure.
//!
//! Lower-EL entries also capture the user `TPIDR_EL0` into the current thread, and the restore
//! path reinstalls it before `eret`; see [`save_user_tls`] and [`aarch64_restore_user_tls`].
//!
//! The table structure follows the MIT-licensed `raspberrypi-os-tutorials` exception entry
//! (`11_exceptions_part1_groundwork/src/_arch/aarch64/exception.s`); the frame, slot dispatch and
//! recovery path are RavenBSD's.

use core::arch::asm;
use core::mem::size_of;
use core::sync::atomic::Ordering;

use raven_abi::errno::Errno;
use raven_abi::signal::{self, ENTRY_SP, FaultRecord, MContext};
use raven_esr::{Esr, data_abort, ec_name, slot_name};
use raven_usercopy::{UserSlice, is_user_address};

use crate::arch::aarch64::signal as sigframe;
use crate::kprintln;
use crate::process;
use crate::syscall::thread;
use crate::usercopy::{copy_from_user, copy_to_user};

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
     * register. Current-EL frames carry SP_ELx there and must leave SP_EL0 alone.
     *
     * The return state is loaded once and then installed into ELR_EL1/SPSR_EL1 before the GPRs
     * are popped. Mask IRQs for the whole window: a syscall handler runs with interrupts enabled,
     * and a timer IRQ taken after the `msr ELR_EL1`/`msr SPSR_EL1` above would run this same
     * shared path for the nested current-EL frame, overwriting ELR_EL1/SPSR_EL1 with the
     * interrupted kernel PC and PSTATE. The outer path would then `eret` to that kernel PC,
     * re-enter itself and climb the stack by 288 bytes per pass until it faults at the stack top.
     * Masking here is safe: `eret` restores the interrupted DAIF from SPSR. */
__exception_restore:
    msr daifset, #2
    cbz x2, 1f
    ldr x9, [sp, #248]
    msr sp_el0, x9
    bl aarch64_restore_user_tls
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

/// Save the interrupted user TLS base into the current thread.
///
/// `TPIDR_EL0` is user-writable, and aarch64 libc installs its thread pointer with `msr
/// tpidr_el0` rather than `THREAD_SET_TLS`. Capturing the register on every lower-EL exception
/// keeps the kernel's per-thread copy current, so a later context switch does not resurrect a
/// stale base. The saved value is also what `THREAD_SET_TLS` updates before the return path
/// installs it.
fn save_user_tls() {
    let Some(thread) = crate::sched::current_thread() else {
        return;
    };
    if !thread.user {
        return;
    }
    let base: u64;
    // SAFETY: TPIDR_EL0 is the user TLS base and is readable at EL1.
    unsafe {
        asm!(
            "mrs {}, tpidr_el0",
            out(reg) base,
            options(nomem, nostack, preserves_flags),
        );
    }
    thread.set_tls_base(base);
}

/// Install the current thread's TLS base in `TPIDR_EL0` before returning to EL0.
///
/// Called from `__exception_restore` for lower-EL frames after `exception_dispatch` and any
/// context switch. `THREAD_SET_TLS` therefore takes effect for the caller on the syscall return
/// (S23 parity), and a direct user `msr tpidr_el0` survives the kernel round trip.
#[unsafe(no_mangle)]
pub extern "C" fn aarch64_restore_user_tls() {
    let Some(thread) = crate::sched::current_thread() else {
        return;
    };
    if !thread.user {
        return;
    }
    let base = thread.tls_base();
    // SAFETY: EL1 may write TPIDR_EL0; the value is the current user thread's saved base.
    unsafe {
        asm!(
            "msr tpidr_el0, {}",
            in(reg) base,
            options(nomem, nostack, preserves_flags),
        );
    }
}

/// Rust side of every vector slot.
///
/// IRQ slots dispatch through [`crate::arch::irq`] and return to the vector's restore path. The
/// lower-EL synchronous slot handles `svc #0` (EC 0x15) and user faults; the current-EL
/// synchronous slot checks the usercopy fixup table before treating a data abort as fatal. Every
/// unhandled slot prints the trap frame and exits the test with failure.
#[unsafe(no_mangle)]
pub extern "C" fn exception_dispatch(slot: u64, frame: *mut TrapFrame) {
    // Lower-EL slots enter with the user's `TPIDR_EL0` live; capture it before any handler can
    // switch threads or use the registered signal context area.
    if slot >= 8 {
        save_user_tls();
    }
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
                crate::process::page_fault(frame.far, frame.elr)
            }
            EC_IABT_LOWER => {
                let kind = abort_kind(data_abort(esr.iss()).dfsc);
                if deliver_fault(frame, kind, frame.far) {
                    return;
                }
                crate::process::page_fault(frame.far, frame.elr)
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

/// ELR after the sigcode's `svc #0`: `mov x8, #SIGRETURN` (4 bytes) + `svc` (4 bytes).
const SIGCODE_RETURN_ELR: u64 = crate::process::SIGCODE_BASE + 8;
/// NetBSD `_UC_SETSTACK`: the interrupted context was already on the signal stack.
const UC_SETSTACK: u32 = 0x0001_0000;
/// NetBSD `_UC_CLRSTACK`: the interrupted context was not on the signal stack.
const UC_CLRSTACK: u32 = 0x0002_0000;
/// `Q0..Q31` bytes in the kernel's per-thread FP/SIMD area.
const FPU_Q_BYTES: usize = 32 * 16;
/// `FPSR` offset in the kernel's per-thread FP/SIMD area.
const FPU_FPSR_OFFSET: usize = FPU_Q_BYTES;
/// `FPCR` offset in the kernel's per-thread FP/SIMD area.
const FPU_FPCR_OFFSET: usize = FPU_Q_BYTES + 8;

/// Scratch context for one delivery; a 4384-byte local per call would overflow a 20 KiB kernel
/// stack once the SVC and delivery frames are nested.
static SIGNAL_CONTEXT: spin::Mutex<MContext> = spin::Mutex::new(MContext::zero());

/// Scratch C19 frame for one delivery/return; the 0x410-byte frame stays off the kernel stack.
static SIGNAL_FRAME_LAYOUT: spin::Mutex<sigframe::SigFrameLayout> =
    spin::Mutex::new(sigframe::SigFrameLayout::ZERO);

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

/// Restore a `SIGRETURN` frame.
///
/// A kernel-built handler returns through the sigcode to this syscall; ELR is the instruction
/// after the sigcode's `svc` and `x30` still holds the sigcode base the kernel entered with. The
/// frame is therefore derived and validated from the trap frame itself, so nested frames restore
/// independently and a user-forged `SIGRETURN` from ordinary code keeps the legacy fallback.
pub fn sigreturn_frame(frame: &mut TrapFrame) -> Result<(), Errno> {
    if frame.elr == SIGCODE_RETURN_ELR && frame.x[sigframe::REG_LR] == crate::process::SIGCODE_BASE
    {
        return sigreturn_kernel_frame(frame);
    }
    let mut context = SIGNAL_CONTEXT.lock();
    thread::sigreturn(&mut context)?;
    restore_signal_context(frame, &context);
    Ok(())
}

#[allow(clippy::indexing_slicing)]
fn context_from_kernel_frame(frame: &TrapFrame, context: &mut sigframe::MContextLayout) {
    context.gregs = [0; sigframe::NGREG];
    context.gregs[..31].copy_from_slice(&frame.x);
    context.gregs[sigframe::REG_SP] = frame.sp;
    context.gregs[sigframe::REG_ELR] = frame.elr;
    context.gregs[sigframe::REG_SPSR] = frame.spsr;
    context.gregs[sigframe::REG_TPIDR] = 0;
    context.fregs = sigframe::FpregsetLayout::ZERO;
    context.spare = [0; 8];
}

#[allow(clippy::indexing_slicing)]
fn restore_kernel_context(frame: &mut TrapFrame, context: &sigframe::MContextLayout) {
    frame.x.copy_from_slice(&context.gregs[..31]);
    frame.sp = context.gregs[sigframe::REG_SP];
    frame.elr = context.gregs[sigframe::REG_ELR];
    frame.spsr = context.gregs[sigframe::REG_SPSR] & PSTATE_USER_MASK;
    frame.esr = 0;
    frame.far = 0;
}

/// Read the C19 `ucontext_t` back from the frame the handler returned from and restore it.
#[allow(clippy::indexing_slicing)]
fn sigreturn_kernel_frame(frame: &mut TrapFrame) -> Result<(), Errno> {
    let thread = crate::sched::current_thread().ok_or(Errno::ESRCH)?;
    if !thread.user {
        return Err(Errno::EINVAL);
    }
    // The handler was entered with SP at the frame base; validate the whole frame lies inside
    // the user window before any byte is read.
    let fp = frame.sp;
    if fp & 0xf != 0 || !is_user_address(fp) {
        return Err(Errno::EFAULT);
    }
    let frame_size = sigframe::SIGFRAME_SIZE as u64;
    let frame_end = fp.checked_add(frame_size).ok_or(Errno::EFAULT)?;
    if !is_user_address(frame_end.saturating_sub(1)) {
        return Err(Errno::EFAULT);
    }
    let uc = fp
        .checked_add(sigframe::SIGFRAME_UCONTEXT_OFFSET as u64)
        .ok_or(Errno::EFAULT)?;
    if uc != frame.x[sigframe::ARG_UCONTEXT_CALLEE] || !is_user_address(uc) {
        return Err(Errno::EFAULT);
    }
    let mut scratch = SIGNAL_FRAME_LAYOUT.lock();
    scratch.ucontext = sigframe::UContextLayout::ZERO;
    let bytes = unsafe {
        // SAFETY: the scratch ucontext is a live POD mirror; the mutable byte view covers
        // exactly its size and the copy fills it before it is read. `UserSlice` bounds the user
        // range and `copy_from_user` fails on an unmapped or read-protected page.
        core::slice::from_raw_parts_mut(
            &mut scratch.ucontext as *mut sigframe::UContextLayout as *mut u8,
            sigframe::UCONTEXT_SIZE,
        )
    };
    copy_from_user(bytes, UserSlice::new(uc, sigframe::UCONTEXT_SIZE as u64)?)?;
    let mask = u64::from(scratch.ucontext.uc_sigmask[0])
        | (u64::from(scratch.ucontext.uc_sigmask[1]) << 32);
    let unblockable = crate::process::signal_bit(crate::process::status::SIGKILL)
        | crate::process::signal_bit(crate::process::status::SIGSTOP);
    thread
        .sig_blocked
        .store(mask & !unblockable, Ordering::Release);
    // NetBSD `setucontext`: restore the altstack state the frame recorded before its delivery,
    // so a nested normal-stack frame cannot clear an outer handler's `SS_ONSTACK`.
    thread.sig_on_altstack.store(
        scratch.ucontext.uc_flags & UC_SETSTACK != 0,
        Ordering::Release,
    );
    restore_frame_fpu(&scratch.ucontext, &thread);
    restore_kernel_context(frame, &scratch.ucontext.uc_mcontext);
    Ok(())
}

/// Copy the frame's `__fregs` back into the thread's FP/SIMD area and load it.
///
/// The aarch64 kernel is soft-float and has no syscall parking path, so the live registers still
/// hold the handler's values here; they must be replaced with the interrupted image before the
/// vector returns to EL0.
#[allow(clippy::indexing_slicing)]
fn restore_frame_fpu(context: &sigframe::UContextLayout, thread: &crate::sched::thread::Thread) {
    let area = thread.fpu_area_ptr();
    // SAFETY: `area` is the live 528-byte thread save area and `context.fregs.q` is 512 bytes.
    unsafe {
        core::ptr::copy_nonoverlapping(
            context.uc_mcontext.fregs.q.as_ptr() as *const u8,
            area,
            FPU_Q_BYTES,
        );
        let fpsr = u64::from(context.uc_mcontext.fregs.fpsr);
        let fpcr = u64::from(context.uc_mcontext.fregs.fpcr);
        core::ptr::copy_nonoverlapping(fpsr.to_le_bytes().as_ptr(), area.add(FPU_FPSR_OFFSET), 8);
        core::ptr::copy_nonoverlapping(fpcr.to_le_bytes().as_ptr(), area.add(FPU_FPCR_OFFSET), 8);
        crate::arch::aarch64::fpu::restore(thread.fpu_area_const_ptr());
    }
}

/// Park the interrupted FP/SIMD state into the frame.
#[allow(clippy::indexing_slicing)]
fn save_frame_fpu(layout: &mut sigframe::SigFrameLayout, thread: &crate::sched::thread::Thread) {
    // SAFETY: the delivery path runs on the current thread's CPU, no other context owns the
    // FP/SIMD registers, and the thread's save area is live.
    unsafe {
        crate::arch::aarch64::fpu::save(thread.fpu_area_ptr());
    }
    let area = thread.fpu_area_const_ptr();
    // SAFETY: `area` is the live 528-byte thread save area; the destination is the frame's
    // 512-byte `__qregs` array and the offsets are the architectural layout.
    unsafe {
        core::ptr::copy_nonoverlapping(
            area,
            layout.ucontext.uc_mcontext.fregs.q.as_mut_ptr() as *mut u8,
            FPU_Q_BYTES,
        );
        let mut word = [0u8; 8];
        core::ptr::copy_nonoverlapping(area.add(FPU_FPSR_OFFSET), word.as_mut_ptr(), 8);
        layout.ucontext.uc_mcontext.fregs.fpsr =
            u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
        core::ptr::copy_nonoverlapping(area.add(FPU_FPCR_OFFSET), word.as_mut_ptr(), 8);
        layout.ucontext.uc_mcontext.fregs.fpcr =
            u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
    }
}

/// Build and enter a C19 aarch64 `sendsig` frame for the oldest deliverable kernel signal.
///
/// Returns false when no kernel `sigaction` handler is pending, so the caller can fall back to
/// the retired `SIG_SETUP` entry while libc still registers it.
#[allow(clippy::indexing_slicing)]
fn deliver_kernel_signal(frame: &mut TrapFrame) -> bool {
    let Some(process) = process::current() else {
        return false;
    };
    let Some(thread) = crate::sched::current_thread() else {
        return false;
    };
    if !thread.user {
        return false;
    }
    let blocked = thread.sig_blocked.load(Ordering::Acquire);
    let Some((record, action)) = process.next_kernel_signal(blocked) else {
        return false;
    };
    let old_mask = blocked;
    let mut new_mask = old_mask | action.mask;
    if action.flags & crate::process::SA_NODEFER == 0 {
        new_mask |= crate::process::signal_bit(record.signo);
    }
    let unblockable = crate::process::signal_bit(crate::process::status::SIGKILL)
        | crate::process::signal_bit(crate::process::status::SIGSTOP);
    thread
        .sig_blocked
        .store(new_mask & !unblockable, Ordering::Release);
    let alt_sp = thread.sig_alt_sp.load(Ordering::Acquire);
    let alt_size = thread.sig_alt_size.load(Ordering::Acquire);
    let alt_flags = thread.sig_alt_flags.load(Ordering::Acquire);
    let was_on_altstack = thread.sig_on_altstack.load(Ordering::Acquire);
    let on_altstack = action.flags & crate::process::SA_ONSTACK != 0
        && alt_flags & crate::process::SS_DISABLE == 0
        && alt_sp != 0
        && !was_on_altstack;
    let stack_top = if on_altstack {
        match alt_sp.checked_add(alt_size) {
            Some(top) => top,
            None => process::exit_process(Errno::EFAULT.to_ret() as i32),
        }
    } else {
        frame.sp
    };
    let Some(place) = sigframe::frame(stack_top) else {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    };
    let mut layout = SIGNAL_FRAME_LAYOUT.lock();
    *layout = sigframe::SigFrameLayout::ZERO;
    // NetBSD `sendsig`: record the altstack state from before this delivery so `SIGRETURN`
    // restores an outer frame's `SS_ONSTACK` after a nested normal-stack frame.
    layout.ucontext.uc_flags = if was_on_altstack {
        UC_SETSTACK
    } else {
        UC_CLRSTACK
    };
    layout.siginfo.si_signo = record.signo as i32;
    layout.siginfo.si_code = record.code as i32;
    layout.siginfo.si_pid = record.sender as i32;
    layout.siginfo.si_uid = record.uid;
    if record.code == crate::process::SI_QUEUE_CODE {
        layout.siginfo.si_value = record.value;
    }
    layout.ucontext.uc_sigmask[0] = old_mask as u32;
    layout.ucontext.uc_sigmask[1] = (old_mask >> 32) as u32;
    layout.ucontext.uc_stack = sigframe::StackLayout {
        ss_sp: if on_altstack { alt_sp } else { 0 },
        ss_size: alt_size,
        ss_flags: if on_altstack {
            crate::process::SS_ONSTACK
        } else {
            0
        },
        spare: 0,
    };
    context_from_kernel_frame(frame, &mut layout.ucontext.uc_mcontext);
    save_frame_fpu(&mut layout, &thread);
    let size = core::mem::size_of::<sigframe::SigFrameLayout>();
    let bytes = unsafe {
        // SAFETY: the scratch frame is a live, fully initialised POD frame; the byte view
        // covers exactly its size and is only read for the copy.
        core::slice::from_raw_parts(
            &*layout as *const sigframe::SigFrameLayout as *const u8,
            size,
        )
    };
    let Ok(destination) = UserSlice::new(place.sp, size as u64) else {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    };
    if copy_to_user(destination, bytes).is_err() {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    }
    drop(layout);
    if action.flags & crate::process::SA_RESETHAND != 0 {
        process
            .sigacts
            .lock()
            .set(record.signo, crate::process::SigAction::default_action());
    }
    if on_altstack {
        thread.sig_on_altstack.store(true, Ordering::Release);
    }
    frame.x[sigframe::ARG_SIG] = u64::from(record.signo);
    frame.x[sigframe::ARG_SIGINFO] = place.siginfo;
    frame.x[sigframe::ARG_UCONTEXT] = place.ucontext;
    frame.x[sigframe::ARG_UCONTEXT_CALLEE] = place.ucontext;
    frame.x[sigframe::REG_LR] = crate::process::SIGCODE_BASE;
    frame.elr = action.handler;
    frame.sp = place.sp;
    true
}

/// Map a data/instruction abort status code to the C58 fault kind.
fn abort_kind(dfsc: u8) -> u32 {
    match dfsc {
        0x09..=0x0b | 0x0d..=0x0f => signal::FAULT_PAGE_PERM,
        0x21 => signal::FAULT_MISALIGN,
        _ => signal::FAULT_PAGE_NOMAP,
    }
}

/// Enter the registered entry with `bits` in `x0`, the optional fault record
/// pointer in `x1` and the current user signal mask in `x2`.
fn enter_signal(
    frame: &mut TrapFrame,
    due: &thread::SignalEntry,
    bits: u64,
    second: u64,
    mask: u64,
) {
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
    if let Some(argument) = frame.x.get_mut(2) {
        *argument = mask;
    }
    frame.elr = due.entry;
    frame.sp = stack;
}

/// Deliver a pending `SIGRESEND` or kick to the registered user entry.
///
/// Called after the `svc` result is in `x0` and before the vector restores the
/// EL0 frame, so the saved context is the interrupted user state.
pub fn deliver_signal(frame: &mut TrapFrame) {
    if deliver_kernel_signal(frame) {
        return;
    }
    if let Some(due) = thread::signal_resend_due() {
        let fault_pointer = {
            let mut context = SIGNAL_CONTEXT.lock();
            context_from_frame(frame, &mut context);
            thread::signal_commit(&due, &context, None)
        };
        enter_signal(frame, &due, u64::from(due.bits), fault_pointer, due.mask);
        return;
    }
    let Some(due) = thread::signal_due() else {
        return;
    };
    let fault_pointer = {
        let mut context = SIGNAL_CONTEXT.lock();
        context_from_frame(frame, &mut context);
        thread::signal_commit(&due, &context, None)
    };
    enter_signal(frame, &due, u64::from(due.bits), fault_pointer, due.mask);
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
    enter_signal(frame, &due, 0, fault_pointer, due.mask);
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
