// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `vdprintf(3)` (the `dprintf` trampoline target).

use core::ffi::{c_char, c_int};

use crate::stdio::va::{List, reader};

crate::raven_variadic!(prefix: "", export: "dprintf", target: crate::stdio::vdprintf::vdprintf, named: 2);

/// `vdprintf`: format to a descriptor.
///
/// # Safety
/// `format` is NUL terminated and `list` is the trampoline list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vdprintf(fd: c_int, format: *const c_char, list: *mut List) -> c_int {
    // SAFETY: the caller's format contract.
    let format = unsafe { crate::ffi::cstr::slice(format) };
    // SAFETY: the trampoline built the list.
    let mut env = crate::va::CEnv::new(
        unsafe { reader(list) },
        crate::stdio::vprintf::errno_text,
        crate::stdio::vprintf::alloc,
    );
    let options = crate::stdio::vprintf::options();
    match raven_libc_stdio::vdprintf_fd(
        &crate::ffi::env::ShellEnv,
        fd,
        format,
        &mut env,
        &options,
        &crate::stdio::vprintf::FloatHook,
    ) {
        Ok(value) => value,
        Err(error) => crate::ffi::map_fmt::apply(error),
    }
}
