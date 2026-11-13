// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `vsprintf(3)` (and the `sprintf` trampoline target).
use core::ffi::{c_char, c_int};

use raven_libc_fmt::glue::vsnprintf_core;

use crate::stdio::va::{List, reader};

/// Format into an unbounded caller buffer.
///
/// # Safety
/// `buffer` is large enough for the result plus a NUL; `format` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vsprintf(
    buffer: *mut c_char,
    format: *const c_char,
    list: *mut List,
) -> c_int {
    // SAFETY: the caller's format contract.
    let format = unsafe { crate::ffi::cstr::slice(format) };
    // SAFETY: the trampoline built the list.
    let mut env = crate::va::CEnv::new(
        unsafe { reader(list) },
        crate::stdio::vprintf::errno_text,
        crate::stdio::vprintf::alloc,
    );
    let options = crate::stdio::vprintf::options();
    // SAFETY: `vsprintf` promises the buffer holds the result; `usize::MAX` is the C "unbounded".
    let out = unsafe { crate::ffi::cstr::slice_mut(buffer, usize::MAX) };
    match vsnprintf_core(
        out,
        usize::MAX,
        format,
        &mut env,
        &options,
        &crate::stdio::vprintf::FloatHook,
    ) {
        Ok(value) => value,
        Err(error) => crate::ffi::map_fmt::apply(error),
    }
}
