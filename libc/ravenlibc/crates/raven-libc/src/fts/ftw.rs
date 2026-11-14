// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ftw(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_abi::structs::Stat;

use crate::platform::ShellDirSys;

/// The C `ftw` callback.
pub(crate) type FtwFn = extern "C" fn(*const c_char, *const Stat, c_int) -> c_int;

/// Walk `path` in pre-order.
///
/// # Safety
/// `path` is NUL terminated; `func` is a valid C callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ftw(path: *const c_char, func: Option<FtwFn>, fd_limit: c_int) -> c_int {
    let Some(func) = func else {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        )));
    };
    // SAFETY: the caller's path contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    let mut sys = ShellDirSys;
    let mut callback = |name: &[u8], stat: &Stat, flag: i32| -> c_int {
        func(super::cstr_vec(name).as_ptr().cast::<c_char>(), stat, flag)
    };
    match raven_libc_fts::ftw(&mut sys, path, &mut callback, fd_limit) {
        Ok(value) => value,
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}
