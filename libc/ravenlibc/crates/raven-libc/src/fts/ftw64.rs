// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ftw64(3)`; the 64-bit `stat` ABI makes it an alias of `ftw`.

use core::ffi::{c_char, c_int};

use super::ftw::FtwFn;

/// Walk `path` in pre-order with the 64-bit `struct stat`.
///
/// # Safety
/// `path` is NUL terminated; `func` is a valid C callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ftw64(path: *const c_char, func: Option<FtwFn>, fd_limit: c_int) -> c_int {
    // SAFETY: forwarded caller contract.
    unsafe { super::ftw::ftw(path, func, fd_limit) }
}
