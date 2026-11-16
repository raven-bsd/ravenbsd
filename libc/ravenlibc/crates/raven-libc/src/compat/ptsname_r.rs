// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `ptsname_r` C-ABI placeholder.
//!
//! RavenBSD has no pty scheme yet (PG-54), so the entry point is an explicit `ENOSYS` rather than
//! pretending that a `/dev/ptmx`-style node exists.

use core::ffi::{c_char, c_int};

use super::unsupported;

/// `ptsname_r` (PG-54).
///
/// # Safety
/// `buffer` is NULL or writable for `length` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ptsname_r(_fd: c_int, _buffer: *mut c_char, _length: usize) -> c_int {
    unsupported()
}
