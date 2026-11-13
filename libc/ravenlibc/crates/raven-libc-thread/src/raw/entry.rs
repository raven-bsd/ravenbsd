// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The per-architecture thread entry stub (CLEANROOM row 38).
//!
//! The stub receives one argument: the address of `StartArgs`. The first word of `StartArgs` is
//! the Rust trampoline pointer (`thread_start`); the stub aligns the stack to 16 bytes, clears
//! the frame pointer and jumps through that pointer with the argument register unchanged. The
//! trampoline never returns, so the discarded return address is harmless. No Linux `clone`
//! convention and no external symbol is used.

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .text
    .globl __rl_thread_entry
    .hidden __rl_thread_entry
    .type __rl_thread_entry,@function
__rl_thread_entry:
    and rsp, -16
    xor ebp, ebp
    jmp qword ptr [rdi]
    .size __rl_thread_entry, . - __rl_thread_entry
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .text
    .globl __rl_thread_entry
    .hidden __rl_thread_entry
    .type __rl_thread_entry,%function
__rl_thread_entry:
    ldr x16, [x0]
    mov x17, sp
    and x17, x17, #-16
    mov sp, x17
    mov x29, xzr
    br x16
    .size __rl_thread_entry, . - __rl_thread_entry
"#
);

#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(
    r#"
    .text
    .globl __rl_thread_entry
    .hidden __rl_thread_entry
    .type __rl_thread_entry,%function
__rl_thread_entry:
    ld t0, 0(a0)
    andi sp, sp, -16
    mv s0, zero
    jr t0
    .size __rl_thread_entry, . - __rl_thread_entry
"#
);

/// The address of the entry stub, for `CreateArgs.entry`.
#[must_use]
pub fn entry_address() -> usize {
    unsafe extern "C" {
        fn __rl_thread_entry();
    }
    (__rl_thread_entry as unsafe extern "C" fn()) as usize
}

#[cfg(test)]
#[path = "entry_ut.rs"]
mod tests;
