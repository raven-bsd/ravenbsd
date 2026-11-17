// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mbrtoc16`.

use core::ffi::c_char;

use raven_libc_ctype::{MbMode, MbState, Mbr16};

/// `mbrtoc16`.
///
/// # Safety
/// `s` has `n` readable bytes (or is NULL); `pc16` and `state` are NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbrtoc16(
    pc16: *mut u16,
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
    let result = raven_libc_ctype::mbrtoc16(state, MbMode::Utf8, input);
    match result {
        Mbr16::Char { unit, .. } | Mbr16::PendingLow { unit } => {
            if !pc16.is_null() {
                // SAFETY: `pc16` is a writable `char16_t` slot.
                unsafe { *pc16 = unit };
            }
        }
        Mbr16::Incomplete | Mbr16::Ilseq => {}
    }
    super::uchar::size(result.c_return())
}
