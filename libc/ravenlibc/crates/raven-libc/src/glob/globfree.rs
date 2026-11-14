// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `globfree(3)`.
use super::{GlobT, take_state};

/// Release the arrays of a previous `glob` call.
///
/// # Safety
/// `pglob` is NULL or points to a `glob_t` returned by `glob`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn globfree(pglob: *mut GlobT) {
    // SAFETY: the caller's glob_t; a foreign or zeroed object has no mark and is left alone.
    if let Some(mut shell) = unsafe { take_state(pglob) } {
        shell.free_paths();
    }
}
