// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `vsnprintf(3)` (and the `snprintf` trampoline target).
use core::ffi::{c_char, c_int};

use raven_libc_fmt::glue::vsnprintf_core;

use crate::stdio::va::{List, reader};

/// Format into a bounded buffer.
///
/// # Safety
/// `buffer` has `cap` writable bytes (or is NULL with `cap == 0`); `format` is NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vsnprintf(
    buffer: *mut c_char,
    cap: usize,
    format: *const c_char,
    list: *mut List,
) -> c_int {
    // SAFETY: the caller's buffer contract.
    let out = unsafe { crate::ffi::cstr::slice_mut(buffer, cap) };
    // SAFETY: the caller's format contract.
    let format = unsafe { crate::ffi::cstr::slice(format) };
    // SAFETY: the trampoline built the list.
    let mut env = crate::va::CEnv::new(
        unsafe { reader(list) },
        crate::stdio::vprintf::errno_text,
        crate::stdio::vprintf::alloc,
    );
    let options = crate::stdio::vprintf::options();
    match vsnprintf_core(
        out,
        cap,
        format,
        &mut env,
        &options,
        &crate::stdio::vprintf::FloatHook,
    ) {
        Ok(value) => value,
        Err(error) => crate::ffi::map_fmt::apply(error),
    }
}
