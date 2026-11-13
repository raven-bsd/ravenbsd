// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `vfprintf(3)` (and the `fprintf` trampoline target).
use core::ffi::{c_char, c_int, c_void};

use raven_libc_stdio::file_from_ptr;

use crate::stdio::va::{List, reader};

/// Format to `file`.
///
/// # Safety
/// `file` is a `FILE` from this libc, `format` is NUL terminated and `list` the trampoline list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vfprintf(
    file: *mut c_void,
    format: *const c_char,
    list: *mut List,
) -> c_int {
    if file.is_null() {
        crate::ffi::errno::set_raw(22);
        return -1;
    }
    // SAFETY: the caller passes a FILE from this libc.
    let file: &raven_libc_stdio::File<crate::ffi::env::ShellEnv> = file_from_ptr(file.cast());
    // SAFETY: the caller's format contract.
    let format = unsafe { crate::ffi::cstr::slice(format) };
    // SAFETY: the trampoline built the list.
    let mut env = crate::va::CEnv::new(
        unsafe { reader(list) },
        crate::stdio::vprintf::errno_text,
        crate::stdio::vprintf::alloc,
    );
    let options = crate::stdio::vprintf::options();
    match raven_libc_stdio::vfprintf_file(
        file,
        format,
        &mut env,
        &options,
        &crate::stdio::vprintf::FloatHook,
    ) {
        Ok(value) => value,
        Err(error) => crate::ffi::map_fmt::apply(error),
    }
}
