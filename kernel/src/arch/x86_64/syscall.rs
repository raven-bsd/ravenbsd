// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `syscall` MSR setup and the assembly entry stub.

use crate::arch::x86_64::gdt::{KERNEL_CS, KERNEL_SS, STAR_USER_BASE, USER_CS, USER_SS};
use crate::arch::x86_64::usermode::{percpu_address, percpu_init};

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
    mov r10, [rsp + 24]

    mov rdx, rcx
    shr rdx, 47
    cmp rdx, 0
    je 1f
    cmp rdx, 0x1ffff
    jne 2f
1:
    or r11, 0x200
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
}

#[unsafe(no_mangle)]
extern "C" fn syscall_bad_rip(rip: u64) -> ! {
    crate::process::bad_sysret(rip)
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
