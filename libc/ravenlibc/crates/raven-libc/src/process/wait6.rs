// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `wait6(2)`.
use core::ffi::{c_int, c_void};

use raven_libc_abi::structs::SiginfoT;

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
use raven_abi::syscall::PROC_WAIT6;

/// Wait for the children `idtype`/`id` names, filling `status`, `wrusage` and `siginfo`.
///
/// The call is the raw `PROC_WAIT6` seam (contract C21), not the native-spawner pid table, so a
/// `fork(2)` child is waitable. `status` is the POSIX status word, `wrusage` is the 288-byte
/// `struct __wrusage` and `info` is the 128-byte `siginfo_t`; a null pointer skips that output.
///
/// # Safety
/// `status`, `wrusage` and `info` are NULL or writable for their documented sizes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wait6(
    idtype: c_int,
    id: u32,
    status: *mut c_int,
    options: c_int,
    wrusage: *mut c_void,
    info: *mut SiginfoT,
) -> c_int {
    let Ok(idtype) = u32::try_from(idtype) else {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::invalid()));
    };
    let Ok(options) = u32::try_from(options) else {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::invalid()));
    };
    #[cfg(any(target_os = "none", target_os = "ravenbsd"))]
    {
        // SAFETY: the kernel validates the id type and option word, and every non-null output
        // pointer is a caller-owned writable range of the size the contract names.
        let raw = unsafe {
            raven_libc_sys::arch::syscall6(
                PROC_WAIT6,
                idtype as usize,
                id as usize,
                options as usize,
                status as usize,
                wrusage as usize,
                info as usize,
            )
        };
        crate::ffi::ret::int(
            raven_libc_sys::errno::ret_i32(raw)
                .map_err(|error| crate::ffi::map_proc::error(i32::from(error.0))),
        )
    }
    #[cfg(not(any(target_os = "none", target_os = "ravenbsd")))]
    {
        // The host build has no kernel wait tree; the target syscall is the implementation.
        crate::ffi::ret::int(Err(crate::ffi::map_proc::error(i32::from(
            raven_abi::errno::Errno::ENOSYS.0,
        ))))
    }
}
