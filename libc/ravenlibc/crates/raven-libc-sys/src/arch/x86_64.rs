// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! x86-64 raw system calls.
//!
// RavenBSD: copied from userland/rt/src/sys.rs (same project, BSD-3-Clause)
// RavenBSD: moved from userland/rt/src/sys.rs

use super::convention::X86_64 as CONVENTION;

/// Register holding the syscall number.
pub const NR_REG: &str = CONVENTION.nr_reg;
/// Argument registers 0..5.
pub const ARG_REGS: [&str; 6] = CONVENTION.arg_regs;
/// Register holding the result.
pub const RET_REG: &str = CONVENTION.ret_reg;

/// Raw syscall with no arguments.
///
/// # Safety
///
/// The caller must use a valid `raven-abi` syscall number and satisfy its pointer arguments.
#[inline(always)]
pub unsafe fn syscall0(nr: usize) -> isize {
    let ret: isize;
    // SAFETY: `syscall` follows the raven-abi convention: number in `rax`, arguments
    // in `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`, and the result in `rax`. The kernel reads
    // and writes user memory, so the default memory clobber is required.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            clobber_abi("C"),
            options(nostack),
        );
    }
    ret
}

/// Raw syscall with one argument.
///
/// # Safety
///
/// See [`syscall0`].
#[inline(always)]
pub unsafe fn syscall1(nr: usize, a0: usize) -> isize {
    let ret: isize;
    // SAFETY: see `syscall0`.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            in("rdi") a0,
            clobber_abi("C"),
            options(nostack),
        );
    }
    ret
}

/// Raw syscall with two arguments.
///
/// # Safety
///
/// See [`syscall0`].
#[inline(always)]
pub unsafe fn syscall2(nr: usize, a0: usize, a1: usize) -> isize {
    let ret: isize;
    // SAFETY: see `syscall0`.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            in("rdi") a0,
            in("rsi") a1,
            clobber_abi("C"),
            options(nostack),
        );
    }
    ret
}

/// Raw syscall with three arguments.
///
/// # Safety
///
/// See [`syscall0`].
#[inline(always)]
pub unsafe fn syscall3(nr: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let ret: isize;
    // SAFETY: see `syscall0`.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            clobber_abi("C"),
            options(nostack),
        );
    }
    ret
}

/// Raw syscall with four arguments.
///
/// # Safety
///
/// See [`syscall0`].
#[inline(always)]
pub unsafe fn syscall4(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> isize {
    let ret: isize;
    // SAFETY: see `syscall0`. `r10` carries the fourth argument instead of `rcx`, which the
    // `syscall` instruction destroys.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            clobber_abi("C"),
            options(nostack),
        );
    }
    ret
}

/// Raw syscall with five arguments.
///
/// # Safety
///
/// See [`syscall0`].
#[inline(always)]
pub unsafe fn syscall5(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    let ret: isize;
    // SAFETY: see `syscall0`.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8") a4,
            clobber_abi("C"),
            options(nostack),
        );
    }
    ret
}

/// Raw syscall with six arguments.
///
/// # Safety
///
/// See [`syscall0`].
#[inline(always)]
pub unsafe fn syscall6(
    nr: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    let ret: isize;
    // SAFETY: see `syscall0`.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8") a4,
            in("r9") a5,
            clobber_abi("C"),
            options(nostack),
        );
    }
    ret
}
