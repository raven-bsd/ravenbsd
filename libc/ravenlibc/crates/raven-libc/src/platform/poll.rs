// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `poll(2)`.
use core::ffi::c_int;
use core::mem::size_of;

use alloc::vec::Vec;

use raven_libc_abi::structs::Pollfd;

/// Wait for events on `nfds` descriptors.
///
/// # Safety
/// `fds` is NULL when `nfds == 0`, else points to `nfds` writable `struct pollfd` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn poll(fds: *mut Pollfd, nfds: usize, timeout: c_int) -> c_int {
    if nfds != 0 {
        // FreeBSD `kern_poll` copyin rejects a bad `ufds` with EFAULT. This shell copies the
        // array with the fault-safe `USER_COPY` before it forms any slice over the caller's
        // pointer; the window check is only a fast reject for pointers that cannot be valid.
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
    if nfds > raven_libc_platform::poll::MAX_POLL_FDS {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        )));
    }
    let bytes = match nfds.checked_mul(size_of::<Pollfd>()) {
        Some(bytes) => bytes,
        None => {
            return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
                raven_libc_abi::errno::EFAULT as u16,
            )));
        }
    };
    let mut snapshot: Vec<Pollfd> = Vec::new();
    if nfds != 0 {
        if snapshot.try_reserve_exact(nfds).is_err() {
            return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
                raven_libc_abi::errno::ENOMEM as u16,
            )));
        }
        snapshot.resize(
            nfds,
            Pollfd {
                fd: 0,
                events: 0,
                revents: 0,
            },
        );
        if let Err(error) =
            raven_libc_sys::sys::user_copy(snapshot.as_mut_ptr().cast(), fds.cast(), bytes)
        {
            return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(error.0)));
        }
    }
    let result =
        raven_libc_platform::poll::poll(crate::ffi::platform::global(), &mut snapshot, timeout);
    if result.is_ok() && nfds != 0 {
        // FreeBSD `pollout` copies `revents` back; a failed copy-out is EFAULT.
        if let Err(error) =
            raven_libc_sys::sys::user_copy(fds.cast(), snapshot.as_ptr().cast(), bytes)
        {
            return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(error.0)));
        }
    }
    match result {
        Ok(ready) => c_int::try_from(ready).unwrap_or(c_int::MAX),
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}
