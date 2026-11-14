// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `wordfree(3)`.
use super::{WordExpT, free_words};

/// Release the arrays of a previous `wordexp` call.
///
/// # Safety
/// `out` is NULL or points to a `wordexp_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wordfree(out: *mut WordExpT) {
    if out.is_null() {
        return;
    }
    // SAFETY: the caller's wordexp_t.
    let (array, offs, count) = unsafe { ((*out).we_wordv, (*out).we_offs, (*out).we_wordc) };
    // SAFETY: the array and strings were allocated by `publish_words`.
    unsafe { free_words(array, offs, count) };
    // SAFETY: the caller's wordexp_t; the result is empty after the free.
    unsafe {
        (*out).we_wordc = 0;
        (*out).we_wordv = core::ptr::null_mut();
        (*out).we_offs = 0;
    }
}
