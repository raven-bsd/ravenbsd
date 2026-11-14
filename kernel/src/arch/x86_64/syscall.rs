// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `syscall` MSR setup and the assembly entry stub.

use raven_abi::errno::Errno;
use raven_abi::signal::{self, ENTRY_SP, FaultRecord, MContext};
use raven_usercopy::is_user_address;

use crate::arch::x86_64::gdt::{KERNEL_CS, KERNEL_SS, STAR_USER_BASE, USER_CS, USER_SS};
use crate::arch::x86_64::trapframe::TrapFrame;
use crate::arch::x86_64::usermode::{percpu_address, percpu_init};
use crate::process;
use crate::syscall::thread;

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
    call syscall_dispatch

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

    /* Use r10 as the canonical-rip scratch (it is reloaded with the user rsp
     * below), so rdx, the third syscall argument / entry mask register, stays
     * in the return frame. */
    mov r10, rcx
    shr r10, 47
    cmp r10, 0
    je 1f
    cmp r10, 0x1ffff
    jne 2f
1:
    or r11, 0x200
    mov r10, [rsp + 24]
    mov rsp, r10
    swapgs
    sysretq
2:
    mov rdi, rcx
    call syscall_bad_rip
    ud2
    .size syscall_entry, . - syscall_entry
"#
);

unsafe extern "C" {
    fn syscall_entry();
}

/// Configure the syscall MSRs and per-CPU GS bases.
pub fn init() {
    percpu_init();
    let efer = rdmsr(IA32_EFER);
    wrmsr(IA32_EFER, efer | 1);
    let star = (u64::from(STAR_USER_BASE) << 48) | (u64::from(KERNEL_CS) << 32);
    wrmsr(IA32_STAR, star);
    wrmsr(IA32_LSTAR, syscall_entry as *const () as usize as u64);
    wrmsr(IA32_FMASK, SYSCALL_FMASK);
    wrmsr(IA32_GS_BASE, 0);
    wrmsr(IA32_KERNEL_GS_BASE, percpu_address());
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

/// Restore a `SIGRETURN` context from the registered user area into the frame.
pub fn sigreturn_frame(frame: &mut TrapFrame) -> Result<(), Errno> {
    let mut context = SIGNAL_CONTEXT.lock();
    thread::sigreturn(&mut context)?;
    restore_signal_context(frame, &context);
    Ok(())
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
