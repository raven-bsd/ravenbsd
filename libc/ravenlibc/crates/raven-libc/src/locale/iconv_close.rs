// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `iconv_close` from `<iconv.h>`.

use alloc::boxed::Box;
use core::ffi::{c_int, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_iconv::Iconv;

/// `iconv_close`.
///
/// # Safety
/// `cd` is a descriptor from `iconv_open` or `(iconv_t)-1`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iconv_close(cd: *mut c_void) -> c_int {
    if cd.is_null() || cd as usize == usize::MAX {
        crate::ffi::errno::set(Errno(9));
        return -1;
    }
    // SAFETY: `cd` is a Box from `iconv_open`.
    drop(unsafe { Box::from_raw(cd.cast::<Iconv>()) });
    0
}
