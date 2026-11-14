// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `execve(2)`.
use core::ffi::{c_char, c_int};

/// Replace the image with `path`, using the caller's argument and environment arrays.
///
/// # Safety
/// `path` is a NUL-terminated string; `argv`/`envp` are NULL-terminated string arrays.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn execve(
    path: *const c_char,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> c_int {
    if path.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    // SAFETY: the caller's three C arrays.
    let (program, arguments, environment) = unsafe {
        (
            crate::ffi::cstr::slice(path),
            crate::ffi::argv::collect(argv),
            crate::ffi::argv::collect(envp),
        )
    };
    let (arguments, environment) = match (arguments, environment) {
        (Ok(arguments), Ok(environment)) => (arguments, environment),
        (Err(error), _) | (_, Err(error)) => return crate::ffi::ret::int(Err(error)),
    };
    crate::process::exec_common::run(program, arguments, environment, None, false)
}
