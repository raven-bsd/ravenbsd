// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `execv(2)`.
use core::ffi::{c_char, c_int};

/// Replace the image with `path` and the current environment.
///
/// # Safety
/// `path` is a NUL-terminated string; `argv` is a NULL-terminated string array.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn execv(path: *const c_char, argv: *const *const c_char) -> c_int {
    if path.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    // SAFETY: the caller's path and argument array.
    let (program, arguments) = unsafe {
        (
            crate::ffi::cstr::slice(path),
            crate::ffi::argv::collect(argv),
        )
    };
    let arguments = match arguments {
        Ok(arguments) => arguments,
        Err(error) => return crate::ffi::ret::int(Err(error)),
    };
    match crate::process::exec_common::current_envp() {
        Ok(environment) => {
            crate::process::exec_common::run(program, arguments, environment, None, false)
        }
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}
