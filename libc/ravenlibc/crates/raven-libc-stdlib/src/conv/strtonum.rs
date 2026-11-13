// SPDX-License-Identifier: ISC
// Copyright (c) 2004 Ted Unangst and Todd Miller
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/strtonum.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: strtonum.c,v 1.8 2015/09/13 08:31:48 guenther Exp $)
//
// Permission to use, copy, modify, and distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE. IN NO EVENT SHALL
// THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

//! `strtonum`: a bounded decimal conversion with the OpenBSD error texts.

use super::strtol::strtol;
use crate::IntStatus;

/// Why `strtonum` rejected the input.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum StrtonumError {
    /// Not a complete decimal number, or `lo > hi`.
    Invalid,
    /// Below `lo`, including negative overflow.
    TooSmall,
    /// Above `hi`, including positive overflow.
    TooLarge,
}

impl StrtonumError {
    /// The static message without its terminating NUL.
    #[must_use]
    pub const fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Invalid => b"invalid",
            Self::TooSmall => b"too small",
            Self::TooLarge => b"too large",
        }
    }

    /// The NUL-terminated form the shell stores in `*errstrp`.
    #[must_use]
    pub const fn as_c_bytes(self) -> &'static [u8] {
        match self {
            Self::Invalid => b"invalid\0",
            Self::TooSmall => b"too small\0",
            Self::TooLarge => b"too large\0",
        }
    }
}

/// Convert all of `s` as a base-10 `i64` in `[lo, hi]`.
///
/// On success the shell leaves `errno` as it was; on failure it sets `EINVAL` or `ERANGE` and
/// returns 0, exactly as the donor's `errval` table does.
///
/// # Errors
///
/// [`StrtonumError::Invalid`] for trailing junk, no digits or `lo > hi`; the other two for a
/// value (or a saturated overflow) outside the bounds.
pub fn strtonum(s: &[u8], lo: i64, hi: i64) -> Result<i64, StrtonumError> {
    if lo > hi {
        return Err(StrtonumError::Invalid);
    }
    let (value, consumed, status) = strtol(s, 10);
    if consumed == 0 || consumed != s.len() {
        return Err(StrtonumError::Invalid);
    }
    if status == IntStatus::Range {
        // RavenBSD: the donor tests the saturated value against LLONG_MIN/MAX; the sign of the
        // saturated range is already known here.
        return Err(if value < 0 {
            StrtonumError::TooSmall
        } else {
            StrtonumError::TooLarge
        });
    }
    if value < lo {
        return Err(StrtonumError::TooSmall);
    }
    if value > hi {
        return Err(StrtonumError::TooLarge);
    }
    Ok(value)
}

#[cfg(test)]
#[path = "strtonum_ut.rs"]
mod tests;
