// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mbrtowc(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_ctype::{MbMode, MbState, Mbr};

/// Decode one wide character from `s` in the UTF-8 locale.
///
/// # Safety
/// `s` has `n` readable bytes (or is NULL); `state` and `pwc` are NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbrtowc(
    pwc: *mut c_int,
    s: *const c_char,
    n: usize,
    state: *mut MbState,
) -> isize {
    let mut local = MbState::INITIAL;
    // SAFETY: the caller passes either NULL or a writable `mbstate_t`.
    let state = unsafe { state.as_mut() }.unwrap_or(&mut local);
    if s.is_null() {
        *state = MbState::INITIAL;
        return 0;
    }
    // SAFETY: the caller grants `n` readable bytes.
    let input = unsafe { core::slice::from_raw_parts(s.cast::<u8>(), n) };
    match raven_libc_ctype::mbrtowc(state, MbMode::Utf8, input) {
        Mbr::Char { wc, used } => {
            if !pwc.is_null() {
                // SAFETY: `pwc` is a writable `wchar_t` slot by the caller's contract.
                unsafe { *pwc = wc as c_int };
            }
            if wc == 0 { 0 } else { used as isize }
        }
        Mbr::Incomplete => -2,
        Mbr::Ilseq => {
            crate::ffi::errno::set_raw(84);
            -1
        }
    }
}
