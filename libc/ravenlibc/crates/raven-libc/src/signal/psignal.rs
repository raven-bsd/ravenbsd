// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `psignal(3)` and `psiginfo(3)`: the C entry points over the pure text layout of
//! `raven-libc-signal`, writing to standard error.

use core::ffi::{c_char, c_int};

use raven_libc_signal::psignal::psignal_text_with;
use raven_libc_str::errmsg::strsignal::signal_message;

/// The stack buffer for one message; a longer message is truncated.
const MESSAGE_SIZE: usize = 512;

/// Write the text to standard error.
fn write_error(bytes: &[u8]) {
    let platform = crate::ffi::platform::global();
    let mut written = 0usize;
    while let Some(rest) = bytes.get(written..) {
        match platform.write(2, rest) {
            Ok(0) | Err(_) => return,
            Ok(count) => written = written.saturating_add(count),
        }
    }
}

/// `psignal(3)`: print `msg: <signal description>` on standard error.
///
/// # Safety
/// `msg` is NULL or a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psignal(sig: c_int, msg: *const c_char) {
    // SAFETY: the caller's C string contract.
    let message = unsafe { crate::ffi::cstr::slice(msg) };
    let text = signal_message(sig).unwrap_or(b"Unknown signal");
    let mut out = [0u8; MESSAGE_SIZE];
    let length = psignal_text_with(sig, message, text, &mut out);
    write_error(out.get(..length).unwrap_or(&[]));
}

/// `psiginfo(3)`: print the message for the record's signal.
///
/// # Safety
/// `info` is a readable `siginfo_t`; `msg` is NULL or a readable NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn psiginfo(
    info: *const raven_libc_abi::structs::SiginfoT,
    msg: *const c_char,
) {
    if info.is_null() {
        return;
    }
    // SAFETY: the caller provides a readable `siginfo_t` and a C string.
    unsafe {
        let record = &*info;
        let message = crate::ffi::cstr::slice(msg);
        let text = signal_message(record.si_signo).unwrap_or(b"Unknown signal");
        let mut out = [0u8; MESSAGE_SIZE];
        let length = psignal_text_with(record.si_signo, message, text, &mut out);
        write_error(out.get(..length).unwrap_or(&[]));
    }
}
