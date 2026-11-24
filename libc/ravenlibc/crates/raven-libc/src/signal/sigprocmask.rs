// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigprocmask(3)`: the raw kernel 0x88 per-thread u64 mask.
use core::ffi::c_int;

use raven_libc_abi::errno::Errno;
use raven_libc_signal::SigSet;

/// Change the calling thread's signal mask.
///
/// # Safety
/// `set`/`old` are NULL or point to `sigset_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigprocmask(how: c_int, set: *const SigSet, old: *mut SigSet) -> c_int {
    let set_ptr = if set.is_null() {
        0
    } else {
        set.cast::<u64>() as usize
    };
    let mut previous = 0u64;
    let old_ptr = if old.is_null() {
        0
    } else {
        &mut previous as *mut u64 as usize
    };
    // SAFETY: the kernel copies one u64 from/to the live pointers for the call.
    let raw = unsafe {
        raven_libc_sys::arch::syscall3(
            raven_abi::syscall::SIGPROCMASK,
            how as usize,
            set_ptr,
            old_ptr,
        )
    };
    if raw < 0 {
        crate::ffi::errno::set(Errno(raw.unsigned_abs() as u16));
        return -1;
    }
    if !old.is_null() {
        // SAFETY: the caller supplies a writable sigset_t; the kernel mask is the first word.
        let target = unsafe { &mut *old };
        if let Some(slot) = target.0.first_mut() {
            *slot = previous;
        }
        for slot in target.0.iter_mut().skip(1) {
            *slot = 0;
        }
    }
    0
}
