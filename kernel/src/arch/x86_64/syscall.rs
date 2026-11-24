// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `syscall` MSR setup and the assembly entry stub.

use core::sync::atomic::Ordering;
use raven_abi::errno::Errno;
use raven_abi::signal::{self, ENTRY_SP, FaultRecord, MContext};
use raven_usercopy::{UserSlice, is_user_address};

use crate::arch::x86_64::gdt::{KERNEL_CS, KERNEL_SS, STAR_USER_BASE, USER_CS, USER_SS};
use crate::arch::x86_64::trapframe::TrapFrame;
use crate::arch::x86_64::usermode::{percpu_address, percpu_init};
use crate::process;
use crate::syscall::thread;
use crate::usercopy::{copy_from_user, copy_to_user};

const IA32_EFER: u32 = 0xC000_0080;
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_FMASK: u32 = 0xC000_0084;
const IA32_GS_BASE: u32 = 0xC000_0101;
const IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;

/// Clear IF, TF, DF and AC on syscall entry.
const SYSCALL_FMASK: u64 = 0x0000_0000_0004_0700;

core::arch::global_asm!(
    r#"
    .section .text
    .global syscall_entry
    .type syscall_entry, @function
syscall_entry:
    swapgs
    mov qword ptr gs:[16], rsp
    mov rsp, qword ptr gs:[8]

    push 0x1b
    push qword ptr gs:[16]
    push r11
    push 0x23
    push rcx
    push 0
    push 0

    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    mov rdi, rsp
    call sched_syscall_enter
    mov rdi, rsp
    call syscall_dispatch
    call sched_syscall_exit

    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    add rsp, 16

    mov rcx, [rsp]
    mov r11, [rsp + 16]

    /* Use r10 as the canonical-rip scratch, but preserve the user's r10 across
     * the check: it is a normal callee-saved-by-ABI register for the syscall
     * instruction and user code may keep live values in it. */
    push r10
    mov r10, rcx
    shr r10, 47
    cmp r10, 0
    je 1f
    cmp r10, 0x1ffff
    jne 2f
1:
    or r11, 0x200
    pop r10
    mov rsp, [rsp + 24]
    swapgs
    sysretq
2:
    pop r10
    mov rdi, rcx
    call syscall_bad_rip
    ud2
    .size syscall_entry, . - syscall_entry
"#
);

unsafe extern "C" {
    fn syscall_entry();
}

/// Configure the boot processor's syscall MSRs and per-CPU GS bases.
pub fn init() {
    percpu_init();
    init_msrs();
    wrmsr(IA32_GS_BASE, 0);
    wrmsr(IA32_KERNEL_GS_BASE, percpu_address());
}

/// Configure the syscall MSRs on an application processor.
///
/// `STAR`, `LSTAR`, `FMASK` and `EFER.SCE` are per-CPU state; an AP that enters ring 3 must
/// install them before its first user syscall.  The GS pair is already installed by `ap_start`'s
/// `install_gs`, and `enter_user` normalises it before ring 3.
pub fn init_cpu() {
    init_msrs();
}

/// Write `EFER.SCE`, `STAR`, `LSTAR`, `FMASK` and enable FSGSBASE on this CPU.
fn init_msrs() {
    let efer = rdmsr(IA32_EFER);
    wrmsr(IA32_EFER, efer | 1);
    let star = (u64::from(STAR_USER_BASE) << 48) | (u64::from(KERNEL_CS) << 32);
    wrmsr(IA32_STAR, star);
    wrmsr(IA32_LSTAR, syscall_entry as *const () as usize as u64);
    wrmsr(IA32_FMASK, SYSCALL_FMASK);
    enable_fsgsbase();
}

/// Enable user `rdfsbase`/`wrfsbase` when the CPU advertises FSGSBASE.
///
/// The TLS qtest uses `rdfsbase` to observe the base that the context switch
/// installed; the kernel's authoritative source remains the `Thread` field.
fn enable_fsgsbase() {
    let leaf = core::arch::x86_64::__cpuid_count(7, 0);
    if leaf.ebx & 1 == 0 {
        return;
    }
    let mut cr4: u64;
    // SAFETY: ring-0 CR4 read.
    unsafe {
        core::arch::asm!(
            "mov {}, cr4",
            out(reg) cr4,
            options(nomem, nostack, preserves_flags),
        );
    }
    if cr4 & (1 << 16) == 0 {
        cr4 |= 1 << 16;
        // SAFETY: FSGSBASE was advertised by CPUID; this only opens the
        // per-thread base instructions to user mode.
        unsafe {
            core::arch::asm!(
                "mov cr4, {}",
                in(reg) cr4,
                options(nostack, preserves_flags),
            );
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn syscall_bad_rip(rip: u64) -> ! {
    crate::process::bad_sysret(rip)
}

/// User-visible RFLAGS bits that `SIGRETURN` may restore. IOPL, NT, VM and the
/// reserved high bits are never taken from the user context; IF and bit 1 are
/// forced on because ring 3 cannot clear them on this kernel.
const USER_RFLAGS_MASK: u64 = 0x0000_0000_0024_0fd7;
/// RFLAGS value forced on return to user mode (bit 1 and IF).
const USER_RFLAGS_SET: u64 = 0x202;

/// Scratch context for one delivery; serialised because the copy is short and must not enlarge
/// the 20 KiB kernel stack by a 256-byte frame per call.
static SIGNAL_CONTEXT: spin::Mutex<MContext> = spin::Mutex::new(MContext::zero());

/// System V amd64 `stack_t` (24 bytes).
#[repr(C)]
#[derive(Clone, Copy)]
struct UserStack {
    ss_sp: u64,
    ss_flags: i32,
    _pad: i32,
    ss_size: u64,
}

/// System V amd64 `ucontext_t` (936 bytes; `uc_mcontext` is the C36 256-byte `MContext`).
#[repr(C)]
#[derive(Clone, Copy)]
struct UserUContext {
    uc_flags: u64,
    uc_link: u64,
    uc_stack: UserStack,
    uc_mcontext: MContext,
    uc_sigmask: [u64; 16],
    uc_fpregs: [u64; 64],
}

/// System V amd64 `siginfo_t` (128 bytes).
#[repr(C)]
#[derive(Clone, Copy)]
struct UserSigInfo {
    si_signo: i32,
    si_errno: i32,
    si_code: i32,
    _pad: i32,
    fields: [u8; 112],
}

/// NetBSD amd64 `struct sigframe_siginfo`: return address, `siginfo_t`, `ucontext_t`.
#[repr(C)]
#[derive(Clone, Copy)]
struct SigFrame {
    sf_ra: u64,
    sf_si: UserSigInfo,
    sf_uc: UserUContext,
}

const _: () = assert!(core::mem::size_of::<UserStack>() == 24);
const _: () = assert!(core::mem::size_of::<UserUContext>() == 936);
const _: () = assert!(core::mem::offset_of!(UserUContext, uc_stack) == 16);
const _: () = assert!(core::mem::offset_of!(UserUContext, uc_mcontext) == 40);
const _: () = assert!(core::mem::offset_of!(UserUContext, uc_sigmask) == 296);
const _: () = assert!(core::mem::size_of::<UserSigInfo>() == 128);
const _: () = assert!(core::mem::size_of::<SigFrame>() == 1072);
const _: () = assert!(core::mem::offset_of!(SigFrame, sf_si) == 8);
const _: () = assert!(core::mem::offset_of!(SigFrame, sf_uc) == 136);

const ZERO_UCONTEXT: UserUContext = UserUContext {
    uc_flags: 0,
    uc_link: 0,
    uc_stack: UserStack {
        ss_sp: 0,
        ss_flags: 0,
        _pad: 0,
        ss_size: 0,
    },
    uc_mcontext: MContext::zero(),
    uc_sigmask: [0; 16],
    uc_fpregs: [0; 64],
};

const ZERO_SIGINFO: UserSigInfo = UserSigInfo {
    si_signo: 0,
    si_errno: 0,
    si_code: 0,
    _pad: 0,
    fields: [0; 112],
};

impl UserSigInfo {
    /// Store one `u32` field in the `siginfo_t` union area.
    fn set_u32(&mut self, offset: usize, value: u32) {
        if let Some(slot) = self.fields.get_mut(offset..offset.saturating_add(4)) {
            slot.copy_from_slice(&value.to_le_bytes());
        }
    }

    /// Store one `u64` field in the `siginfo_t` union area.
    fn set_u64(&mut self, offset: usize, value: u64) {
        if let Some(slot) = self.fields.get_mut(offset..offset.saturating_add(8)) {
            slot.copy_from_slice(&value.to_le_bytes());
        }
    }
}

const ZERO_FRAME: SigFrame = SigFrame {
    sf_ra: 0,
    sf_si: ZERO_SIGINFO,
    sf_uc: ZERO_UCONTEXT,
};

/// Scratch frame for one delivery/return: keeping the 1072-byte `sigframe` off the kernel stack
/// follows the S44 archive's `MContext` scratch decision (the fault path already runs near the
/// stack guard). Delivery and `SIGRETURN` are short and serialised per CPU.
static SIGNAL_FRAME: spin::Mutex<SigFrame> = spin::Mutex::new(ZERO_FRAME);

/// Snapshot the interrupted user state in the C36 `mcontext_t` layout.
#[allow(clippy::indexing_slicing)] // Register indices are compile-time constants within `gregs`.
pub fn context_from_frame(frame: &TrapFrame, context: &mut MContext) {
    context.gregs = [0; 23];
    context.fpregs = 0;
    context.reserved = [0; 8];
    context.gregs[signal::REG_R8] = frame.r8;
    context.gregs[signal::REG_R9] = frame.r9;
    context.gregs[signal::REG_R10] = frame.r10;
    context.gregs[signal::REG_R11] = frame.r11;
    context.gregs[signal::REG_R12] = frame.r12;
    context.gregs[signal::REG_R13] = frame.r13;
    context.gregs[signal::REG_R14] = frame.r14;
    context.gregs[signal::REG_R15] = frame.r15;
    context.gregs[signal::REG_RDI] = frame.rdi;
    context.gregs[signal::REG_RSI] = frame.rsi;
    context.gregs[signal::REG_RBP] = frame.rbp;
    context.gregs[signal::REG_RBX] = frame.rbx;
    context.gregs[signal::REG_RDX] = frame.rdx;
    context.gregs[signal::REG_RAX] = frame.rax;
    context.gregs[signal::REG_RCX] = frame.rcx;
    context.gregs[signal::REG_RSP] = frame.rsp;
    context.gregs[signal::REG_RIP] = frame.rip;
    context.gregs[signal::REG_EFL] = frame.rflags;
    context.gregs[signal::REG_CSGSFS] = frame.cs & 0xffff;
    context.gregs[signal::REG_ERR] = frame.error_code;
    context.gregs[signal::REG_TRAPNO] = frame.vector;
}

/// Apply a `SIGRETURN` context to the user frame, forcing user segments and a
/// safe RFLAGS image.
#[allow(clippy::indexing_slicing)] // Register indices are compile-time constants within `gregs`.
pub fn restore_signal_context(frame: &mut TrapFrame, context: &MContext) {
    frame.r8 = context.gregs[signal::REG_R8];
    frame.r9 = context.gregs[signal::REG_R9];
    frame.r10 = context.gregs[signal::REG_R10];
    frame.r11 = context.gregs[signal::REG_R11];
    frame.r12 = context.gregs[signal::REG_R12];
    frame.r13 = context.gregs[signal::REG_R13];
    frame.r14 = context.gregs[signal::REG_R14];
    frame.r15 = context.gregs[signal::REG_R15];
    frame.rdi = context.gregs[signal::REG_RDI];
    frame.rsi = context.gregs[signal::REG_RSI];
    frame.rbp = context.gregs[signal::REG_RBP];
    frame.rbx = context.gregs[signal::REG_RBX];
    frame.rdx = context.gregs[signal::REG_RDX];
    frame.rax = context.gregs[signal::REG_RAX];
    frame.rcx = context.gregs[signal::REG_RCX];
    frame.rsp = context.gregs[signal::REG_RSP];
    frame.rip = context.gregs[signal::REG_RIP];
    frame.rflags = (context.gregs[signal::REG_EFL] & USER_RFLAGS_MASK) | USER_RFLAGS_SET;
    frame.cs = u64::from(USER_CS);
    frame.ss = u64::from(USER_SS);
    frame.error_code = 0;
    frame.vector = 0;
}

/// Restore a `SIGRETURN` frame.
///
/// A kernel-built handler returns through the sigcode to this syscall with `rsp = sf_ra + 8`
/// (the handler `ret` popped the return address), so the `sigframe` is at `rsp - 8` and the
/// `ucontext_t` starts at `fp + 136`. The retired `SIG_SETUP` model keeps its own fallback until
/// the crate removes it (C19 removes the registration).
pub fn sigreturn_frame(frame: &mut TrapFrame) -> Result<(), Errno> {
    if crate::sched::current_thread()
        .is_some_and(|thread| thread.sig_in_handler.load(Ordering::Acquire))
    {
        return sigreturn_kernel_frame(frame);
    }
    let mut context = SIGNAL_CONTEXT.lock();
    thread::sigreturn(&mut context)?;
    restore_signal_context(frame, &context);
    Ok(())
}

/// Read the C19 `ucontext_t` back from the active kernel `sigframe` and restore it.
fn sigreturn_kernel_frame(frame: &mut TrapFrame) -> Result<(), Errno> {
    let thread = crate::sched::current_thread().ok_or(Errno::ESRCH)?;
    let fp = thread.sig_frame.load(Ordering::Acquire);
    if fp == 0 || !is_user_address(fp) {
        return Err(Errno::EFAULT);
    }
    if frame.rsp != fp.checked_add(8).ok_or(Errno::EFAULT)? {
        return Err(Errno::EINVAL);
    }
    let address = fp
        .checked_add(core::mem::offset_of!(SigFrame, sf_uc) as u64)
        .ok_or(Errno::EFAULT)?;
    let mut scratch = SIGNAL_FRAME.lock();
    scratch.sf_uc = ZERO_UCONTEXT;
    let bytes = unsafe {
        // SAFETY: the scratch `ucontext_t` is a live POD mirror; the mutable byte view covers
        // exactly its size and the copy fills it before it is read.
        core::slice::from_raw_parts_mut(
            &mut scratch.sf_uc as *mut UserUContext as *mut u8,
            core::mem::size_of::<UserUContext>(),
        )
    };
    copy_from_user(
        bytes,
        UserSlice::new(address, core::mem::size_of::<UserUContext>() as u64)?,
    )?;
    let unblockable = process::signal_bit(crate::process::status::SIGKILL)
        | process::signal_bit(crate::process::status::SIGSTOP);
    thread.sig_blocked.store(
        scratch.sf_uc.uc_sigmask.first().copied().unwrap_or(0) & !unblockable,
        Ordering::Release,
    );
    thread.sig_in_handler.store(false, Ordering::Release);
    thread.sig_frame.store(0, Ordering::Release);
    thread.sig_on_altstack.store(false, Ordering::Release);
    restore_signal_context(frame, &scratch.sf_uc.uc_mcontext);
    Ok(())
}

/// Address of a `sigframe` below the AMD64 128-byte red zone, 8 modulo 16 as if after a `call`.
fn frame_address(stack_top: u64) -> Option<u64> {
    let below = stack_top.checked_sub(128)?;
    let base = below.checked_sub(core::mem::size_of::<SigFrame>() as u64)?;
    let fp = if base & 0xf == 8 {
        base
    } else {
        (base & !0xf).checked_sub(8)?
    };
    let end = fp.checked_add(core::mem::size_of::<SigFrame>() as u64 - 1)?;
    if fp & 0xf != 8 || end >= below || !is_user_address(fp) || !is_user_address(end) {
        return None;
    }
    Some(fp)
}

/// Build and enter a C19 `sigframe` for the oldest deliverable kernel signal.
///
/// Returns false when no kernel `sigaction` handler is pending, so the caller can fall back to
/// the retired `SIG_SETUP` entry while libc still registers it.
fn deliver_kernel_signal(frame: &mut TrapFrame) -> bool {
    let Some(process) = process::current() else {
        return false;
    };
    let Some(thread) = crate::sched::current_thread() else {
        return false;
    };
    if !thread.user || thread.sig_in_handler.load(Ordering::Acquire) {
        return false;
    }
    let blocked = thread.sig_blocked.load(Ordering::Acquire);
    let Some((record, action)) = process.next_kernel_signal(blocked) else {
        return false;
    };
    let mut context = SIGNAL_CONTEXT.lock();
    context_from_frame(frame, &mut context);
    let old_mask = blocked;
    let mut new_mask = old_mask | action.mask;
    if action.flags & process::SA_NODEFER == 0 {
        new_mask |= process::signal_bit(record.signo);
    }
    let unblockable = process::signal_bit(crate::process::status::SIGKILL)
        | process::signal_bit(crate::process::status::SIGSTOP);
    thread
        .sig_blocked
        .store(new_mask & !unblockable, Ordering::Release);
    let alt_sp = thread.sig_alt_sp.load(Ordering::Acquire);
    let alt_size = thread.sig_alt_size.load(Ordering::Acquire);
    let alt_flags = thread.sig_alt_flags.load(Ordering::Acquire);
    let on_altstack = action.flags & process::SA_ONSTACK != 0
        && alt_flags & crate::process::SS_DISABLE == 0
        && alt_sp != 0
        && !thread.sig_on_altstack.load(Ordering::Acquire);
    let stack_top = if on_altstack {
        match alt_sp.checked_add(alt_size) {
            Some(top) => top,
            None => process::exit_process(Errno::EFAULT.to_ret() as i32),
        }
    } else {
        frame.rsp
    };
    let Some(fp) = frame_address(stack_top) else {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    };
    let mut sigframe = SIGNAL_FRAME.lock();
    sigframe.sf_ra = crate::process::SIGCODE_BASE;
    sigframe.sf_si = ZERO_SIGINFO;
    sigframe.sf_uc = ZERO_UCONTEXT;
    sigframe.sf_si.si_signo = record.signo as i32;
    sigframe.sf_si.si_code = record.code as i32;
    sigframe.sf_si.set_u32(0, record.sender as u32);
    sigframe.sf_si.set_u32(4, record.uid);
    if record.code == process::SI_QUEUE_CODE {
        sigframe.sf_si.set_u64(8, record.value);
    }
    sigframe.sf_uc.uc_stack = UserStack {
        ss_sp: if on_altstack { alt_sp } else { 0 },
        ss_flags: if on_altstack {
            crate::process::SS_ONSTACK as i32
        } else {
            0
        },
        _pad: 0,
        ss_size: alt_size,
    };
    sigframe.sf_uc.uc_mcontext = *context;
    sigframe.sf_uc.uc_sigmask[0] = old_mask;
    let size = core::mem::size_of::<SigFrame>();
    let bytes = unsafe {
        // SAFETY: the scratch frame is a live, fully initialised POD frame; the byte view covers
        // exactly its size and is only read for the copy.
        core::slice::from_raw_parts(&*sigframe as *const SigFrame as *const u8, size)
    };
    let Ok(destination) = UserSlice::new(fp, size as u64) else {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    };
    if copy_to_user(destination, bytes).is_err() {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    }
    drop(sigframe);
    drop(context);
    if action.flags & process::SA_RESETHAND != 0 {
        process
            .sigacts
            .lock()
            .set(record.signo, process::SigAction::default_action());
    }
    thread.sig_in_handler.store(true, Ordering::Release);
    thread.sig_frame.store(fp, Ordering::Release);
    thread.sig_on_altstack.store(on_altstack, Ordering::Release);
    frame.rdi = u64::from(record.signo);
    frame.rsi = fp + core::mem::offset_of!(SigFrame, sf_si) as u64;
    frame.rdx = fp + core::mem::offset_of!(SigFrame, sf_uc) as u64;
    frame.rip = action.handler;
    frame.rsp = fp;
    true
}

/// Enter the registered entry with `bits` in `rdi`, the optional fault record
/// pointer in `rsi` and the current user signal mask in `rdx`.
fn enter_signal(
    frame: &mut TrapFrame,
    due: &thread::SignalEntry,
    bits: u64,
    second: u64,
    mask: u64,
) {
    let stack = if due.flags & ENTRY_SP != 0 {
        frame.rsp
    } else {
        match frame.rsp.checked_sub(128) {
            Some(stack) => stack & !0xf,
            None => process::exit_process(Errno::EFAULT.to_ret() as i32),
        }
    };
    if !is_user_address(stack) {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    }
    frame.rdi = bits;
    frame.rsi = second;
    frame.rdx = mask;
    frame.rip = due.entry;
    frame.rsp = stack;
}

/// Deliver a pending `SIGRESEND` or kick to the registered user entry.
///
/// Called after the syscall result is in the frame and before `sysretq`, so
/// the saved context is exactly the interrupted user state.
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
        pc: frame.rip as usize,
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
    crate::kprintln!("process: user fault kind={kind} status={status}");
    process::exit_current(status)
}

fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: ring-0 MSR read; the caller supplies architectural MSR numbers.
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags),
        );
    }
    (u64::from(high) << 32) | u64::from(low)
}

fn wrmsr(msr: u32, value: u64) {
    // SAFETY: ring-0 MSR write; the caller supplies architectural MSR numbers and values.
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nomem, nostack, preserves_flags),
        );
    }
}

const _: () = {
    assert!(KERNEL_CS == 0x08);
    assert!(KERNEL_SS == 0x10);
    assert!(USER_CS == 0x23);
    assert!(USER_SS == 0x1B);
    assert!(SYSCALL_FMASK & 0x200 != 0);
};

#[cfg(test)]
#[path = "syscall_ut.rs"]
mod tests;
