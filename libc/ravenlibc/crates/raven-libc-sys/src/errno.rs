// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The per-thread `errno` cell and its provider seam.
//!
//! Before the TCB exists (`ld-elf.so.1`, early startup) a process-wide [`AtomicI32`] fallback is
//! used. `raven-libc-tls` (C57) calls [`install_errno_slot`] once the thread control block is
//! available; from then on every access goes through the installed slot function. There is no
//! crate dependency from `raven-libc-sys` to `raven-libc-tls`.

use core::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

use raven_abi::errno::Errno;

static FALLBACK: AtomicI32 = AtomicI32::new(0);
static PROVIDER: AtomicUsize = AtomicUsize::new(0);

/// The current thread's `errno` value.
#[inline]
pub fn errno() -> i32 {
    let provider = PROVIDER.load(Ordering::Acquire);
    if provider == 0 {
        return FALLBACK.load(Ordering::Relaxed);
    }
    // SAFETY: only `install_errno_slot` stores a non-zero value here, and its contract says the
    // pointer is a valid `unsafe fn() -> *mut i32` for the lifetime of the process. The returned
    // slot is the current thread's `errno` word.
    unsafe {
        let slot: unsafe fn() -> *mut i32 = core::mem::transmute(provider);
        let pointer = slot();
        if pointer.is_null() {
            FALLBACK.load(Ordering::Relaxed)
        } else {
            *pointer
        }
    }
}

/// Store `e` as the current thread's `errno`.
#[inline]
pub fn set_errno(e: i32) {
    let provider = PROVIDER.load(Ordering::Acquire);
    if provider == 0 {
        FALLBACK.store(e, Ordering::Relaxed);
        return;
    }
    // SAFETY: see [`errno`].
    unsafe {
        let slot: unsafe fn() -> *mut i32 = core::mem::transmute(provider);
        let pointer = slot();
        if pointer.is_null() {
            FALLBACK.store(e, Ordering::Relaxed);
        } else {
            *pointer = e;
        }
    }
}

/// Decode a raw syscall return value.
#[inline]
pub fn ret(r: isize) -> Result<usize, Errno> {
    Errno::from_ret(r)
}

/// Decode a raw syscall return value into an `i32`.
#[inline]
pub fn ret_i32(r: isize) -> Result<i32, Errno> {
    let value = ret(r)?;
    i32::try_from(value).map_err(|_| Errno::EINVAL)
}

/// Install the TCB-owned `errno` slot provider.
///
/// # Safety
///
/// `provider` must return a pointer to a live, writable `i32` belonging to the calling thread,
/// for as long as the process runs. It is called on every `errno` access and must not allocate,
/// take locks or call back into libc.
///
/// Installing a different provider a second time is harmless (the new one wins); a debug build
/// records the mismatch with `debug_assert!`.
pub unsafe fn install_errno_slot(provider: unsafe fn() -> *mut i32) {
    let new = provider as usize;
    let previous = PROVIDER.swap(new, Ordering::AcqRel);
    debug_assert!(
        previous == 0 || previous == new,
        "errno provider installed twice with different functions"
    );
}

#[cfg(test)]
pub(crate) fn reset_errno_slot() {
    PROVIDER.store(0, Ordering::Release);
    FALLBACK.store(0, Ordering::Relaxed);
}

#[cfg(test)]
#[path = "errno_ut.rs"]
mod tests;
