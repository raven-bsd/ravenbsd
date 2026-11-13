// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `host-fake` replacements for the per-architecture stubs.

/// Raw syscall with no arguments, served by the fake kernel.
///
/// # Safety
///
/// The pointer arguments must satisfy the same contract as the real stub.
#[inline]
pub unsafe fn syscall0(nr: usize) -> isize {
    // SAFETY: the fake dispatch validates the number and never executes an instruction.
    unsafe { super::dispatch(nr, [0; 6]) }
}

/// Raw syscall with one argument.
///
/// # Safety
///
/// See [`syscall0`].
#[inline]
pub unsafe fn syscall1(nr: usize, a0: usize) -> isize {
    // SAFETY: see `syscall0`.
    unsafe { super::dispatch(nr, [a0, 0, 0, 0, 0, 0]) }
}

/// Raw syscall with two arguments.
///
/// # Safety
///
/// See [`syscall0`].
#[inline]
pub unsafe fn syscall2(nr: usize, a0: usize, a1: usize) -> isize {
    // SAFETY: see `syscall0`.
    unsafe { super::dispatch(nr, [a0, a1, 0, 0, 0, 0]) }
}

/// Raw syscall with three arguments.
///
/// # Safety
///
/// See [`syscall0`].
#[inline]
pub unsafe fn syscall3(nr: usize, a0: usize, a1: usize, a2: usize) -> isize {
    // SAFETY: see `syscall0`.
    unsafe { super::dispatch(nr, [a0, a1, a2, 0, 0, 0]) }
}

/// Raw syscall with four arguments.
///
/// # Safety
///
/// See [`syscall0`].
#[inline]
pub unsafe fn syscall4(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> isize {
    // SAFETY: see `syscall0`.
    unsafe { super::dispatch(nr, [a0, a1, a2, a3, 0, 0]) }
}

/// Raw syscall with five arguments.
///
/// # Safety
///
/// See [`syscall0`].
#[inline]
pub unsafe fn syscall5(nr: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize) -> isize {
    // SAFETY: see `syscall0`.
    unsafe { super::dispatch(nr, [a0, a1, a2, a3, a4, 0]) }
}

/// Raw syscall with six arguments.
///
/// # Safety
///
/// See [`syscall0`].
#[inline]
pub unsafe fn syscall6(
    nr: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    // SAFETY: see `syscall0`.
    unsafe { super::dispatch(nr, [a0, a1, a2, a3, a4, a5]) }
}
