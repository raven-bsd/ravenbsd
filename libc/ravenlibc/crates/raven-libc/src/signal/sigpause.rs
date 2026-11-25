// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sigpause(3)`.
use core::ffi::c_int;
use core::ptr;

use raven_libc_abi::generated::signal;
use raven_libc_signal::SigSet;

/// Read the calling thread's authoritative mask, remove `sig`, and wait.
#[unsafe(no_mangle)]
pub extern "C" fn sigpause(sig: c_int) -> c_int {
    let mut mask = SigSet::empty();
    // SAFETY: `mask` is a live writable public sigset_t and no new set is supplied.
    if unsafe { super::sigprocmask::sigprocmask(signal::SIG_BLOCK, ptr::null(), &mut mask) } != 0 {
        return -1;
    }
    if mask.del(sig).is_err() {
        crate::ffi::errno::set(raven_libc_abi::errno::Errno(
            raven_libc_abi::generated::errno::EINVAL as u16,
        ));
        return -1;
    }
    // SAFETY: `mask` is a live readable public sigset_t.
    unsafe { super::sigsuspend::sigsuspend(&mask) }
}
