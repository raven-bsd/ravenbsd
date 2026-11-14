// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `error(3)`: the C59 variadic trampoline and the GNU diagnostic line.
use core::ffi::{c_char, c_int};

use raven_libc_stdio::vasprintf_core;

use crate::personality_linux::{error_globals, error_message_count, error_print_progname};
use crate::stdio::va::{List, reader};
use crate::stdio::vprintf::{FloatHook, options};

crate::raven_variadic!(
    prefix: "",
    export: "error",
    target: crate::personality_linux::error::__raven_va_error,
    named: 3
);

/// Flush stdout, then write `bytes` to stderr.
fn write_stderr(bytes: &[u8]) {
    use raven_libc_stdio::fflush;
    let streams = crate::stdio::streams::streams();
    let _ = fflush(streams.state(), Some(streams.stdout()));
    let _ = raven_libc_stdio::fwrite(streams.stderr(), bytes, 1, bytes.len());
}

fn render(format: &[u8], list: *mut List) -> alloc::vec::Vec<u8> {
    // SAFETY: the trampoline built the list.
    let mut env = crate::va::CEnv::new(
        unsafe { reader(list) },
        crate::stdio::vprintf::errno_text,
        crate::stdio::vprintf::alloc,
    );
    vasprintf_core(format, &mut env, &options(), &FloatHook).unwrap_or_default()
}

fn error_impl(status: c_int, errnum: c_int, format: *const c_char, list: *mut List) {
    let format = if format.is_null() {
        &b""[..]
    } else {
        // SAFETY: the caller's format contract.
        unsafe { crate::ffi::cstr::slice(format) }
    };
    let message = render(format, list);
    let strerror = raven_libc_str::errmsg::strerror::message(errnum);
    let _ = error_message_count::bump();
    let hook = error_print_progname::hook();
    let program = if hook.is_some() {
        if let Some(callback) = hook {
            // SAFETY: the caller installed a C function of the declared signature.
            unsafe { callback() };
        }
        alloc::vec::Vec::new()
    } else {
        error_globals::program_name()
    };
    let mut out = alloc::vec::Vec::new();
    raven_libc_personality_linux::error::format_error(
        &program, errnum, &message, strerror, &mut out,
    );
    write_stderr(&out);
    if status != 0 {
        crate::stdlib::exit::exit(status);
    }
}

/// The trampoline target: `error(status, errnum, format, ...)`.
///
/// # Safety
/// The trampoline built `list`; `format` is a NUL-terminated C string.
pub unsafe extern "C" fn __raven_va_error(
    status: c_int,
    errnum: c_int,
    format: *const c_char,
    list: *mut List,
) -> c_int {
    error_impl(status, errnum, format, list);
    0
}
