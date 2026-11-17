// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `vfwprintf(3)` (and the `fwprintf` trampoline target).

use core::ffi::{c_int, c_void};

use raven_libc_stdio::file_from_ptr;

use crate::stdio::va::{List, reader};

/// A `wchar_t` string as a slice; `wchar_t` is 32-bit on the RavenBSD targets.
///
/// # Safety
/// `format` is NULL or points to a readable NUL-terminated `wchar_t` string.
unsafe fn wide_format<'a>(format: *const u32) -> &'a [u32] {
    if format.is_null() {
        return &[];
    }
    let mut length = 0usize;
    // SAFETY: the caller's string has a NUL; the walk cannot pass it.
    while unsafe { *format.add(length) } != 0 {
        length = length.wrapping_add(1);
    }
    // SAFETY: `length` readable units precede the NUL the walk found.
    unsafe { core::slice::from_raw_parts(format, length) }
}

/// `vfwprintf`.
///
/// # Safety
/// `file` is a FILE from this libc, `format` is NUL terminated and `list` is the trampoline list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vfwprintf(
    file: *mut c_void,
    format: *const u32,
    list: *mut List,
) -> c_int {
    if file.is_null() {
        crate::ffi::errno::set(raven_libc_abi::errno::Errno(22));
        return -1;
    }
    // SAFETY: the caller passes a FILE from this libc.
    let target: &raven_libc_stdio::File<crate::ffi::env::ShellEnv> = file_from_ptr(file.cast());
    // SAFETY: the caller's wide-format contract.
    let format = unsafe { wide_format(format) };
    // SAFETY: the trampoline built the list.
    let mut env = crate::va::CEnv::new(
        unsafe { reader(list) },
        crate::stdio::vprintf::errno_text,
        crate::stdio::vprintf::alloc,
    );
    let options = crate::stdio::vprintf::options();
    match raven_libc_stdio::vfwprintf_file(
        target,
        format,
        &mut env,
        &options,
        &crate::stdio::vprintf::FloatHook,
    ) {
        Ok(value) => value,
        Err(error) => crate::ffi::map_fmt::apply(error),
    }
}
