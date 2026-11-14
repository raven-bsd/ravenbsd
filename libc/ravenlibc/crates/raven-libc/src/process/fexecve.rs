// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fexecve(2)`.
use core::ffi::{c_char, c_int};

/// Execute the image open on `fd` with the caller's arrays.
///
/// # Safety
/// `argv`/`envp` are NULL-terminated string arrays.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fexecve(
    fd: c_int,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> c_int {
    // SAFETY: the caller's two string arrays.
    let (arguments, environment) = unsafe {
        (
            crate::ffi::argv::collect(argv),
            crate::ffi::argv::collect(envp),
        )
    };
    let (arguments, environment) = match (arguments, environment) {
        (Ok(arguments), Ok(environment)) => (arguments, environment),
        (Err(error), _) | (_, Err(error)) => return crate::ffi::ret::int(Err(error)),
    };
    crate::process::exec_common::run(&[], arguments, environment, Some(fd), false)
}
