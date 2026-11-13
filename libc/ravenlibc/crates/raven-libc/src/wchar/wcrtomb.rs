// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `wcrtomb(3)`.
use core::ffi::c_char;

use raven_libc_ctype::{MbMode, MbState};

/// Encode `wc` into `s` in the UTF-8 locale.
///
/// # Safety
/// `s` has four writable bytes (or is NULL); `state` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcrtomb(s: *mut c_char, wc: u32, state: *mut MbState) -> usize {
    let mut local = MbState::INITIAL;
    // SAFETY: the caller passes either NULL or a writable `mbstate_t`.
    let state = unsafe { state.as_mut() }.unwrap_or(&mut local);
    *state = MbState::INITIAL;
    let mut buffer = [0u8; 4];
    match raven_libc_ctype::wcrtomb(MbMode::Utf8, wc, &mut buffer) {
        Ok(length) => {
            if !s.is_null() {
                let bytes = buffer.get(..length).unwrap_or(&buffer);
                // SAFETY: the caller grants four writable bytes; `bytes.len() <= 4`.
                unsafe { crate::ffi::out::write_bytes(s.cast(), bytes, 4) };
            }
            length
        }
        Err(_) => {
            crate::ffi::errno::set_raw(84);
            usize::MAX
        }
    }
}
