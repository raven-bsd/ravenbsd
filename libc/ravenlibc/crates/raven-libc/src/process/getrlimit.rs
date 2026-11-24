// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getrlimit(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::Rlimit;

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
use raven_libc_proc::syscall_backend::PROC_GETRLIMIT;

/// Read one resource limit.
///
/// # Safety
/// `limit` is NULL or points to a writable `struct rlimit`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getrlimit(resource: c_int, limit: *mut Rlimit) -> c_int {
    if limit.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    #[cfg(any(target_os = "none", target_os = "ravenbsd"))]
    {
        // The kernel owns the per-process limits (`PROC_GETRLIMIT`); the libc used to answer from
        // a fixed 1024/32768 table, which is not the value the kernel enforces.
        let mut words = [0u64; 2];
        // SAFETY: `words` is a live two-word array; the kernel validates the pointer and writes
        // exactly `[cur, max]`.
        let raw = unsafe {
            raven_libc_sys::arch::syscall2(
                PROC_GETRLIMIT,
                usize::try_from(resource).unwrap_or(usize::MAX),
                words.as_mut_ptr() as usize,
            )
        };
        return match raven_libc_sys::errno::ret(raw) {
            Ok(_) => {
                let [rlim_cur, rlim_max] = words;
                // SAFETY: the caller's writable limit record was checked non-null above.
                unsafe { limit.write(Rlimit { rlim_cur, rlim_max }) };
                0
            }
            Err(error) => {
                crate::ffi::ret::int(Err(crate::ffi::map_proc::error(i32::from(error.0))))
            }
        };
    }
    #[cfg(not(any(target_os = "none", target_os = "ravenbsd")))]
    {
        let result = crate::ffi::map_proc::rlimit(
            &crate::ffi::procbackend::backend(),
            0,
            resource,
            core::ptr::null(),
        );
        match result {
            // SAFETY: the caller's writable limit record.
            Ok(pair) => unsafe {
                limit.write(crate::ffi::map_proc::limit(pair));
                0
            },
            Err(error) => crate::ffi::ret::int(Err(crate::ffi::map_proc::error(error))),
        }
    }
}
