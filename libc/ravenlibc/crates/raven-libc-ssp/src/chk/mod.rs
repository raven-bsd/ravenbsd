// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The FORTIFY check cores, one module per FreeBSD `*_chk.c` file.
//!
//! The cores never touch a buffer and never call a string function; the C-ABI shell performs the
//! actual copy/format call after a core returns `Ok`. `slen == usize::MAX` means "unknown object
//! size" and is never an error (A-malloc-10, T-14).

pub mod fgets_chk;
pub mod memcpy_chk;
pub mod memmove_chk;
pub mod mempcpy_chk;
pub mod memset_chk;
pub mod snprintf_chk;
pub mod sprintf_chk;
pub mod stpcpy_chk;
pub mod stpncpy_chk;
pub mod strcat_chk;
pub mod strcpy_chk;
pub mod strlcat_chk;
pub mod strlcpy_chk;
pub mod strncat_chk;
pub mod strncpy_chk;
pub mod vsnprintf_chk;
pub mod vsprintf_chk;

/// The check failed; the shell turns this into `__chk_fail`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChkFail;

/// The C `strlen` over a slice whose end acts as the NUL terminator.
#[must_use]
pub(crate) fn strlen(bytes: &[u8]) -> usize {
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes.get(index) == Some(&0) {
            return index;
        }
        index = index.wrapping_add(1);
    }
    bytes.len()
}

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
