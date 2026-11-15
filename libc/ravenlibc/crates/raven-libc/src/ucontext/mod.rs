// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public `getcontext`/`setcontext`/`swapcontext` entry points.
//!
//! The C ABI passes a `ucontext_t *`; the hidden `__rl_*` assembly in
//! `raven-libc-signal` operates on the `mcontext_t` at `ucontext + 40`
//! (`UC_MCONTEXT_WORD`). [`context_abi`] calls the public signal context
//! functions, which add that offset and handle the signal mask, instead of
//! tail-jumping into the raw assembly with the wrong pointer.

mod context_abi;
#[path = "raven_va_makecontext.rs"]
mod makecontext;

use core::ffi::c_int;

use raven_libc_abi::structs::UcontextT;

/// `getcontext(3)`.
///
/// # Safety
/// `context` is NULL or points to a writable `ucontext_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getcontext(context: *mut UcontextT) -> c_int {
    // SAFETY: the C contract above is the callee's safety contract.
    unsafe { context_abi::getcontext(context) }
}

/// `setcontext(3)`; returns only on a NULL argument.
///
/// # Safety
/// `context` is NULL or points to a readable `ucontext_t` captured by
/// `getcontext` or prepared by `makecontext`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setcontext(context: *const UcontextT) -> c_int {
    // SAFETY: the C contract above is the callee's safety contract.
    unsafe { context_abi::setcontext(context) }
}

/// `swapcontext(3)`.
///
/// # Safety
/// `old` is NULL or writable; `new` is NULL or readable and was captured or
/// prepared with the same `ucontext_t` layout.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn swapcontext(old: *mut UcontextT, new: *const UcontextT) -> c_int {
    // SAFETY: the C contract above is the callee's safety contract.
    unsafe { context_abi::swapcontext(old, new) }
}
