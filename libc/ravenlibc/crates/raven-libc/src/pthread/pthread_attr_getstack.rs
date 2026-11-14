// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_attr_getstack(3)`.
use raven_libc_thread::thread::attr as a;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_attr_getstack(
    attr: *const a::Attr,
    addr: *mut *mut core::ffi::c_void,
    size: *mut usize,
) -> core::ffi::c_int {
    if attr.is_null() || addr.is_null() || size.is_null() {
        return 22;
    }
    let attr_ref = unsafe { &*attr }; // SAFETY: attr is non-NULL after the check; the caller owns a live attribute for the call.
    match a::getstack(attr_ref) {
        Ok((base, length)) => {
            unsafe {
                // SAFETY: addr and size are non-NULL after the check and writable for one pointer and one usize.
                *addr = base as *mut core::ffi::c_void;
                *size = length;
            };
            0
        }
        Err(e) => super::common::err(e),
    }
}
