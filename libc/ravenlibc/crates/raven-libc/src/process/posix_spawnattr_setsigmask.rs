// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawnattr_setsigmask(3)`.
use core::ffi::c_int;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::{PosixSpawnattrT, SigsetT};

/// Copy the child signal mask into the attribute blob.
///
/// # Safety
/// The two pointers follow their C contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawnattr_setsigmask(
    attr: *mut PosixSpawnattrT,
    mask: *const SigsetT,
) -> c_int {
    if attr.is_null() || mask.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's writable blob and readable 128-byte mask.
    unsafe {
        core::ptr::copy_nonoverlapping(
            mask.cast::<u8>(),
            crate::ffi::map_proc::attr_view(attr).sigmask.as_mut_ptr(),
            128,
        );
    }
    0
}
