// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `glob(3)`.
use alloc::vec::Vec;
use core::ffi::{c_char, c_int};

use super::{
    GlobT, ShellGlobDir, ShellGlobEnv, ShellPatternLocale, glob_code, glob_flags, state_of,
};

/// Expand `pattern` into `pglob`.
///
/// # Safety
/// `pattern` is NUL terminated; `errfunc` is NULL or a valid C callback; `pglob` points to a
/// writable `glob_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn glob(
    pattern: *const c_char,
    flags: c_int,
    errfunc: Option<extern "C" fn(*const c_char, c_int) -> c_int>,
    pglob: *mut GlobT,
) -> c_int {
    if pglob.is_null() {
        return 1;
    }
    if flags & 0x4000 != 0 {
        crate::ffi::errno::set_raw(raven_libc_abi::errno::EINVAL);
        return 4;
    }
    // SAFETY: the caller's glob_t; the offset is read before the state borrow.
    let requested_offs = unsafe { (*pglob).gl_offs };
    // SAFETY: the caller's glob_t.
    let Some(shell) = (unsafe { state_of(pglob) }) else {
        return 1;
    };
    shell.state.offs = if flags & 0x08 != 0 { requested_offs } else { 0 };
    // SAFETY: the caller's pattern contract.
    let pattern = unsafe { crate::ffi::cstr::slice(pattern) };
    let mut dir = ShellGlobDir::new();
    let env = ShellGlobEnv;
    let locale = ShellPatternLocale;
    let mut callback = |path: &[u8], error: i32| -> c_int { call_errfunc(errfunc, path, error) };
    let handler: Option<&mut dyn FnMut(&[u8], i32) -> c_int> = if errfunc.is_some() {
        Some(&mut callback)
    } else {
        None
    };
    let code = raven_libc_glob::glob(
        &mut dir,
        &env,
        &locale,
        pattern,
        glob_flags(flags),
        handler,
        &mut shell.state,
    );
    if shell.state.errno != 0 {
        crate::ffi::errno::set_raw(shell.state.errno);
    }
    if code != 0 {
        return glob_code(code);
    }
    if shell.publish(pglob) != 0 {
        return 1;
    }
    glob_code(code)
}

/// Call the caller's `gl_errfunc` with a NUL-terminated path.
fn call_errfunc(
    errfunc: Option<extern "C" fn(*const c_char, c_int) -> c_int>,
    path: &[u8],
    error: i32,
) -> c_int {
    let Some(func) = errfunc else {
        return 0;
    };
    let mut text: Vec<u8> = Vec::with_capacity(path.len().saturating_add(1));
    text.extend_from_slice(path);
    text.push(0);
    func(text.as_ptr().cast::<c_char>(), error)
}
