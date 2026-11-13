// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The multibyte and wide-character seam shared by `fnmatch` and `glob` (doc 07 section 4.6).
//!
//! Two implementations cover the locales of H-3: [`ByteLocale`] for `C`/`POSIX`
//! (`MB_CUR_MAX == 1`, bytes `0x80..=0xFF` pass through as code units) and [`Utf8Locale`] for
//! every UTF-8 locale (`MB_CUR_MAX == 4`). Both are thin views over `raven-libc-ctype`; the
//! calling thread's locale object and every `errno` store stay in the shell.

use raven_libc_ctype::mode::MbMode;
use raven_libc_ctype::utf8::internal::codeunit;
use raven_libc_ctype::utf8::{mbtowc, wcrtomb};
use raven_libc_ctype::wctype::{iswctype, towlower, towupper, wctype};

/// One decoded character, the number of bytes consumed, or a failure.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum MbResult {
    /// A code point and the bytes consumed from the input.
    Char {
        /// The decoded code point.
        wc: u32,
        /// Bytes consumed.
        len: usize,
    },
    /// The input ends in the middle of a character.
    Incomplete,
    /// An invalid sequence.
    Invalid,
}

/// A `wctype_t` value; `0` is "unknown class" and is represented by `None` at the seam.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct Wctype(pub u64);

/// The locale operations `fnmatch` and `glob` need, without global locale state.
pub trait PatternLocale {
    /// `MB_CUR_MAX`: 1 in `C`/`POSIX`, 4 in a UTF-8 locale.
    fn mb_cur_max(&self) -> usize;
    /// Decode one character from the start of `s`.
    fn mbtowc(&self, s: &[u8]) -> MbResult;
    /// Map a class name (without NUL) to a type value; `None` for an unknown name.
    fn wctype(&self, name: &[u8]) -> Option<Wctype>;
    /// Test a code point against a type value.
    fn iswctype(&self, c: u32, t: Wctype) -> bool;
    /// Simple uppercase mapping.
    fn towupper(&self, c: u32) -> u32;
    /// Simple lowercase mapping.
    fn towlower(&self, c: u32) -> u32;
}

/// The conversion mode implied by `MB_CUR_MAX`.
#[must_use]
pub fn mode_of(loc: &dyn PatternLocale) -> MbMode {
    if loc.mb_cur_max() == 1 {
        MbMode::Byte
    } else {
        MbMode::Utf8
    }
}

/// Encode one code point; the number of bytes is returned, `0` when unencodable.
///
/// The caller falls back to one raw byte on `0`, as the donor does (`glob.c:1087-1091`).
pub fn encode(loc: &dyn PatternLocale, wc: u32) -> ([u8; 4], usize) {
    let mut out = [0u8; 4];
    match wcrtomb(mode_of(loc), wc, &mut out) {
        Ok(n) => (out, n),
        Err(_) => ([0u8; 4], 0),
    }
}

/// The `C`/`POSIX` locale: every byte is one code unit.
#[derive(Clone, Copy, Debug, Default)]
pub struct ByteLocale;

impl PatternLocale for ByteLocale {
    fn mb_cur_max(&self) -> usize {
        1
    }

    fn mbtowc(&self, s: &[u8]) -> MbResult {
        match s.first() {
            Some(&b) => MbResult::Char {
                wc: codeunit(b),
                len: 1,
            },
            None => MbResult::Invalid,
        }
    }

    fn wctype(&self, name: &[u8]) -> Option<Wctype> {
        match wctype(name) {
            0 => None,
            t => Some(Wctype(t)),
        }
    }

    fn iswctype(&self, c: u32, t: Wctype) -> bool {
        iswctype(c, t.0)
    }

    fn towupper(&self, c: u32) -> u32 {
        towupper(c)
    }

    fn towlower(&self, c: u32) -> u32 {
        towlower(c)
    }
}

/// Every UTF-8 locale (H-3): validated UTF-8, `MB_CUR_MAX == 4`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Utf8Locale;

impl PatternLocale for Utf8Locale {
    fn mb_cur_max(&self) -> usize {
        4
    }

    fn mbtowc(&self, s: &[u8]) -> MbResult {
        match mbtowc(MbMode::Utf8, s) {
            Ok((wc, len)) => MbResult::Char { wc, len },
            Err(_) => MbResult::Invalid,
        }
    }

    fn wctype(&self, name: &[u8]) -> Option<Wctype> {
        match wctype(name) {
            0 => None,
            t => Some(Wctype(t)),
        }
    }

    fn iswctype(&self, c: u32, t: Wctype) -> bool {
        iswctype(c, t.0)
    }

    fn towupper(&self, c: u32) -> u32 {
        towupper(c)
    }

    fn towlower(&self, c: u32) -> u32 {
        towlower(c)
    }
}

#[cfg(test)]
#[path = "locale_ut.rs"]
mod tests;
