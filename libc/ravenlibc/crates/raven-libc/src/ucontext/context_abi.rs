// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The C `ucontext_t *` ABI of `getcontext`, `setcontext` and `swapcontext`.
//!
//! The hidden `__rl_*` assembly in `raven-libc-signal` operates on the
//! `mcontext_t` embedded at `ucontext + UC_MCONTEXT_WORD * 8` (40 bytes on
//! x86-64), not on the `ucontext_t` the C ABI passes. The public signal
//! context functions add that offset and maintain the signal mask; these
//! functions are the pointer and `errno` layer between the C ABI and them.
//!
//! This file is also the source included by the host regression crate
//! `tests/libc/ucontext`, so it must not depend on the shell's target-only
//! modules beyond `crate::ffi::ret`.

use core::ffi::c_int;

use raven_libc_abi::errno::{EFAULT, Errno};
use raven_libc_abi::structs::UcontextT;

/// `EFAULT` as the signal context error type.
const EFAULT_ERR: Errno = Errno(EFAULT as u16);

/// `getcontext(3)` against a C `ucontext_t *`.
///
/// # Safety
///
/// `context` is NULL or points to a writable `ucontext_t`.
#[must_use]
pub unsafe fn getcontext(context: *mut UcontextT) -> c_int {
    if context.is_null() {
        return crate::ffi::ret::int(Err(EFAULT_ERR));
    }
    // SAFETY: the caller supplied a non-NULL writable context.
    let result = raven_libc_signal::context::getcontext(unsafe { &mut *context });
    crate::ffi::ret::int(result.map(|()| 0))
}

/// `setcontext(3)` against a C `ucontext_t *`; it returns only for NULL.
///
/// # Safety
///
/// `context` is NULL or points to a readable `ucontext_t` captured by
/// `getcontext` or prepared by `makecontext`.
#[must_use]
pub unsafe fn setcontext(context: *const UcontextT) -> c_int {
    if context.is_null() {
        return crate::ffi::ret::int(Err(EFAULT_ERR));
    }
    // SAFETY: the caller supplied a non-NULL readable context.
    raven_libc_signal::context::setcontext(unsafe { &*context });
}

/// `swapcontext(3)` against C `ucontext_t *` arguments.
///
/// # Safety
///
/// `old` is NULL or writable; `new` is NULL or readable and was captured or
/// prepared with the same `ucontext_t` layout.
#[must_use]
pub unsafe fn swapcontext(old: *mut UcontextT, new: *const UcontextT) -> c_int {
    if old.is_null() || new.is_null() {
        return crate::ffi::ret::int(Err(EFAULT_ERR));
    }
    // SAFETY: both non-NULL contexts follow the C contract.
    let result = raven_libc_signal::context::swapcontext(unsafe { &mut *old }, unsafe { &*new });
    crate::ffi::ret::int(result.map(|()| 0))
}
