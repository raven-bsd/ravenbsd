// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getrlimit64`.

use super::*;

/// Read a 64-bit resource limit.
///
/// # Safety
/// `limit` points to a writable `struct rlimit`.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn getrlimit64(resource: c_int, limit: *mut Rlimit) -> c_int {
    if limit.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    match raven_libc_platform::compat::getrlimit64(global(), resource) {
        Ok((soft, hard)) => {
            // SAFETY: the caller supplied a writable Rlimit.
            unsafe {
                *limit = Rlimit {
                    rlim_cur: soft,
                    rlim_max: hard,
                }
            };
            0
        }
        Err(error) => ret::int(Err(map_error(error))),
    }
}
