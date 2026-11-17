// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `nftw64(3)`; the 64-bit `stat` ABI makes it an alias of `nftw`.

use core::ffi::{c_char, c_int};

use super::nftw::NftwFn;

/// Walk `path` with the 64-bit `struct stat`.
///
/// # Safety
/// `path` is NUL terminated; `func` is a valid C callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn nftw64(
    path: *const c_char,
    func: Option<NftwFn>,
    fd_limit: c_int,
    flags: c_int,
) -> c_int {
    // SAFETY: forwarded caller contract.
    unsafe { super::nftw::nftw(path, func, fd_limit, flags) }
}
