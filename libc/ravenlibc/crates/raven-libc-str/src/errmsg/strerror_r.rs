// SPDX-License-Identifier: LicenseRef-PublicDomain
// Written in 2017 by Ingo Schwarze <schwarze@openbsd.org>.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strerror_r.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (the donor header reads: "Written in 2017 by Ingo Schwarze <schwarze@openbsd.org>.
//  Released into the public domain." — no $OpenBSD$ tag in this checkout)
//
// This file is released into the public domain; the RavenBSD adaptation is under the same terms.

//! XSI `strerror_r`: `int` return, no `errno` write (A-str-7).

use super::strerror::{message, write_unknown};

/// XSI `strerror_r` result: the caller maps this to its `int` return value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Xsi {
    /// Success; `out` holds the NUL-terminated message.
    Ok,
    /// Unknown error number; `out` holds `"Unknown error: N"` (NUL terminated when it fits).
    Einval,
    /// The message was truncated; `out` holds a NUL-terminated prefix when it is not empty.
    Erange,
}

fn copy_terminated(out: &mut [u8], text: &[u8]) -> bool {
    if out.is_empty() {
        return false;
    }
    let usable = out.len().wrapping_sub(1);
    let mut index = 0usize;
    while index < usable {
        let Some(&byte) = text.get(index) else {
            break;
        };
        if let Some(slot) = out.get_mut(index) {
            *slot = byte;
        }
        index = index.wrapping_add(1);
    }
    if let Some(slot) = out.get_mut(index) {
        *slot = 0;
    }
    text.len() <= usable
}

/// Fill `out` with the NUL-terminated (possibly truncated) message for `e`.
///
/// Never touches `errno`. An unknown number returns [`Xsi::Einval`] with the BSD-style
/// `"Unknown error: N"` text; a known message that does not fit returns [`Xsi::Erange`] with a
/// NUL-terminated prefix. `n == 0` writes nothing and reports `Erange` (known) or `Einval`
/// (unknown).
#[must_use]
pub fn strerror_r_xsi(e: i32, out: &mut [u8]) -> Xsi {
    if let Some(text) = message(e) {
        if copy_terminated(out, text) {
            return Xsi::Ok;
        }
        return Xsi::Erange;
    }
    let mut buffer = [0u8; super::BUF_LEN];
    let written = write_unknown(&mut buffer, e);
    let text = match buffer.get(..written) {
        Some(text) => text,
        None => return Xsi::Einval,
    };
    let _ = copy_terminated(out, text);
    Xsi::Einval
}

#[cfg(test)]
#[path = "strerror_r_ut.rs"]
mod tests;
