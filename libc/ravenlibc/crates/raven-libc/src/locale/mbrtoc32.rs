// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mbrtoc32`.

use core::ffi::{c_char, c_uint};

use raven_libc_ctype::{MbMode, MbState, Mbr};

/// `mbrtoc32`.
///
/// # Safety
/// `s` has `n` readable bytes (or is NULL); `pc32` and `state` are NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbrtoc32(
    pc32: *mut c_uint,
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
    let result = raven_libc_ctype::mbrtoc32(state, MbMode::Utf8, input);
    if let Mbr::Char { wc, .. } = result
        && !pc32.is_null()
    {
        // SAFETY: `pc32` is a writable `char32_t` slot.
        unsafe { *pc32 = wc };
    }
    super::uchar::size(result.c_return())
}
