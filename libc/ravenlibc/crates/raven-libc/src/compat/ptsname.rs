// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ptsname`: the `/dev/pts/<N>` slave name of a master descriptor.
//!
//! The pty server names pairs by id; `ptsname_r` formats the `/dev/pts/<id>` path. POSIX
//! `ptsname` returns a static buffer, so this shell copies the same name into a per-process
//! buffer (no allocation on the pty path).
//!
//! This export must remain a defining member of the public static archive. `archive_exports`
//! (K2) checks `Symbols.list` against `libc.a`; listing the name in `Symbols.list`/`Symbol.map`
//! without a defining shell member is the K436 merge failure.

use core::ffi::{c_char, c_int};
use core::ptr::null_mut;

use super::ptsname_r::ptsname_r;

/// The POSIX static result buffer. The pty path is short; 128 bytes matches the libutil donor.
const BUFFER_LEN: usize = 128;
static mut BUFFER: [c_char; BUFFER_LEN] = [0; BUFFER_LEN];

/// `ptsname`.
///
/// # Safety
/// Scalar argument only; the returned pointer is the static buffer and is valid until the next
/// `ptsname` call in this process.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ptsname(fd: c_int) -> *mut c_char {
    // SAFETY: `BUFFER` is this process's writable static storage; `ptsname_r` writes a bounded,
    // NUL-terminated name and returns 0 on success.
    let result = unsafe {
        let pointer = core::ptr::addr_of_mut!(BUFFER).cast::<c_char>();
        ptsname_r(fd, pointer, BUFFER_LEN)
    };
    if result != 0 {
        return null_mut();
    }
    // SAFETY: the same static buffer, now holding the NUL-terminated name.
    unsafe { core::ptr::addr_of_mut!(BUFFER).cast::<c_char>() }
}

#[cfg(test)]
#[path = "ptsname_ut.rs"]
mod tests;
