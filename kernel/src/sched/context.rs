// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Low-level context switching and the new-thread trampoline.
//!
//! `sched_switch` saves the callee-saved registers on the outgoing kernel stack and publishes its
//! stack pointer, then loads the incoming stack pointer and returns on it. A new thread's stack is
//! prepared with the same frame shape, but its return address is `sched_thread_start`.
//!
//! * x86-64 saves `rbx, rbp, r12..r15`, switches CR3/TSS/GS state, and saves/restores the
//!   per-thread x87/SSE state through `arch::x86_64::fpu`.
//! * aarch64 saves `x19..x30` (96 bytes, SP stays 16-byte aligned), publishes the thread pointer
//!   in `TPIDR_EL1`, and switches TTBR0 with a fixed per-process ASID. A user thread starts in
//!   EL0 through `eret` with `SP_EL0`/`ELR_EL1`/`SPSR_EL1`; kernel threads keep the empty user
//!   root. aarch64 FP/SIMD state is not part of this ticket.
//!
//! The x86-64 kernel is soft-float but user threads may execute x87/SSE; the state is eager and
//! per-thread (DR-14, PG-12).

use core::sync::atomic::Ordering;

use raven_abi::errno::Errno;

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::fpu;
#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::ap_start;
#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::fpu;
#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::gdt::{self, USER_CS, USER_SS};
use crate::sched::thread::Thread;
use crate::sync;

#[cfg(target_arch = "x86_64")]
const IA32_FS_BASE: u32 = 0xC000_0100;
#[cfg(target_arch = "x86_64")]
const IA32_GS_BASE: u32 = 0xC000_0101;
#[cfg(target_arch = "x86_64")]
const IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text

    /* rdi = &outgoing.saved_rsp, rsi = incoming.saved_rsp */
    .global sched_switch
    .type sched_switch, @function
sched_switch:
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15
    mov [rdi], rsp
    mov rsp, rsi
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx
    ret
    .size sched_switch, . - sched_switch

    .global sched_thread_start
    .type sched_thread_start, @function
sched_thread_start:
    and rsp, -16
    call sched_thread_entry
    ud2
    .size sched_thread_start, . - sched_thread_start
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text

    /* x0 = &outgoing.saved_sp, x1 = incoming.saved_sp
     *
     * The frame holds the twelve callee-saved registers x19..x30 in 96 bytes; every push/pop is
     * 16-byte aligned, so the AAPCS64 stack alignment holds across a switch. */
    .global sched_switch
    .type sched_switch, %function
sched_switch:
    sub sp, sp, #96
    stp x19, x20, [sp, #0]
    stp x21, x22, [sp, #16]
    stp x23, x24, [sp, #32]
    stp x25, x26, [sp, #48]
    stp x27, x28, [sp, #64]
    stp x29, x30, [sp, #80]
    mov x9, sp
    str x9, [x0]
    mov sp, x1
    ldp x19, x20, [sp, #0]
    ldp x21, x22, [sp, #16]
    ldp x23, x24, [sp, #32]
    ldp x25, x26, [sp, #48]
    ldp x27, x28, [sp, #64]
    ldp x29, x30, [sp, #80]
    add sp, sp, #96
    ret
    .size sched_switch, . - sched_switch

    /* New-thread trampoline: the prepared frame pops into this address with SP at the stack top
     * (16-byte aligned). x29/x30 are zeroed for clean frame-pointer walks. */
    .global sched_thread_start
    .type sched_thread_start, %function
sched_thread_start:
    mov x29, #0
    mov x30, #0
    bl sched_thread_entry
    brk #0
    .size sched_thread_start, . - sched_thread_start
"#
);

#[cfg(target_arch = "x86_64")]
unsafe extern "C" {
    fn sched_switch(saved_rsp: *mut u64, next_rsp: u64);
    fn sched_thread_start();
}

#[cfg(target_arch = "aarch64")]
unsafe extern "C" {
    fn sched_switch(saved_sp: *mut u64, next_sp: u64);
    fn sched_thread_start();
}

/// Build the initial kernel stack for a new thread and return its saved stack pointer.
///
/// x86-64: the frame is `[r15 r14 r13 r12 rbp rbx return-address unused]`; the return address
/// points at `sched_thread_start`. The dummy qword keeps the trampoline's entry stack alignment
/// equal to the System V call convention (8 mod 16).
#[cfg(target_arch = "x86_64")]
pub fn prepare_stack(top: u64) -> Result<u64, Errno> {
    let saved = top.checked_sub(64).ok_or(Errno::ENOMEM)?;
    let trampoline = sched_thread_start as *const () as u64;
    let frame = saved as *mut u64;
    // SAFETY: `saved..top` is inside the freshly allocated, page-aligned kernel stack.
    unsafe {
        core::ptr::write_unaligned(frame, 0); // r15
        core::ptr::write_unaligned(frame.add(1), 0); // r14
        core::ptr::write_unaligned(frame.add(2), 0); // r13
        core::ptr::write_unaligned(frame.add(3), 0); // r12
        core::ptr::write_unaligned(frame.add(4), 0); // rbp
        core::ptr::write_unaligned(frame.add(5), 0); // rbx
        core::ptr::write_unaligned(frame.add(6), trampoline);
        core::ptr::write_unaligned(frame.add(7), 0);
    }
    Ok(saved)
}

/// Build the initial kernel stack for a new thread and return its saved stack pointer.
///
/// aarch64: the frame is the twelve callee-saved registers `x19..x30` (96 bytes); `x30` is the
/// trampoline address and the rest are zero.
#[cfg(target_arch = "aarch64")]
pub fn prepare_stack(top: u64) -> Result<u64, Errno> {
    let saved = top.checked_sub(96).ok_or(Errno::ENOMEM)?;
    let trampoline = sched_thread_start as *const () as u64;
    let frame = saved as *mut u64;
    // SAFETY: `saved..top` is inside the freshly allocated, page-aligned kernel stack.
    unsafe {
        let mut index = 0usize;
        while index < 11 {
            core::ptr::write_unaligned(frame.add(index), 0); // x19..x29
            index = index.saturating_add(1);
        }
        core::ptr::write_unaligned(frame.add(11), trampoline); // x30
    }
    Ok(saved)
}

/// Account the user-mode interval that ends now, then clear the entry stamp.
fn account_user_time(thread: &Thread) {
    if !thread.user {
        return;
    }
    let start = thread.user_enter_ns.swap(0, Ordering::AcqRel);
    if start != 0 {
        let elapsed = crate::time::now_ns().saturating_sub(start);
        if elapsed != 0 {
            thread.process.user_ns.fetch_add(elapsed, Ordering::AcqRel);
        }
    }
}

/// Start a user-mode interval when the thread is about to run in ring 3.
fn start_user_time(thread: &Thread) {
    if thread.user {
        thread
            .user_enter_ns
            .store(crate::time::now_ns(), Ordering::Release);
    }
}

/// True while a user thread is inside a syscall (its FP state is parked).
#[cfg(target_arch = "x86_64")]
fn thread_in_syscall(thread: &Thread) -> bool {
    thread.user && thread.fpu_saved.load(Ordering::Acquire)
}

#[cfg(not(target_arch = "x86_64"))]
fn thread_in_syscall(_thread: &Thread) -> bool {
    false
}

/// Switch from `from` to `to`.
///
/// # Safety
///
/// Both threads must be alive and owned by the caller for the duration of the call. The caller
/// must hold the scheduler critical section; the preemption counter and per-CPU pointers are
/// transferred here.
#[cfg(target_arch = "x86_64")]
pub unsafe fn switch(from: &Thread, to: &Thread) {
    from.preempt_count
        .store(sync::preempt_count(), Ordering::SeqCst);
    // SAFETY: the scheduler critical section gives exclusive access to the CPU state.
    unsafe {
        sync::set_preempt_count(to.preempt_count.load(Ordering::SeqCst));
    }
    #[cfg(target_arch = "x86_64")]
    {
        crate::sched::smp::percpu::note_switch(to.user);
        // The per-CPU diagnostic follows the incoming thread: a blocked syscall thread keeps
        // `fpu_saved` set even after it leaves the CPU, so do not carry its marker over.
        crate::sched::smp::percpu::note_syscall(to.user && to.fpu_saved.load(Ordering::Acquire));
        crate::sched::smp::percpu::note_page_fault(false);
        crate::sched::smp::percpu::note_page_fault_phase(0);
        crate::sched::smp::percpu::note_wait_phase(0);
    }

    ap_start::set_kernel_rsp(to.kernel_stack_top);
    ap_start::set_current_thread(to as *const Thread as u64);
    // Every CPU has its own TSS since S75; install the incoming thread's kernel stack in the
    // ring-3 rsp0 slot of the CPU running the switch.
    gdt::set_rsp0(crate::sched::smp::percpu::this_cpu(), to.kernel_stack_top);

    let current = read_cr3();
    if current != to.cr3 {
        write_cr3(to.cr3);
    }

    // Install the incoming user thread's TLS base. Kernel threads always get
    // zero, so a stale user FS base never leaks into kernel execution.
    wrmsr(IA32_FS_BASE, if to.user { to.tls_base() } else { 0 });

    // Save the outgoing live FP/SIMD state unless the thread is inside a syscall: in that case
    // its user state is already parked in the thread area by `sched_syscall_enter`. Restore the
    // incoming state unless that thread is in a syscall too; its state is restored on syscall
    // exit. A new thread gets its FINIT/MXCSR defaults from `FpuArea::new`.
    let from_in_syscall = thread_in_syscall(from);
    if !from_in_syscall {
        account_user_time(from);
    }
    if !from_in_syscall {
        // SAFETY: the scheduler owns the CPU state and the thread's area is live.
        unsafe {
            fpu::save(from.fpu_area_ptr());
        }
    }
    let to_in_syscall = thread_in_syscall(to);
    if !to_in_syscall {
        start_user_time(to);
    }
    if !to_in_syscall {
        // SAFETY: same ownership; `to` is alive for the switch.
        unsafe {
            fpu::restore(to.fpu_area_const_ptr());
        }
    }

    let next_rsp = to.saved_rsp();
    // SAFETY: the frame layout was prepared by `prepare_stack` or by a previous `sched_switch`;
    // the caller keeps both threads alive across the switch.
    unsafe {
        sched_switch(from.saved_rsp_slot(), next_rsp);
    }
}

/// Switch from `from` to `to` (aarch64).
///
/// # Safety
///
/// Both threads must be alive and owned by the caller for the duration of the call. The caller
/// must hold the scheduler critical section with interrupts masked.
#[cfg(target_arch = "aarch64")]
pub unsafe fn switch(from: &Thread, to: &Thread) {
    from.preempt_count
        .store(sync::preempt_count(), Ordering::SeqCst);
    // SAFETY: the scheduler critical section gives exclusive access to the CPU state.
    unsafe {
        sync::set_preempt_count(to.preempt_count.load(Ordering::SeqCst));
    }
    let current = read_cr3();
    if current != to.cr3 {
        // User pages carry `nG`; a fixed per-process ASID avoids a full flush on every switch.
        // Kernel threads use ASID 0 with the empty TTBR0 root.
        let asid = if to.user {
            u16::try_from(to.process.id).unwrap_or(0) & 0xff
        } else {
            0
        };
        crate::arch::mmu::switch_ttbr0(to.cr3, asid);
    }
    crate::arch::percpu::set_current_thread(to as *const Thread as u64);

    // TPIDR_EL1 remains the kernel thread pointer; TPIDR_EL0 is the user TLS
    // base and must be zero for kernel threads.
    let tls_base = if to.user { to.tls_base() } else { 0 };
    // SAFETY: EL1 may write TPIDR_EL0; the value is either the user-selected TLS
    // base or zero for a kernel thread. Interrupts are masked by the scheduler.
    unsafe {
        core::arch::asm!(
            "msr tpidr_el0, {}",
            in(reg) tls_base,
            options(nomem, nostack, preserves_flags),
        );
    }

    // Park the outgoing thread's live Q0..Q31/FPSR/FPCR and install the incoming thread's copy.
    // The kernel is soft-float, so no FP/SIMD state has to survive the switch itself. A new
    // thread restores the reset FPCR/FPSR zeros from `FpuArea::new`.
    // SAFETY: the scheduler owns this CPU's FP/SIMD state and both areas are live across the
    // switch.
    unsafe {
        fpu::save(from.fpu_area_ptr());
        fpu::restore(to.fpu_area_const_ptr());
    }

    let next_sp = to.saved_rsp();
    // SAFETY: the frame layout was prepared by `prepare_stack` or by a previous `sched_switch`;
    // the caller keeps both threads alive across the switch.
    unsafe {
        sched_switch(from.saved_rsp_slot(), next_sp);
    }
}

/// Rust entry point of a newly switched-in x86-64 thread.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
extern "C" fn sched_thread_entry() -> ! {
    thread_entry()
}

#[cfg(target_arch = "x86_64")]
fn thread_entry() -> ! {
    // Copy the entry fields out and drop the `Arc<Thread>` clone before diverging: the
    // trampoline frame never returns (user threads enter ring 3 through it), so a live local
    // would leave the thread owning a reference to itself on its abandoned stack.  S36's
    // deferred reaping caches that stack for reuse, and the orphaned reference would keep every
    // exited thread's allocation alive (a measured ~200-byte leak per exit).
    let (user, user_entry, user_stack, arg, kernel_entry) = {
        let thread = match crate::sched::current_thread() {
            Some(thread) => thread,
            None => idle_forever(),
        };
        (
            thread.user,
            thread.user_entry,
            thread.user_stack,
            thread.arg,
            thread.kernel_entry,
        )
    };
    if user {
        // SAFETY: the address space, kernel stack and per-CPU state are installed by `switch`;
        // the user range was validated before the thread was created.
        unsafe {
            enter_user(user_entry, user_stack, arg);
        }
    }
    // Kernel threads run with interrupts enabled.
    // SAFETY: the timer and other gates are installed before the scheduler starts.
    unsafe {
        sync::enable_interrupts();
    }
    if let Some(entry) = kernel_entry {
        entry();
    }
    crate::sched::exit_thread(0)
}

/// Rust entry point of a newly switched-in aarch64 thread.
#[cfg(target_arch = "aarch64")]
#[unsafe(no_mangle)]
extern "C" fn sched_thread_entry() -> ! {
    // Drop the self-reference before diverging, as the x86-64 trampoline does: the abandoned
    // frame must not keep the thread's own `Arc` alive on a cached stack.
    let (user, user_entry, user_stack, arg, kernel_entry) = {
        let thread = match crate::sched::current_thread() {
            Some(thread) => thread,
            None => idle_forever(),
        };
        (
            thread.user,
            thread.user_entry,
            thread.user_stack,
            thread.arg,
            thread.kernel_entry,
        )
    };
    if user {
        // SAFETY: the address space, kernel stack and per-CPU state were installed by `switch`;
        // the user range was validated before the thread was created.
        unsafe {
            enter_user(user_entry, user_stack, arg);
        }
    }
    // SAFETY: the vector table and the GIC/timer are initialised before the scheduler starts.
    unsafe {
        sync::enable_interrupts();
    }
    if let Some(entry) = kernel_entry {
        entry();
    }
    crate::sched::exit_thread(0)
}

#[cfg(target_arch = "x86_64")]
fn idle_forever() -> ! {
    loop {
        // SAFETY: `hlt` is ring-0 and interrupts are enabled by the caller's iret path.
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

#[cfg(target_arch = "aarch64")]
fn idle_forever() -> ! {
    loop {
        // SAFETY: `wfi` is valid at EL1 and wakes on the timer IRQ.
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack, preserves_flags));
        }
    }
}

/// Enter ring 3 on the current thread's initial user context.
///
/// # Safety
///
/// `entry` and `stack` must be valid user addresses in the active address space.
#[cfg(target_arch = "x86_64")]
unsafe fn enter_user(entry: u64, stack: u64, arg: u64) -> ! {
    let percpu = ap_start::percpu_address(crate::sched::smp::percpu::this_cpu());
    crate::sched::smp::percpu::note_user_flags(0x202);
    let gs_base = rdmsr(IA32_GS_BASE);
    if gs_base == percpu {
        // The kernel arrived from ring 3 through a syscall/trap and is in swapped state.
        // SAFETY: ring-0 swapgs; the two states are the architectural kernel/user GS pairs.
        unsafe {
            core::arch::asm!("swapgs", options(nomem, nostack, preserves_flags));
        }
    }
    // Normalise the pair for the user half: GS_BASE = 0, KERNEL_GS_BASE = per-CPU block.
    if rdmsr(IA32_GS_BASE) != 0 {
        wrmsr(IA32_GS_BASE, 0);
    }
    if rdmsr(IA32_KERNEL_GS_BASE) != percpu {
        wrmsr(IA32_KERNEL_GS_BASE, percpu);
    }

    // SAFETY: the iret frame is built on the current kernel stack; the caller installed the
    // process CR3, TSS.rsp0 and per-CPU kernel stack.
    unsafe {
        core::arch::asm!(
            "push {ss}",
            "push {sp}",
            "push 0x202",
            "push {cs}",
            "push {entry}",
            "mov rdi, {arg}",
            "xor eax, eax",
            "xor ebx, ebx",
            "xor ecx, ecx",
            "xor edx, edx",
            "xor esi, esi",
            "xor ebp, ebp",
            "xor r8d, r8d",
            "xor r9d, r9d",
            "xor r10d, r10d",
            "xor r11d, r11d",
            "xor r12d, r12d",
            "xor r13d, r13d",
            "xor r14d, r14d",
            "xor r15d, r15d",
            "iretq",
            ss = in(reg) u64::from(USER_SS),
            sp = in(reg) stack,
            cs = in(reg) u64::from(USER_CS),
            entry = in(reg) entry,
            arg = in(reg) arg,
            options(noreturn),
        );
    }
}

/// Enter EL0 on the current thread's initial user context.
///
/// The kernel runs on SP_EL1, so the user stack and return state go into `SP_EL0`, `ELR_EL1` and
/// `SPSR_EL1`; `SPSR_EL1 = 0` selects `EL0t` with interrupts enabled. Every general-purpose
/// register except `x0` (the first-argument slot the C8 entry ignores) is cleared so no kernel
/// pointer leaks into user mode.
///
/// # Safety
///
/// `entry` and `stack` must be valid user addresses in the active address space.
#[cfg(target_arch = "aarch64")]
unsafe fn enter_user(entry: u64, stack: u64, arg: u64) -> ! {
    // SAFETY: the caller installed the process TTBR0 and the thread's kernel stack; the user
    // range was validated before thread creation.
    unsafe {
        core::arch::asm!(
            "msr sp_el0, {stack}",
            "msr elr_el1, {entry}",
            "msr spsr_el1, xzr",
            "isb",
            "mov x0, {arg}",
            "mov x1, xzr",
            "mov x2, xzr",
            "mov x3, xzr",
            "mov x4, xzr",
            "mov x5, xzr",
            "mov x6, xzr",
            "mov x7, xzr",
            "mov x8, xzr",
            "mov x9, xzr",
            "mov x10, xzr",
            "mov x11, xzr",
            "mov x12, xzr",
            "mov x13, xzr",
            "mov x14, xzr",
            "mov x15, xzr",
            "mov x16, xzr",
            "mov x17, xzr",
            "mov x18, xzr",
            "mov x19, xzr",
            "mov x20, xzr",
            "mov x21, xzr",
            "mov x22, xzr",
            "mov x23, xzr",
            "mov x24, xzr",
            "mov x25, xzr",
            "mov x26, xzr",
            "mov x27, xzr",
            "mov x28, xzr",
            "mov x29, xzr",
            "mov x30, xzr",
            "eret",
            stack = in(reg) stack,
            entry = in(reg) entry,
            arg = in(reg) arg,
            options(noreturn),
        );
    }
}

/// Read CR3.
#[cfg(target_arch = "x86_64")]
pub(crate) fn read_cr3() -> u64 {
    let value: u64;
    // SAFETY: ring-0 CR3 read.
    unsafe {
        core::arch::asm!(
            "mov {}, cr3",
            out(reg) value,
            options(nomem, nostack, preserves_flags),
        );
    }
    value
}

/// Read the active TTBR0 root (the ASID bits are masked off).
#[cfg(target_arch = "aarch64")]
pub(crate) fn read_cr3() -> u64 {
    let value: u64;
    // SAFETY: TTBR0_EL1 is readable at EL1.
    unsafe {
        core::arch::asm!(
            "mrs {}, ttbr0_el1",
            out(reg) value,
            options(nomem, nostack, preserves_flags),
        );
    }
    value & 0x0000_FFFF_FFFF_F000
}

/// Configure and verify the architectural FP/SIMD controls.
///
/// Clears CR0.EM/TS, sets MP and CR4.OSFXSR/OSXMMEXCPT, and enables XSAVE when the CPU supports
/// it. Returns false when SSE is absent, in which case the scheduler refuses to start.
#[cfg(target_arch = "x86_64")]
pub(crate) fn init_fpu() -> bool {
    fpu::init()
}

/// Enable FP/SIMD at EL0/EL1 on this CPU (`CPACR_EL1.FPEN`).
///
/// Per-CPU; every CPU that may run a user thread calls this before the scheduler admits one.
/// ARMv8-A requires FP/SIMD, so this reports success unconditionally (see
/// [`crate::arch::aarch64::fpu::init`]).
#[cfg(target_arch = "aarch64")]
pub(crate) fn init_fpu() -> bool {
    fpu::init()
}

/// Install `tp` in `IA32_FS_BASE` for the calling thread.
///
/// `THREAD_SET_TLS` must be effective immediately for the caller: a user thread may read `%fs:`
/// as soon as the syscall returns, without waiting for a context switch.
#[cfg(target_arch = "x86_64")]
pub(crate) fn install_tls_base(tp: u64) {
    wrmsr(IA32_FS_BASE, tp);
}

/// Read the current stack pointer.
#[cfg(target_arch = "x86_64")]
pub(crate) fn read_rsp() -> u64 {
    let value: u64;
    // SAFETY: reading rsp has no memory side effects.
    unsafe {
        core::arch::asm!(
            "mov {}, rsp",
            out(reg) value,
            options(nomem, nostack, preserves_flags),
        );
    }
    value
}

/// Save the current user thread's FP/SIMD state on the syscall entry path.
///
/// Called from `syscall_entry` after the general-purpose registers are on the kernel stack and
/// before any Rust syscall code runs. Kernel threads are ignored.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub extern "C" fn sched_syscall_enter(frame: *const crate::arch::x86_64::trapframe::TrapFrame) {
    crate::sched::smp::percpu::note_syscall(true);
    if !frame.is_null() {
        // SAFETY: the syscall stub passes its live trap frame in `rdi`.
        let flags = unsafe { (*frame).rflags };
        // SAFETY: the same live trap frame is valid; the stub still owns it for this call.
        let number = unsafe { (*frame).rax };
        crate::sched::smp::percpu::note_user_flags(flags);
        crate::sched::smp::percpu::note_syscall_number(number);
    }
    let Some(thread) = crate::sched::current_thread() else {
        return;
    };
    if !thread.user {
        return;
    }
    if !frame.is_null() {
        // SAFETY: the syscall stub passes its live trap frame in `rdi`; this call owns it.
        let number = unsafe { (*frame).rax };
        // SAFETY: `frame` is the live trap frame the syscall stub passed in `rdi`; the
        // enclosing `!frame.is_null()` check proves it is non-null, and the stub keeps the
        // frame alive for the duration of this call.
        let pc = unsafe { (*frame).rip };
        thread.process.last_syscall.store(number, Ordering::Relaxed);
        thread.process.last_pc.store(pc, Ordering::Relaxed);
        thread.process.in_syscall.fetch_add(1, Ordering::Relaxed);
    }
    account_user_time(&thread);
    if !thread.fpu_saved.swap(true, Ordering::AcqRel) {
        // SAFETY: the syscall entry path owns the CPU and the thread is current.
        unsafe {
            fpu::save(thread.fpu_area_ptr());
        }
    }
    // syscall entry masks IF. The memory, scheme IPC and futex syscalls can be long; keep only
    // those interruptible so a target CPU can take an SMP TLB IPI while the other syscall paths
    // stay non-preemptible. Router/wait-queue state uses IRQ-safe locks, so the wake graph
    // remains protected even with IF enabled here. `sched_syscall_exit` masks again before the
    // final swapgs/sysretq pair.
    if !frame.is_null() {
        // SAFETY: the syscall stub passes its live trap frame in `rdi`.
        let number = unsafe { (*frame).rax as usize };
        let interruptible = matches!(
            raven_abi::syscall::Syscall::from_usize(number),
            Some(
                raven_abi::syscall::Syscall::MAP
                    | raven_abi::syscall::Syscall::UNMAP
                    | raven_abi::syscall::Syscall::PROTECT
                    | raven_abi::syscall::Syscall::THREAD_CREATE
                    | raven_abi::syscall::Syscall::THREAD_EXIT
                    | raven_abi::syscall::Syscall::EXIT
                    | raven_abi::syscall::Syscall::FUTEX
                    | raven_abi::syscall::Syscall::SCHEME_REGISTER
                    | raven_abi::syscall::Syscall::OPEN
                    | raven_abi::syscall::Syscall::READ
                    | raven_abi::syscall::Syscall::WRITE
                    | raven_abi::syscall::Syscall::CLOSE
                    | raven_abi::syscall::Syscall::DUP
                    | raven_abi::syscall::Syscall::FSTAT
                    | raven_abi::syscall::Syscall::CALL
                    | raven_abi::syscall::Syscall::RECV
                    | raven_abi::syscall::Syscall::SEND
            )
        ) || matches!(
            number,
            crate::syscall::ipc::PREAD | crate::syscall::ipc::PWRITE | crate::syscall::ipc::SEEK
        );
        if interruptible {
            ap_start::enable_local_interrupts();
        }
    }
}

/// Restore the current user thread's FP/SIMD state on the syscall return path.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub extern "C" fn sched_syscall_exit() {
    crate::sched::smp::percpu::note_syscall(false);
    // Keep the swapgs/sysretq return path non-interruptible: after swapgs the GS pair is in the
    // user state and a kernel-mode interrupt would read the wrong per-CPU block.
    ap_start::disable_local_interrupts();
    let Some(thread) = crate::sched::current_thread() else {
        return;
    };
    if !thread.user {
        return;
    }
    let _ = thread
        .process
        .in_syscall
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_sub(1))
        });
    if thread.fpu_saved.swap(false, Ordering::AcqRel) {
        // SAFETY: the syscall return path owns the CPU and the thread is current.
        unsafe {
            fpu::restore(thread.fpu_area_const_ptr());
        }
    }
    start_user_time(&thread);
}

/// Read the current stack pointer.
#[cfg(target_arch = "aarch64")]
pub(crate) fn read_rsp() -> u64 {
    let value: u64;
    // SAFETY: reading SP has no memory side effects.
    unsafe {
        core::arch::asm!(
            "mov {}, sp",
            out(reg) value,
            options(nomem, nostack, preserves_flags),
        );
    }
    value
}

/// Write CR3 (flushes non-global TLB entries).
#[cfg(target_arch = "x86_64")]
fn write_cr3(value: u64) {
    // SAFETY: the value is a page-table root installed by the kernel.
    unsafe {
        core::arch::asm!(
            "mov cr3, {}",
            in(reg) value,
            options(nostack, preserves_flags),
        );
    }
}

#[cfg(target_arch = "x86_64")]
fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: ring-0 MSR read of an architectural address.
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

#[cfg(target_arch = "x86_64")]
fn wrmsr(msr: u32, value: u64) {
    // SAFETY: ring-0 MSR write of an architectural address.
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
