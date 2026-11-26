// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `if_nametoindex` from `<net/if.h>`.

use core::ffi::{c_char, c_uint};

use raven_libc_abi::errno::Errno;
use raven_libc_net::ErrnoExt;
use raven_libc_resolv::IfNames;

/// `if_nametoindex`.
///
/// # Safety
/// `name` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn if_nametoindex(name: *const c_char) -> c_uint {
    if name.is_null() {
        crate::ffi::errno::set(Errno::EFAULT);
        return 0;
    }
    // SAFETY: the caller's C string contract.
    let bytes = unsafe { crate::ffi::cstr::slice(name) };
    let Ok(text) = core::str::from_utf8(bytes) else {
        crate::ffi::errno::set(Errno::EINVAL);
        return 0;
    };
    let index = crate::socket::with_net(|net| net.ifnames().name_to_index(text));
    match index {
        Some(index) => index,
        // RFC 2553 4.1 / FreeBSD if_nametoindex.c: unknown name is ENXIO.
        None => {
            crate::ffi::errno::set(Errno::ENXIO);
            0
        }
    }
}
