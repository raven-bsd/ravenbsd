// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `wordexp(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_glob::WordExp;

use super::{
    ShellRunner, WordExpT, free_words, publish_words, wordexp_code, wordexp_flags, words_of,
};

/// Expand `words` into `out`.
///
/// # Safety
/// `words` is NUL terminated; `out` points to a writable `wordexp_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wordexp(words: *const c_char, out: *mut WordExpT, flags: c_int) -> c_int {
    // SAFETY: the caller's wordexp_t.
    let (old_array, old_offs, old_count) =
        unsafe { ((*out).we_wordv, (*out).we_offs, (*out).we_wordc) };
    let mut engine = WordExp {
        words: if flags & 2 != 0 {
            // SAFETY: the previous array has `old_offs + old_count + 1` slots.
            unsafe { words_of(old_array, old_offs, old_count) }
        } else {
            alloc::vec::Vec::new()
        },
        offs: if flags & 1 != 0 { old_offs } else { 0 },
    };
    // SAFETY: the caller's string contract.
    let input = unsafe { crate::ffi::cstr::slice(words) };
    let mut runner = ShellRunner::default();
    let code = raven_libc_glob::wordexp(&mut runner, input, wordexp_flags(flags), &mut engine);
    if runner.called {
        return -1;
    }
    if code != 0 {
        return wordexp_code(code);
    }
    // SAFETY: the old array belongs to this wordexp_t and is replaced below.
    unsafe { free_words(old_array, old_offs, old_count) };
    // SAFETY: the caller's wordexp_t.
    let code = unsafe { publish_words(out, &engine) };
    if code != 0 {
        // SAFETY: the caller's wordexp_t; a failed publication leaves no array.
        unsafe {
            (*out).we_wordc = 0;
            (*out).we_wordv = core::ptr::null_mut();
        }
        return 1;
    }
    0
}
