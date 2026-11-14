// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawnattr_getsigmask(3)`.
use core::ffi::c_int;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::{PosixSpawnattrT, SigsetT};

/// Copy the child signal mask out of the attribute blob.
///
/// # Safety
/// The two pointers follow their C contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawnattr_getsigmask(
    attr: *const PosixSpawnattrT,
    mask: *mut SigsetT,
) -> c_int {
    if attr.is_null() || mask.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's readable blob and writable 128-byte mask.
    unsafe {
        core::ptr::copy_nonoverlapping(
            crate::ffi::map_proc::attr_ref(attr).sigmask.as_ptr(),
            mask.cast::<u8>(),
            128,
        );
    }
    0
}
