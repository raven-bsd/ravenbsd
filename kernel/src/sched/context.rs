// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Low-level context switching and the new-thread trampoline.
//!
//! `sched_switch` saves the System V callee-saved registers on the outgoing kernel stack and
//! publishes its stack pointer, then loads the incoming stack pointer and returns on it. A new
//! thread's stack is prepared with the same frame shape, but its return address is
//! `sched_thread_start`. No FP/SIMD state is saved: the kernel and phase-1 userland are soft-float
//! (DR-14), and CR0.TS is never set by the kernel (S05 leaves the boot value, with CR0.EM clear).

use core::sync::atomic::Ordering;

use raven_abi::errno::Errno;

use crate::arch::x86_64::ap_start;
use crate::arch::x86_64::gdt::{self, USER_CS, USER_SS};
use crate::sched::thread::Thread;
use crate::sync;

const IA32_GS_BASE: u32 = 0xC000_0101;
const IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;

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

unsafe extern "C" {
    fn sched_switch(saved_rsp: *mut u64, next_rsp: u64);
    fn sched_thread_start();
}

/// Build the initial kernel stack for a new thread and return its saved `rsp`.
///
/// The frame is `[r15 r14 r13 r12 rbp rbx return-address unused]`; the return address points at
/// `sched_thread_start`. The dummy qword keeps the trampoline's entry stack alignment equal to
/// the System V call convention (8 mod 16).
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

/// Switch from `from` to `to`.
///
/// # Safety
///
/// Both threads must be alive and owned by the caller for the duration of the call. The caller
/// must hold the scheduler critical section; the preemption counter and per-CPU pointers are
/// transferred here.
pub unsafe fn switch(from: &Thread, to: &Thread) {
    from.preempt_count
        .store(sync::preempt_count(), Ordering::SeqCst);
    // SAFETY: the scheduler critical section gives exclusive access to the CPU state.
    unsafe {
        sync::set_preempt_count(to.preempt_count.load(Ordering::SeqCst));
    }

    ap_start::set_kernel_rsp(to.kernel_stack_top);
    ap_start::set_current_thread(to as *const Thread as u64);
    // The single TSS belongs to the boot processor; application processors only run kernel
    // threads in S10 and never take a ring-3 to ring-0 transition.
    if crate::sched::smp::percpu::this_cpu() == 0 {
        gdt::set_rsp0(to.kernel_stack_top);
    }

    let current = read_cr3();
    if current != to.cr3 {
        write_cr3(to.cr3);
    }

    let next_rsp = to.saved_rsp();
    // SAFETY: the frame layout was prepared by `prepare_stack` or by a previous `sched_switch`;
    // the caller keeps both threads alive across the switch.
    unsafe {
        sched_switch(from.saved_rsp_slot(), next_rsp);
    }
}

/// Rust entry point of a newly switched-in thread.
#[unsafe(no_mangle)]
extern "C" fn sched_thread_entry() -> ! {
    thread_entry()
}

fn thread_entry() -> ! {
    let thread = match crate::sched::current_thread() {
        Some(thread) => thread,
        None => idle_forever(),
    };
    if thread.user {
        // SAFETY: the address space, kernel stack and per-CPU state are installed by `switch`;
        // the user range was validated before the thread was created.
        unsafe {
            enter_user(thread.user_entry, thread.user_stack, thread.arg);
        }
    }
    // Kernel threads run with interrupts enabled.
    // SAFETY: the timer and other gates are installed before the scheduler starts.
    unsafe {
        sync::enable_interrupts();
    }
    if let Some(entry) = thread.kernel_entry {
        entry();
    }
    crate::sched::exit_thread(0)
}

fn idle_forever() -> ! {
    loop {
        // SAFETY: `hlt` is ring-0 and interrupts are enabled by the caller's iret path.
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

/// Enter ring 3 on the current thread's initial user context.
///
/// # Safety
///
/// `entry` and `stack` must be valid user addresses in the active address space.
unsafe fn enter_user(entry: u64, stack: u64, arg: u64) -> ! {
    let percpu = ap_start::percpu_address(crate::sched::smp::percpu::this_cpu());
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

/// Read CR3.
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

/// True when CR0 has neither `EM` nor `TS` set.
///
/// S05/S08 leave the boot policy: no x87 emulation and no lazy task switch. S09 does not save
/// FP/SIMD state, so a TS/EM bit appearing later is a kernel bug and the scheduler refuses to
/// start.
pub(crate) fn fpu_state_ok() -> bool {
    let value: u64;
    // SAFETY: ring-0 CR0 read.
    unsafe {
        core::arch::asm!(
            "mov {}, cr0",
            out(reg) value,
            options(nomem, nostack, preserves_flags),
        );
    }
    value & ((1 << 2) | (1 << 3)) == 0
}

/// Read the current stack pointer.
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

/// Write CR3 (flushes non-global TLB entries).
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
