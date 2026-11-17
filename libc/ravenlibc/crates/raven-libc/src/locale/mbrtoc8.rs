// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mbrtoc8`.

use core::ffi::c_char;

use raven_libc_ctype::{MbMode, MbState, Mbr8};

/// `mbrtoc8`.
///
/// # Safety
/// `s` has `n` readable bytes (or is NULL); `pc8` and `state` are NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbrtoc8(
    pc8: *mut u8,
    s: *const c_char,
    n: usize,
    state: *mut MbState,
) -> usize {
    let mut local = MbState::INITIAL;
    // SAFETY: the caller's state contract.
    let state = unsafe { super::uchar::state_or_local(state, &mut local) };
    if s.is_null() {
        *state = MbState::INITIAL;
        return 0;
    }
    // SAFETY: the caller grants `n` readable bytes.
    let input = unsafe { core::slice::from_raw_parts(s.cast::<u8>(), n) };
    let result = raven_libc_ctype::mbrtoc8(state, MbMode::Utf8, input);
    match result {
        Mbr8::Char { unit, .. } | Mbr8::Continuation { unit } => {
            if !pc8.is_null() {
                // SAFETY: `pc8` is a writable `char8_t` slot.
                unsafe { *pc8 = unit };
            }
        }
        Mbr8::Incomplete | Mbr8::Ilseq => {}
    }
    super::uchar::size(result.c_return())
}
