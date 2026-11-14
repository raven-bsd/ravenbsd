// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `l64a(3)`.
use core::ffi::{c_char, c_long};

static mut BUFFER: [u8; 8] = [0; 8];

/// Encode `value` into the static eight-byte result buffer.
#[unsafe(no_mangle)]
pub extern "C" fn l64a(value: c_long) -> *mut c_char {
    // SAFETY: one process static buffer; single-threaded until C57.
    let buffer = unsafe { &mut *(&raw mut BUFFER) };
    raven_libc_stdlib::misc::l64a(value, buffer);
    buffer.as_mut_ptr().cast::<c_char>()
}
