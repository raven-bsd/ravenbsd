// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getcwd(3)`.
use core::ffi::c_char;

/// Copy the current directory into `buf`, or allocate it when `buf` is NULL.
///
/// # Safety
/// `buf` is NULL or points to `size` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getcwd(buf: *mut c_char, size: usize) -> *mut c_char {
    if !buf.is_null() && size == 0 {
        crate::ffi::errno::set(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        ));
        return core::ptr::null_mut();
    }
    let platform = crate::ffi::platform::global();
    if !buf.is_null() {
        // SAFETY: the caller's buffer contract.
        let out = unsafe { crate::ffi::cstr::slice_mut(buf, size) };
        return match platform.getcwd(Some(out), size) {
            Ok(_) => buf,
            Err(error) => {
                crate::ffi::errno::set_platform(error);
                core::ptr::null_mut()
            }
        };
    }
    match platform.getcwd(None, size) {
        Ok(Some(value)) => {
            // SAFETY: allocating the C copy of the platform's path.
            unsafe { super::copy_c_path(&value) }
        }
        _ => core::ptr::null_mut(),
    }
}
