// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `open_wmemstream(3)`.

use core::ffi::c_void;

/// `open_wmemstream`: a wide memory stream whose buffer/size slots the stream updates.
///
/// # Safety
/// `buffer` and `size` are non-NULL writable pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn open_wmemstream(buffer: *mut *mut u32, size: *mut usize) -> *mut c_void {
    match raven_libc_stdio::open_wmemstream(crate::stdio::streams::streams().state(), buffer, size)
    {
        Ok(file) => file.cast(),
        Err(error) => {
            if let raven_libc_stdio::Fail::Errno(value) = error {
                crate::ffi::errno::set(value);
            }
            core::ptr::null_mut()
        }
    }
}
