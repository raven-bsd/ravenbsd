// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `posix_close`.

use super::*;

/// Close a descriptor with `posix_close` semantics.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn posix_close(fd: c_int, flags: c_int) -> c_int {
    if flags != 0 {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        )));
    }
    unit(raven_libc_platform::compat::posix_close(global(), fd))
}
