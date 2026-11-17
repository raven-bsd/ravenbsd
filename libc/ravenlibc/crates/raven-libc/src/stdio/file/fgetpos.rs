// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fgetpos`/`fsetpos` and their 64-bit aliases.
//!
//! RavenBSD's `fpos_t` is the 16-byte opaque C type from `<stdio.h>`. The first eight bytes hold
//! the current `off_t`; the rest is zeroed. The 64-bit names are the same bodies because `off_t`
//! is 64-bit on every target.

use core::ffi::{c_int, c_void};

use raven_libc_abi::errno::Errno;

/// The C `fpos_t` storage.
pub type Fpos = [u8; 16];

const SEEK_SET: c_int = 0;
const INVALID: Errno = Errno(22);

/// `fgetpos`.
///
/// # Safety
/// `stream` is a FILE and `pos` is a writable `fpos_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgetpos(stream: *mut c_void, pos: *mut Fpos) -> c_int {
    if pos.is_null() {
        return crate::ffi::ret::int(Err(INVALID));
    }
    // SAFETY: forwarded C contract.
    let position = unsafe { super::ftell_impl(stream) };
    if position < 0 {
        return -1;
    }
    let bytes = position.to_ne_bytes();
    // SAFETY: the caller grants a writable 16-byte fpos_t.
    unsafe {
        let target = &mut *pos;
        target.fill(0);
        if let Some(slot) = target.get_mut(..bytes.len()) {
            slot.copy_from_slice(&bytes);
        }
    }
    0
}

/// `fsetpos`.
///
/// # Safety
/// `stream` is a FILE and `pos` is a readable `fpos_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fsetpos(stream: *mut c_void, pos: *const Fpos) -> c_int {
    if pos.is_null() {
        return crate::ffi::ret::int(Err(INVALID));
    }
    // SAFETY: the caller grants a readable 16-byte fpos_t.
    let offset = unsafe { (*pos).get(..8) }
        .and_then(|bytes| <[u8; 8]>::try_from(bytes).ok())
        .map(i64::from_ne_bytes)
        .unwrap_or(0);
    // SAFETY: forwarded C contract.
    unsafe { super::fseek_impl(stream, offset, SEEK_SET) }
}

/// `fgetpos64`.
///
/// # Safety
/// As `fgetpos`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgetpos64(stream: *mut c_void, pos: *mut Fpos) -> c_int {
    // SAFETY: as `fgetpos`.
    unsafe { fgetpos(stream, pos) }
}

/// `fsetpos64`.
///
/// # Safety
/// As `fsetpos`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fsetpos64(stream: *mut c_void, pos: *const Fpos) -> c_int {
    // SAFETY: as `fsetpos`.
    unsafe { fsetpos(stream, pos) }
}
