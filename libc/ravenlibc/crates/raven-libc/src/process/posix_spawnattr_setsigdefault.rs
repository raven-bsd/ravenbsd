// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `posix_spawnattr_setsigdefault(3)`.
use core::ffi::c_int;

use raven_libc_abi::errno::EINVAL;
use raven_libc_abi::structs::{PosixSpawnattrT, SigsetT};

/// Copy the reset-to-default signal set into the attribute blob.
///
/// # Safety
/// The two pointers follow their C contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_spawnattr_setsigdefault(
    attr: *mut PosixSpawnattrT,
    set: *const SigsetT,
) -> c_int {
    if attr.is_null() || set.is_null() {
        return EINVAL;
    }
    // SAFETY: the caller's writable blob and readable 128-byte set.
    unsafe {
        core::ptr::copy_nonoverlapping(
            set.cast::<u8>(),
            crate::ffi::map_proc::attr_view(attr)
                .sigdefault
                .as_mut_ptr(),
            128,
        );
    }
    0
}
