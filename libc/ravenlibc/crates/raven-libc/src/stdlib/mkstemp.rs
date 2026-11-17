// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `mkstemp(3)`: replace the trailing `XXXXXX` of a template with a unique suffix and create the
//! file with `O_RDWR|O_CREAT|O_EXCL`.
//!
//! The template checks and the character set live in `raven-libc-stdlib`; this file owns the
//! `open` retry loop, the entropy source and the caller's buffer write.

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{EEXIST, EINVAL, Errno};
use raven_libc_abi::fcntl::{O_CREAT, O_EXCL, O_RDWR};
use raven_libc_stdlib::path::mkstemp::{SUFFIX_LEN, encode_suffix, is_template, substitute};

/// The `mkstemp` retry budget.
const MAXTRIES: usize = 100;
/// The create mode (the caller's umask is applied by the platform).
const MODE: u32 = 0o600;

/// `mkstemp(3)`: create a unique file from `template`.
///
/// # Safety
/// `template` points to a writable NUL-terminated string whose last six characters are `XXXXXX`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mkstemp(template: *mut c_char) -> c_int {
    if template.is_null() {
        crate::ffi::errno::set(Errno(EINVAL as u16));
        return -1;
    }
    // SAFETY: the caller's NUL-terminated string contract.
    let length = unsafe { crate::ffi::cstr::slice(template) }.len();
    // SAFETY: the same string is writable for its NUL-terminated length.
    let path = unsafe { crate::ffi::cstr::slice_mut(template, length) };
    if !is_template(path) {
        crate::ffi::errno::set(Errno(EINVAL as u16));
        return -1;
    }
    let flags = u32::try_from(O_RDWR | O_CREAT | O_EXCL).unwrap_or(0);
    let mut attempt = 0usize;
    while attempt < MAXTRIES {
        let mut raw = [0u8; SUFFIX_LEN];
        // SAFETY: the six bytes are writable; `arc4random_buf` fills them or exits.
        unsafe { crate::rand::arc4random_buf(raw.as_mut_ptr().cast(), SUFFIX_LEN) };
        let mut suffix = [0u8; SUFFIX_LEN];
        encode_suffix(&raw, &mut suffix);
        if !substitute(path, &suffix) {
            crate::ffi::errno::set(Errno(EINVAL as u16));
            return -1;
        }
        match crate::ffi::platform::global().open(path, flags, MODE) {
            Ok(fd) => return fd,
            Err(error) if error.0 == EEXIST as u16 => {}
            Err(error) => {
                crate::ffi::errno::set(Errno(error.0));
                return -1;
            }
        }
        attempt = attempt.saturating_add(1);
    }
    crate::ffi::errno::set(Errno(EEXIST as u16));
    -1
}
