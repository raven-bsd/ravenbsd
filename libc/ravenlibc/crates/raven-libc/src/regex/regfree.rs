// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `regfree(3)`.
use super::{RegexT, take};

/// Release the compiled expression held by `preg`.
///
/// # Safety
/// `preg` is NULL or points to a `regex_t` previously filled by `regcomp`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn regfree(preg: *mut RegexT) {
    // SAFETY: the caller's regex_t; `take` drops a live Box only when the mark is present.
    drop(unsafe { take(preg.cast::<usize>()) });
}
