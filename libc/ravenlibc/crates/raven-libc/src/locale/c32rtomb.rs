// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `c32rtomb`.

use core::ffi::c_char;

use raven_libc_ctype::{MbMode, MbState};

/// `c32rtomb`.
///
/// # Safety
/// `s` has four writable bytes (or is NULL); `state` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn c32rtomb(s: *mut c_char, c32: u32, state: *mut MbState) -> usize {
    let mut local = MbState::INITIAL;
    // SAFETY: the caller's state contract; UTF-32 carries no partial state, so reset it.
    let state = unsafe { super::uchar::state_or_local(state, &mut local) };
    *state = MbState::INITIAL;
    let mut buffer = [0u8; 4];
    match raven_libc_ctype::c32rtomb(MbMode::Utf8, c32, &mut buffer) {
        Ok(length) => {
            if !s.is_null() {
                // SAFETY: the caller grants four writable bytes; `length <= 4`.
                unsafe {
                    crate::ffi::out::write_bytes(
                        s.cast(),
                        buffer.get(..length).unwrap_or(&buffer),
                        4,
                    )
                };
            }
            length
        }
        Err(_) => {
            crate::ffi::errno::set_raw(84);
            usize::MAX
        }
    }
}
