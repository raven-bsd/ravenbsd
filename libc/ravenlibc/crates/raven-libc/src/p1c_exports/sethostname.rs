// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `sethostname`.

use super::*;

/// Set the host name.
///
/// # Safety
/// `name` has `len` readable bytes.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn sethostname(name: *const c_char, len: usize) -> c_int {
    // SAFETY: the caller's readable buffer contract.
    let name = unsafe { crate::ffi::cstr::bytes_n(name, len) };
    unit(global().sethostname(name))
}
