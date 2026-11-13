// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RISC-V 64 raw system calls.
//!
// RavenBSD: copied from userland/rt/src/sys.rs (same project, BSD-3-Clause)
// RavenBSD: moved from userland/rt/src/sys.rs

use super::convention::RISCV64 as CONVENTION;

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
    let ret: usize;
    // SAFETY: `ecall` follows the raven-abi convention: number in `a7`, arguments
    // in `a0..a5`, and the result in `a0`.
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") nr,
            lateout("a0") ret,
            options(nostack),
        );
    }
    ret as isize
}

/// Raw syscall with one argument.
///
/// # Safety
///
/// See [`syscall0`].
#[inline(always)]
pub unsafe fn syscall1(nr: usize, a0: usize) -> isize {
    let ret: usize;
    // SAFETY: see `syscall0`.
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") nr,
            in("a0") a0,
            lateout("a0") ret,
            options(nostack),
        );
    }
    ret as isize
}

/// Raw syscall with two arguments.
///
/// # Safety
///
/// See [`syscall0`].
#[inline(always)]
pub unsafe fn syscall2(nr: usize, a0: usize, a1: usize) -> isize {
    let ret: usize;
    // SAFETY: see `syscall0`.
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") nr,
            in("a0") a0,
            in("a1") a1,
            lateout("a0") ret,
            options(nostack),
        );
    }
    ret as isize
}

/// Raw syscall with three arguments.
///
/// # Safety
///
/// See [`syscall0`].
#[inline(always)]
pub unsafe fn syscall3(nr: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let ret: usize;
    // SAFETY: see `syscall0`.
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") nr,
            in("a0") a0,
            in("a1") a1,
            in("a2") a2,
            lateout("a0") ret,
            options(nostack),
        );
    }
    ret as isize
}

/// Raw syscall with four arguments.
///
/// # Safety
///
/// See [`syscall0`].
#[inline(always)]
pub unsafe fn syscall4(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> isize {
    let ret: usize;
    // SAFETY: see `syscall0`.
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") nr,
            in("a0") a0,
            in("a1") a1,
            in("a2") a2,
            in("a3") a3,
            lateout("a0") ret,
            options(nostack),
        );
    }
    ret as isize
}

/// Raw syscall with five arguments.
///
/// # Safety
///
/// See [`syscall0`].
#[inline(always)]
pub unsafe fn syscall5(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    let ret: usize;
    // SAFETY: see `syscall0`.
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") nr,
            in("a0") a0,
            in("a1") a1,
            in("a2") a2,
            in("a3") a3,
            in("a4") a4,
            lateout("a0") ret,
            options(nostack),
        );
    }
    ret as isize
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
    let ret: usize;
    // SAFETY: see `syscall0`.
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") nr,
            in("a0") a0,
            in("a1") a1,
            in("a2") a2,
            in("a3") a3,
            in("a4") a4,
            in("a5") a5,
            lateout("a0") ret,
            options(nostack),
        );
    }
    ret as isize
}
