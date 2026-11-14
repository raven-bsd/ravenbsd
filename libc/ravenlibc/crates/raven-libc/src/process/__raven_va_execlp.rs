// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `v` target of the `execlp` trampoline.
use core::ffi::{c_char, c_int};

/// `execlp(file, arg0, ..., NULL)` with the `PATH` search.
///
/// # Safety
/// The C59 trampoline built `list`; `file` and `first` follow the C string contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_va_execlp(
    file: *const c_char,
    first: *const c_char,
    list: *mut crate::stdio::va::List,
) -> c_int {
    if file.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    // SAFETY: the trampoline's list is valid for the call.
    let mut values = unsafe { crate::stdio::va::reader(list) };
    let arguments = match crate::process::va_args::collect_argv(&mut values, first) {
        Ok(arguments) => arguments,
        Err(error) => return crate::ffi::ret::int(Err(error)),
    };
    // SAFETY: the caller's file name contract.
    let program = unsafe { crate::ffi::cstr::slice(file) };
    match crate::process::exec_common::current_envp() {
        Ok(environment) => {
            crate::process::exec_common::run(program, arguments, environment, None, true)
        }
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}
