// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `res_query(3)` over C51's resolver and C63's DNS seams.

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::Errno;
use raven_libc_net::ErrnoExt;
use raven_libc_resolv::NO_RECOVERY;

/// `res_query(3)`.
///
/// # Safety
/// `name` is a NUL-terminated string; `answer` is writable for `anslen` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn res_query(
    name: *const c_char,
    class: c_int,
    rtype: c_int,
    answer: *mut u8,
    anslen: c_int,
) -> c_int {
    if name.is_null() || answer.is_null() || anslen < 0 {
        crate::ffi::errno::set(Errno::EINVAL);
        // SAFETY: the process h_errno cell is the same one __h_errno_location exposes.
        unsafe { crate::netdb::h_errno::h_errno = NO_RECOVERY };
        return -1;
    }
    let (Ok(class), Ok(rtype)) = (u16::try_from(class), u16::try_from(rtype)) else {
        crate::ffi::errno::set(Errno::EINVAL);
        // SAFETY: as above.
        unsafe { crate::netdb::h_errno::h_errno = NO_RECOVERY };
        return -1;
    };
    let length = usize::try_from(anslen).unwrap_or(0);
    // SAFETY: the caller's C-string and buffer contracts.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    // SAFETY: the caller granted `length` writable bytes.
    let answer = unsafe { core::slice::from_raw_parts_mut(answer, length) };
    match crate::netdb::records::resolver_query(name, class, rtype, answer) {
        Ok(length) => c_int::try_from(length).unwrap_or(c_int::MAX),
        Err(error) => {
            crate::ffi::errno::set_raw(error.errno);
            // SAFETY: as above.
            unsafe { crate::netdb::h_errno::h_errno = error.h_errno };
            -1
        }
    }
}

#[cfg(test)]
#[path = "res_query_ut.rs"]
mod tests;
