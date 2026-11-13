// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! AArch64 raw system calls.
//!
// RavenBSD: copied from userland/rt/src/sys.rs (same project, BSD-3-Clause)
// RavenBSD: moved from userland/rt/src/sys.rs

use super::convention::AARCH64 as CONVENTION;

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
    // SAFETY: `svc #0` follows the raven-abi convention: number in `x8`, arguments
    // in `x0..x5`, and the result in `x0`.
    unsafe {
        core::arch::asm!(
            "svc 0",
            in("x8") nr,
            lateout("x0") ret,
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
            "svc 0",
            in("x8") nr,
            in("x0") a0,
            lateout("x0") ret,
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
            "svc 0",
            in("x8") nr,
            in("x0") a0,
            in("x1") a1,
            lateout("x0") ret,
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
            "svc 0",
            in("x8") nr,
            in("x0") a0,
            in("x1") a1,
            in("x2") a2,
            lateout("x0") ret,
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
            "svc 0",
            in("x8") nr,
            in("x0") a0,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            lateout("x0") ret,
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
            "svc 0",
            in("x8") nr,
            in("x0") a0,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            lateout("x0") ret,
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
            "svc 0",
            in("x8") nr,
            in("x0") a0,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            in("x5") a5,
            lateout("x0") ret,
            options(nostack),
        );
    }
    ret as isize
}
