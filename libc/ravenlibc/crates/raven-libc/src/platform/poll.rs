// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `poll(2)`.
use core::ffi::c_int;
use core::mem::size_of;

use raven_libc_abi::structs::Pollfd;

/// Wait for events on `nfds` descriptors.
///
/// # Safety
/// `fds` is NULL when `nfds == 0`, else points to `nfds` writable `struct pollfd` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn poll(fds: *mut Pollfd, nfds: usize, timeout: c_int) -> c_int {
    if nfds != 0 {
        // FreeBSD `kern_poll` copyin rejects a bad `ufds` with EFAULT. This shell reads the array
        // in user mode, so it rejects every pointer outside the user window before forming a
        // slice; a pointer that passes the window but has no mapped page is the kernel copyin path.
        let valid = nfds.checked_mul(size_of::<Pollfd>()).is_some_and(|bytes| {
            raven_libc_platform::poll::validate_user_range(fds as usize as u64, bytes as u64)
                .is_ok()
        });
        if !valid {
            return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
                raven_libc_abi::errno::EFAULT as u16,
            )));
        }
    }
    // SAFETY: the caller's array contract; a NULL pointer with zero entries is the empty slice.
    let entries: &mut [Pollfd] = if nfds == 0 {
        &mut []
    } else {
        unsafe { core::slice::from_raw_parts_mut(fds, nfds) }
    };
    match raven_libc_platform::poll::poll(crate::ffi::platform::global(), entries, timeout) {
        Ok(ready) => c_int::try_from(ready).unwrap_or(c_int::MAX),
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
