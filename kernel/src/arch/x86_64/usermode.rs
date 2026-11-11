// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-CPU state and the ring-3 entry path.

use core::cell::UnsafeCell;
use core::mem::offset_of;

use raven_rmm::PhysAddr;

use crate::arch::x86_64::gdt::{USER_CS, USER_SS};

/// Per-CPU data reachable through `GS` after `swapgs` on a user-to-kernel transition.
#[repr(C)]
pub struct PerCpu {
    /// Pointer to this structure (offset 0).
    pub self_ptr: u64,
    /// Top of the current thread's kernel stack (offset 8).
    pub kernel_rsp: u64,
    /// Scratch slot for the user stack pointer on syscall entry (offset 16).
    pub user_rsp_scratch: u64,
    /// Current thread pointer (offset 24; diagnostic / S09).
    pub current_thread: u64,
}

impl PerCpu {
    const fn new() -> Self {
        Self {
            self_ptr: 0,
            kernel_rsp: 0,
            user_rsp_scratch: 0,
            current_thread: 0,
        }
    }
}

struct PerCpuCell(UnsafeCell<PerCpu>);

// SAFETY: this is the boot processor's legacy GS block, written by the S08 syscall setup and by
// the context code before ring 3.  The scheduler installs the real per-CPU GS block from
// `ap_start` on every CPU; user threads and ring-3 transitions run only on CPU 0 under the current
// execution model, so application processors never read or write this block.  A future
// user-thread-SMP change must give every CPU its own block.
unsafe impl Sync for PerCpuCell {}

static PERCPU: PerCpuCell = PerCpuCell(UnsafeCell::new(PerCpu::new()));

const _: () = {
    assert!(offset_of!(PerCpu, self_ptr) == 0);
    assert!(offset_of!(PerCpu, kernel_rsp) == 8);
    assert!(offset_of!(PerCpu, user_rsp_scratch) == 16);
    assert!(offset_of!(PerCpu, current_thread) == 24);
};

/// Initialise the per-CPU self pointer.
pub fn percpu_init() {
    let pointer = PERCPU.0.get();
    // SAFETY: the boot processor calls this before AP bring-up; no other reader exists yet.
    unsafe {
        (*pointer).self_ptr = pointer as u64;
    }
}

/// Address of the per-CPU block for `IA32_KERNEL_GS_BASE`.
pub fn percpu_address() -> u64 {
    PERCPU.0.get() as u64
}

/// Update the kernel stack pointer used by the syscall entry stub.
pub fn set_kernel_rsp(value: u64) {
    // SAFETY: single-threaded kernel setup before entering user mode.
    unsafe {
        (*PERCPU.0.get()).kernel_rsp = value;
    }
}

/// Record the current thread pointer (diagnostic and S09).
pub fn set_current_thread(value: u64) {
    // SAFETY: single-threaded kernel setup before entering user mode.
    unsafe {
        (*PERCPU.0.get()).current_thread = value;
    }
}

/// Enter ring 3.
///
/// `root` must be the process address space whose upper half contains the kernel mappings.
pub fn enter(entry: u64, stack: u64, kernel_stack_top: u64, root: PhysAddr) -> ! {
    crate::arch::x86_64::gdt::set_rsp0(kernel_stack_top);
    set_kernel_rsp(kernel_stack_top);
    // SAFETY: all kernel mappings are present in the user root's upper half, the TSS rsp0 and
    // per-CPU kernel stack are installed, and the iret frame is built in registers/stack below.
    unsafe {
        core::arch::asm!(
            "mov cr3, {}",
            in(reg) root.0,
            options(nostack),
        );
        core::arch::asm!(
            "push {ss}",
            "push {sp}",
            "push 0x202",
            "push {cs}",
            "push {entry}",
            "mov rdi, {sp}",
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
            options(noreturn),
        );
    }
}
