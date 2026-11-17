// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fmtmsg(3)` from `<fmtmsg.h>`.
//!
//! The C-locale form: `label: text`, then `TO FIX: action` and `tag` when present. The message
//! goes to standard error for `MM_PRINT`; `MM_CONSOLE` has no separate console on this base, so
//! it writes to the same stream. The return is `MM_OK` when everything was written.

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::Errno;

const MM_PRINT: i64 = 256;
const MM_CONSOLE: i64 = 512;
const MM_NOTOK: c_int = -1;
const MM_OK: c_int = 0;
const MM_NOMSG: c_int = 1;

/// Append `bytes` to `out` at `position`; false when it does not fit.
fn push(out: &mut [u8], position: &mut usize, bytes: &[u8]) -> bool {
    let Some(end) = position.checked_add(bytes.len()) else {
        return false;
    };
    let Some(slot) = out.get_mut(*position..end) else {
        return false;
    };
    slot.copy_from_slice(bytes);
    *position = end;
    true
}

/// `fmtmsg`.
///
/// # Safety
/// Every pointer argument is NULL or a NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmtmsg(
    classification: i64,
    label: *const c_char,
    _severity: c_int,
    text: *const c_char,
    action: *const c_char,
    tag: *const c_char,
) -> c_int {
    if classification & (MM_PRINT | MM_CONSOLE) == 0 {
        return MM_NOMSG;
    }
    let mut line = [0u8; 1024];
    let mut position = 0usize;
    let mut ok = true;
    if !label.is_null() {
        // SAFETY: the caller's C string.
        let label = unsafe { crate::ffi::cstr::slice(label) };
        ok = ok && push(&mut line, &mut position, label) && push(&mut line, &mut position, b": ");
    }
    if !text.is_null() {
        // SAFETY: the caller's C string.
        let text = unsafe { crate::ffi::cstr::slice(text) };
        ok = ok && push(&mut line, &mut position, text);
    }
    if !action.is_null() {
        // SAFETY: the caller's C string.
        let action = unsafe { crate::ffi::cstr::slice(action) };
        ok = ok
            && push(&mut line, &mut position, b"\nTO FIX: ")
            && push(&mut line, &mut position, action);
    }
    if !tag.is_null() {
        // SAFETY: the caller's C string.
        let tag = unsafe { crate::ffi::cstr::slice(tag) };
        ok = ok && push(&mut line, &mut position, b"  ") && push(&mut line, &mut position, tag);
    }
    ok = ok && push(&mut line, &mut position, b"\n");
    if !ok {
        crate::ffi::errno::set(Errno(34));
        return MM_NOTOK;
    }
    let bytes = line.get(..position).unwrap_or(&[]);
    match crate::ffi::platform::global().write(2, bytes) {
        Ok(_) => MM_OK,
        Err(_) => MM_NOTOK,
    }
}
