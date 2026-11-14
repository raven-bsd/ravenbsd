// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `waitid(2)`.
use core::ffi::c_int;

use raven_libc_abi::structs::SiginfoT;

/// Wait for the children `idtype`/`id` names.
///
/// # Safety
/// `info` is NULL or points to a writable `siginfo_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn waitid(
    idtype: c_int,
    id: u32,
    info: *mut SiginfoT,
    options: c_int,
) -> c_int {
    let Ok(idtype) = u32::try_from(idtype) else {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::invalid()));
    };
    let result = crate::ffi::map_proc::waitid(
        &crate::ffi::procbackend::backend(),
        idtype,
        i64::from(id),
        info,
        options,
    );
    crate::ffi::ret::int(result.map(|()| 0).map_err(crate::ffi::map_proc::error))
}
