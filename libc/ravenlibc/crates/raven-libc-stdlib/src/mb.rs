// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The stdlib adapters over the C39 UTF-8 codec: `mblen`, `mbtowc`, `wctomb`.
//!
//! These functions are **not restartable** (T-18): unlike `mbrtowc` they carry no state and a
//! `NULL` argument returns 0 (stateless encoding). The decoder itself is
//! `raven-libc-ctype`'s; this module only maps its `Result` to the C return values.

use raven_libc_ctype::utf8;

/// `mblen(s, n)`; `s == None` returns 0, an empty input or an invalid sequence returns -1.
#[must_use]
pub fn mblen(s: Option<&[u8]>, mode: raven_libc_ctype::MbMode) -> i32 {
    let Some(input) = s else {
        return 0;
    };
    if input.is_empty() {
        return -1;
    }
    match utf8::mblen(mode, input) {
        Ok(used) => i32::try_from(used).unwrap_or(-1),
        Err(_) => -1,
    }
}

/// `mbtowc(pwc, s, n)`: `(length or -1, code unit)`.
#[must_use]
pub fn mbtowc(s: Option<&[u8]>, mode: raven_libc_ctype::MbMode) -> (i32, u32) {
    let Some(input) = s else {
        return (0, 0);
    };
    if input.is_empty() {
        return (-1, 0);
    }
    match utf8::mbtowc(mode, input) {
        Ok((unit, used)) => (i32::try_from(used).unwrap_or(-1), unit),
        Err(_) => (-1, 0),
    }
}

/// `wctomb(s, wc)`: the byte count, or -1 for a non-representable code unit.
pub fn wctomb(wc: u32, mode: raven_libc_ctype::MbMode, out: &mut [u8; 4]) -> i32 {
    match utf8::wctomb(mode, wc, out) {
        Ok(used) => i32::try_from(used).unwrap_or(-1),
        Err(_) => -1,
    }
}

#[cfg(test)]
#[path = "mb_ut.rs"]
mod tests;
