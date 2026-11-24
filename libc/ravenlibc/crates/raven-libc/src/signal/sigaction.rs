// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigaction(3)`: the raw kernel 0x87 request and the C 152-byte action.
use core::ffi::c_int;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::structs::Sigaction;
use raven_libc_signal::sigaction_abi::{KernelSigAction, from_c, to_c};

/// Install or query a disposition.
///
/// # Safety
/// `new`/`old` are NULL or point to `struct sigaction` objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sigaction(
    sig: c_int,
    new: *const Sigaction,
    old: *mut Sigaction,
) -> c_int {
    let request = if new.is_null() {
        None
    } else {
        // SAFETY: the caller supplies a readable sigaction.
        Some(from_c(unsafe { &*new }))
    };
    let mut previous = KernelSigAction::default();
    let new_ptr = request
        .as_ref()
        .map_or(0, |action| action as *const KernelSigAction as usize);
    let old_ptr = if old.is_null() {
        0
    } else {
        &mut previous as *mut KernelSigAction as usize
    };
    // SAFETY: the kernel copies the 40-byte request and the previous action from the pointers,
    // which are live for the call.
    let raw = unsafe {
        raven_libc_sys::arch::syscall3(raven_abi::syscall::SIGACTION, sig as usize, new_ptr, old_ptr)
    };
    if raw < 0 {
        crate::ffi::errno::set(Errno(raw.unsigned_abs() as u16));
        return -1;
    }
    if !old.is_null() {
        // SAFETY: the caller supplies a writable sigaction.
        to_c(&previous, unsafe { &mut *old });
    }
    0
}
