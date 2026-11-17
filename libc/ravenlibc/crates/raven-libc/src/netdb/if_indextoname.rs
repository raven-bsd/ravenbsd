// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `if_indextoname` from `<net/if.h>`.

use core::ffi::{c_char, c_uint};

use raven_libc_abi::errno::Errno;
use raven_libc_resolv::IfNames;

/// `if_indextoname`.
///
/// # Safety
/// `buffer` has at least `IF_NAMESIZE` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn if_indextoname(index: c_uint, buffer: *mut c_char) -> *mut c_char {
    if buffer.is_null() {
        crate::ffi::errno::set(Errno(14));
        return core::ptr::null_mut();
    }
    let name = crate::socket::with_net(|net| net.ifnames().index_to_name(index));
    match name {
        Some(name) => {
            // SAFETY: the caller's `IF_NAMESIZE` buffer.
            unsafe { crate::ffi::out::write_cstr(buffer, name.as_bytes(), 16) };
            buffer
        }
        None => crate::ffi::ret::null(Errno(6)),
    }
}
