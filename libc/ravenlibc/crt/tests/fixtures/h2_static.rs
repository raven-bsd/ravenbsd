// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! H2: a static non-PIE program's `__raven_libc_start`.
//!
//! Linked with `crt1.o`; checks the C8 stack (argc, argv, envp) against what the Linux kernel
//! passed and the two `iplt` words against the linked `__rela_iplt_*` range.

#![no_std]

core::arch::global_asm!(
    ".weak __rela_iplt_start",
    ".weak __rela_iplt_end",
    ".text",
    ".globl __rl_test_iplt_start",
    ".type __rl_test_iplt_start,@function",
    "__rl_test_iplt_start:",
    "lea __rela_iplt_start(%rip),%rax",
    "ret",
    ".globl __rl_test_iplt_end",
    ".type __rl_test_iplt_end,@function",
    "__rl_test_iplt_end:",
    "lea __rela_iplt_end(%rip),%rax",
    "ret",
    options(att_syntax),
);

unsafe extern "C" {
    fn __rl_test_iplt_start() -> usize;
    fn __rl_test_iplt_end() -> usize;
}

fn exit(code: i32) -> ! {
    // SAFETY: Linux `exit_group` never returns.
    unsafe {
        core::arch::asm!("syscall", in("rax") 231usize, in("rdi") code as usize, options(noreturn));
    }
}

fn equals(pointer: *const u8, expected: &[u8]) -> bool {
    let mut index = 0usize;
    while index < expected.len() {
        // SAFETY: the C8 stack strings are NUL-terminated and `index` is inside `expected`.
        if unsafe { *pointer.add(index) } != expected[index] {
            return false;
        }
        index += 1;
    }
    // SAFETY: the byte after the expected text must be the NUL terminator.
    (unsafe { *pointer.add(index) }) == 0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_libc_start(
    sp0: *const usize,
    rtld_fini: usize,
    iplt_start: usize,
    iplt_end: usize,
) -> ! {
    if rtld_fini != 0 {
        exit(1);
    }
    // SAFETY: the helper reads the weak linker symbol at run time.
    if iplt_start != unsafe { __rl_test_iplt_start() } {
        exit(2);
    }
    // SAFETY: see above.
    if iplt_end != unsafe { __rl_test_iplt_end() } {
        exit(3);
    }
    // SAFETY: `sp0` is the C8 stack of this process.
    let argc = unsafe { *sp0 };
    if argc != 2 {
        exit(4);
    }
    let argv = sp0.wrapping_add(1) as *const *const u8;
    // SAFETY: the argv array is `argc + 1` words.
    if unsafe { *argv.add(argc) } != core::ptr::null() {
        exit(5);
    }
    // SAFETY: `argv[1]` is a C8 string.
    if !equals(unsafe { *argv.add(1) }, b"hello") {
        exit(6);
    }
    let envp = argv.wrapping_add(argc + 1);
    // SAFETY: the environment array follows the argv terminator.
    let first = unsafe { *envp };
    if first.is_null() {
        exit(7);
    }
    // SAFETY: the first environment entry is a C8 string.
    if !equals(first, b"H2ENV=1") {
        exit(8);
    }
    exit(0)
}
