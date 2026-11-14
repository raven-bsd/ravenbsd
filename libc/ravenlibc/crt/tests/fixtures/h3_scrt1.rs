// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! H3: the `Scrt1` register convention without a loader.
//!
//! The stub entry plays the loader of `donor-map/13` 3.4 step 17: it stores the entry `rsp`,
//! loads `0xfeed` into `rdx` (the loader's exit hook) and jumps into `Scrt1`'s `_start`. The
//! test `__raven_libc_start` then sees `rsi == 0xfeed`, `rdx == 0`, `rcx == 0` and
//! `rdi == entry rsp`.

#![no_std]

#[unsafe(no_mangle)]
pub static mut ENTRY_SP: usize = 0;

core::arch::global_asm!(
    ".text",
    ".globl stub_entry",
    ".type stub_entry,@function",
    "stub_entry:",
    "mov %rsp, ENTRY_SP(%rip)",
    "mov $0xfeed,%edx",
    "jmp _start",
    options(att_syntax),
);

fn exit(code: i32) -> ! {
    // SAFETY: Linux `exit_group` never returns.
    unsafe {
        core::arch::asm!("syscall", in("rax") 231usize, in("rdi") code as usize, options(noreturn));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_libc_start(
    sp0: *const usize,
    rtld_fini: usize,
    iplt_start: usize,
    iplt_end: usize,
) -> ! {
    if sp0 as usize != unsafe { ENTRY_SP } {
        exit(1);
    }
    if rtld_fini != 0xfeed {
        exit(2);
    }
    if iplt_start != 0 || iplt_end != 0 {
        exit(3);
    }
    exit(0)
}
