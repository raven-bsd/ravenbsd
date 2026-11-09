// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `_start` trampoline and `__raven_start`.
//!
//! The kernel/spawn enters a new thread with `sp0` in the first argument register and the C8 stack
//! at that pointer. The assembly aligns the stack and calls the Rust entry function.

#[cfg(not(test))]
use crate::Args;
#[cfg(not(test))]
use crate::args::parse_stack;

#[cfg(all(target_arch = "x86_64", not(test)))]
core::arch::global_asm!(
    ".global _start",
    "_start:",
    "mov %rsp, %rdi",
    "and $-16, %rsp",
    "xor %ebp, %ebp",
    "call __raven_start",
    "ud2",
    options(att_syntax),
);

#[cfg(all(target_arch = "aarch64", not(test)))]
core::arch::global_asm!(
    ".global _start",
    "_start:",
    "mov x0, sp",
    "and x0, x0, #0xfffffffffffffff0",
    "mov sp, x0",
    "bl __raven_start",
    "brk #0",
);

#[cfg(all(target_arch = "riscv64", not(test)))]
core::arch::global_asm!(
    ".global _start",
    "_start:",
    "mv a0, sp",
    "andi sp, sp, -16",
    "call __raven_start",
    "unimp",
);

#[cfg(not(test))]
unsafe extern "C" {
    fn __raven_main(args: Args) -> i32;
}

/// The Rust entry point called by `_start`.
///
/// # Safety
///
/// `sp0` must be the C8 process stack pointer built by the kernel or `spawn`. Only the assembly
/// `_start` trampoline calls this with that pointer.
#[cfg(not(test))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_start(sp0: *const usize) -> ! {
    // SAFETY: `_start` passes the C8 stack pointer in the argument register.
    let args = unsafe { parse_stack(sp0) };
    // SAFETY: `entry!` defines `__raven_main` with this exact C signature in the final program.
    let code = unsafe { __raven_main(args) };
    crate::sys::exit(code)
}
