// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Wide-character stdio C-ABI exports (`fgetwc`).

use core::ffi::c_void;

use raven_libc_abi::errno::Errno;
use raven_libc_ctype::WEOF;
use raven_libc_stdio::{Fail, File, file_from_ptr};

use crate::ffi::env::ShellEnv;

type ShellFile = File<ShellEnv>;

/// `fgetwc`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgetwc(stream: *mut c_void) -> u32 {
    if stream.is_null() {
        crate::ffi::errno::set(Errno(9));
        return WEOF;
    }
    // SAFETY: the caller passes a FILE from this libc.
    let target: &ShellFile = file_from_ptr(stream.cast());
    match raven_libc_stdio::fgetwc(target) {
        Ok(value) => value,
        Err(Fail::Eof) => WEOF,
        Err(Fail::Errno(error)) => {
            crate::ffi::errno::set(error);
            WEOF
        }
    }
}
