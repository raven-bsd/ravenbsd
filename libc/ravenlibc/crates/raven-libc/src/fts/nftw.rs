// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `nftw(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_abi::structs::Stat;

use crate::platform::ShellDirSys;

/// The C `struct FTW`.
#[repr(C)]
pub(crate) struct CFtw {
    /// `base`.
    pub base: c_int,
    /// `level`.
    pub level: c_int,
}

/// The C `nftw` callback.
pub(crate) type NftwFn = extern "C" fn(*const c_char, *const Stat, c_int, *const CFtw) -> c_int;

/// Walk `path` with `nftw` flags.
///
/// # Safety
/// `path` is NUL terminated; `func` is a valid C callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nftw(
    path: *const c_char,
    func: Option<NftwFn>,
    fd_limit: c_int,
    flags: c_int,
) -> c_int {
    let Some(func) = func else {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        )));
    };
    // SAFETY: the caller's path contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    let mut sys = ShellDirSys;
    let mut callback = |name: &[u8], stat: &Stat, flag: i32, ftw: &raven_libc_fts::Ftw| -> c_int {
        let base = c_int::try_from(ftw.base).unwrap_or(c_int::MAX);
        let level = c_int::try_from(ftw.level).unwrap_or(c_int::MAX);
        let c_ftw = CFtw { base, level };
        func(
            super::cstr_vec(name).as_ptr().cast::<c_char>(),
            stat,
            flag,
            &c_ftw,
        )
    };
    match raven_libc_fts::nftw(&mut sys, path, &mut callback, fd_limit, flags) {
        Ok(value) => value,
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}
