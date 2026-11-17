// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `c16rtomb`.

use core::ffi::c_char;

use raven_libc_ctype::{MbMode, MbState};

/// `c16rtomb`.
///
/// # Safety
/// `s` has four writable bytes (or is NULL); `state` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn c16rtomb(s: *mut c_char, c16: u16, state: *mut MbState) -> usize {
    let mut local = MbState::INITIAL;
    // SAFETY: the caller's state contract.
    let state = unsafe { super::uchar::state_or_local(state, &mut local) };
    let mut buffer = [0u8; 4];
    match raven_libc_ctype::c16rtomb(state, MbMode::Utf8, c16, &mut buffer) {
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
