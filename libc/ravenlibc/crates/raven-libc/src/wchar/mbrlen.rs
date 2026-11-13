// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mbrlen(3)` and `mbsinit(3)`.
use core::ffi::c_char;

use raven_libc_ctype::{MbMode, MbState, Mbr};

/// The length of the next multi-byte character.
///
/// # Safety
/// `s` has `n` readable bytes (or is NULL); `state` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbrlen(s: *const c_char, n: usize, state: *mut MbState) -> usize {
    let mut local = MbState::INITIAL;
    // SAFETY: the caller passes either NULL or a writable `mbstate_t`.
    let state = unsafe { state.as_mut() }.unwrap_or(&mut local);
    if s.is_null() {
        *state = MbState::INITIAL;
        return 0;
    }
    // SAFETY: the caller grants `n` readable bytes.
    let input = unsafe { core::slice::from_raw_parts(s.cast::<u8>(), n) };
    let result = raven_libc_ctype::mbrlen(state, MbMode::Utf8, input);
    match result {
        Mbr::Char { wc, used } => {
            if wc == 0 {
                0
            } else {
                used
            }
        }
        Mbr::Incomplete => usize::MAX.wrapping_sub(1),
        Mbr::Ilseq => usize::MAX,
    }
}

/// Whether `state` is the initial conversion state.
///
/// # Safety
/// `state` is NULL or a readable `mbstate_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbsinit(state: *const MbState) -> core::ffi::c_int {
    let local = MbState::INITIAL;
    // SAFETY: the caller passes either NULL or a readable `mbstate_t`.
    let state = unsafe { state.as_ref() }.unwrap_or(&local);
    core::ffi::c_int::from(raven_libc_ctype::mbsinit(state))
}
