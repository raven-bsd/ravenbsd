// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `error_at_line(3)`: the C59 variadic trampoline and the `file:line:` form.
use core::ffi::{c_char, c_int, c_uint};

use crate::personality_common::ERROR_STATE;
use crate::personality_linux::{error_globals, error_message_count, error_one_per_line};
use crate::stdio::va::{List, reader};

crate::raven_variadic!(
    prefix: "",
    export: "error_at_line",
    target: crate::personality_linux::error_at_line::__raven_va_error_at_line,
    named: 5
);

/// The trampoline target.
///
/// # Safety
/// The trampoline built `list`; `format` and `file` are NUL-terminated C strings.
pub unsafe extern "C" fn __raven_va_error_at_line(
    status: c_int,
    errnum: c_int,
    file: *const c_char,
    line: c_uint,
    format: *const c_char,
    list: *mut List,
) -> c_int {
    let format = if format.is_null() {
        &b""[..]
    } else {
        // SAFETY: the caller's format contract.
        unsafe { crate::ffi::cstr::slice(format) }
    };
    let file = if file.is_null() {
        &b""[..]
    } else {
        // SAFETY: the caller's file-name contract.
        unsafe { crate::ffi::cstr::slice(file) }
    };
    // SAFETY: the trampoline built the list.
    let mut env = crate::va::CEnv::new(
        unsafe { reader(list) },
        crate::stdio::vprintf::errno_text,
        crate::stdio::vprintf::alloc,
    );
    let message = raven_libc_stdio::vasprintf_core(
        format,
        &mut env,
        &crate::stdio::vprintf::options(),
        &crate::stdio::vprintf::FloatHook,
    )
    .unwrap_or_default();
    let strerror = raven_libc_str::errmsg::strerror::message(errnum);
    let mut out = alloc::vec::Vec::new();
    let printed = ERROR_STATE.with(|state| {
        state.one_per_line = error_one_per_line::get();
        let printed = raven_libc_personality_linux::error::format_error_at_line(
            state,
            &error_globals::program_name(),
            errnum,
            file,
            line,
            &message,
            strerror,
            &mut out,
        );
        error_message_count::set(state.message_count);
        printed
    });
    let _ = printed;
    use raven_libc_stdio::fflush;
    let streams = crate::stdio::streams::streams();
    let _ = fflush(streams.state(), Some(streams.stdout()));
    if !out.is_empty() {
        let _ = raven_libc_stdio::fwrite(streams.stderr(), &out, 1, out.len());
    }
    if status != 0 {
        crate::stdlib::exit::exit(status);
    }
    0
}
