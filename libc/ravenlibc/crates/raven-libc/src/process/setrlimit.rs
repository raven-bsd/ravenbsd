// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setrlimit(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Rlimit;

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
use raven_libc_proc::syscall_backend::PROC_SETRLIMIT;

/// Write one resource limit.
///
/// # Safety
/// `limit` points to a readable `struct rlimit`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setrlimit(resource: c_int, limit: *const Rlimit) -> c_int {
    if limit.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    #[cfg(any(target_os = "none", target_os = "ravenbsd"))]
    {
        // SAFETY: the caller's readable `struct rlimit` was checked non-null above.
        let value = unsafe { &*limit };
        // SAFETY: both values are scalar u64 words; the kernel applies the soft<=hard,
        // hard-raise-privilege and EINVAL rules and returns the errno.
        let raw = unsafe {
            raven_libc_sys::arch::syscall3(
                PROC_SETRLIMIT,
                usize::try_from(resource).unwrap_or(usize::MAX),
                usize::try_from(value.rlim_cur).unwrap_or(usize::MAX),
                usize::try_from(value.rlim_max).unwrap_or(usize::MAX),
            )
        };
        return match raven_libc_sys::errno::ret(raw) {
            Ok(_) => 0,
            Err(error) => {
                crate::ffi::ret::int(Err(crate::ffi::map_proc::error(i32::from(error.0))))
            }
        };
    }
    #[cfg(not(any(target_os = "none", target_os = "ravenbsd")))]
    {
        let result =
            crate::ffi::map_proc::rlimit(&crate::ffi::procbackend::backend(), 0, resource, limit);
        crate::ffi::ret::int(result.map(|_| 0).map_err(crate::ffi::map_proc::error))
    }
}
