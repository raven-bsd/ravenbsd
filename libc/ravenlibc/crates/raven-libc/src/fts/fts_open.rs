// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fts_open(3)`.
use core::ffi::{c_char, c_int};

use super::{CURRENT_DIR, Cfts, Cftsent, open_common};

/// Open a file hierarchy walk over `argv`.
///
/// # Safety
/// `argv` is a NULL-terminated array of C path strings; `compar` is NULL or a valid C callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fts_open(
    argv: *const *mut c_char,
    options: c_int,
    compar: Option<extern "C" fn(*const *const Cftsent, *const *const Cftsent) -> c_int>,
) -> *mut Cfts {
    // SAFETY: the caller's argv and callback contracts.
    match unsafe { open_common(CURRENT_DIR, argv, options, compar) } {
        Ok(fts) => fts,
        Err(error) => crate::ffi::ret::null(error),
    }
}
