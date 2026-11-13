// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The locale seam of the regex crate (RD-new: `RegexLocale`).
//!
//! `regcomp` and `regexec` read `MB_CUR_MAX` from the calling thread's locale and use the
//! matching object: [`ByteLocale`] for `C`/`POSIX` (`MB_CUR_MAX == 1`, bytes pass through) and
//! [`Utf8Locale`] for every UTF-8 locale (`MB_CUR_MAX == 4`). The wide classification, case
//! mapping and the UTF-8 decoder come from `raven-libc-ctype`; this crate owns no Unicode
//! tables. The shell (C48 later) selects the object.

use raven_libc_ctype::{
    MB_CUR_MAX_BYTE, MB_CUR_MAX_UTF8, MbMode, MbState, Mbr, iswalnum, iswalpha, iswctype, iswlower,
    iswupper, mbrtowc, towlower, towupper, wctype,
};

/// One wide character class name understood by `[[:name:]]` and `wctype`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Wctype {
    /// `alnum`.
    Alnum,
    /// `alpha`.
    Alpha,
    /// `blank`.
    Blank,
    /// `cntrl`.
    Cntrl,
    /// `digit`.
    Digit,
    /// `graph`.
    Graph,
    /// `lower`.
    Lower,
    /// `print`.
    Print,
    /// `punct`.
    Punct,
    /// `space`.
    Space,
    /// `upper`.
    Upper,
    /// `xdigit`.
    Xdigit,
}

impl Wctype {
    /// The `wctype_t` value of this class.
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::Alnum => 1,
            Self::Alpha => 2,
            Self::Blank => 3,
            Self::Cntrl => 4,
            Self::Digit => 5,
            Self::Graph => 6,
            Self::Lower => 7,
            Self::Print => 8,
            Self::Punct => 9,
            Self::Space => 10,
            Self::Upper => 11,
            Self::Xdigit => 12,
        }
    }

    /// Decode a `wctype_t` value; `0` is an unknown class.
    #[must_use]
    pub const fn from_code(code: u64) -> Option<Self> {
        match code {
            1 => Some(Self::Alnum),
            2 => Some(Self::Alpha),
            3 => Some(Self::Blank),
            4 => Some(Self::Cntrl),
            5 => Some(Self::Digit),
            6 => Some(Self::Graph),
            7 => Some(Self::Lower),
            8 => Some(Self::Print),
            9 => Some(Self::Punct),
            10 => Some(Self::Space),
            11 => Some(Self::Upper),
            12 => Some(Self::Xdigit),
            _ => None,
        }
    }
}

/// The result of one `mbrtowc` step.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum MbResult {
    /// A complete character and the number of input bytes consumed.
    Ok(u32, usize),
    /// The input ends in the middle of a character.
    Incomplete,
    /// An invalid byte sequence.
    Invalid,
}

/// The locale operations the engine needs.
pub trait RegexLocale {
    /// `MB_CUR_MAX` of the calling thread's locale.
    fn mb_cur_max(&self) -> usize;
    /// Decode one character from the start of `s`, updating `st`.
    fn mbrtowc(&self, s: &[u8], st: &mut MbState) -> MbResult;
    /// Look up a wide character class name, without NUL.
    fn wctype(&self, name: &[u8]) -> Option<Wctype>;
    /// Classify `c` under a class returned by [`RegexLocale::wctype`].
    fn iswctype(&self, c: u32, t: Wctype) -> bool;
    /// `iswalpha`.
    fn iswalpha(&self, c: u32) -> bool;
    /// `iswupper`.
    fn iswupper(&self, c: u32) -> bool;
    /// `iswlower`.
    fn iswlower(&self, c: u32) -> bool;
    /// `iswalnum`.
    fn iswalnum(&self, c: u32) -> bool;
    /// `towlower`.
    fn towlower(&self, c: u32) -> u32;
    /// `towupper`.
    fn towupper(&self, c: u32) -> u32;
}

/// `C`/`POSIX`: one byte per code unit, bytes `0x00..=0xFF` pass through unchanged.
#[derive(Clone, Copy, Debug, Default)]
pub struct ByteLocale;

impl RegexLocale for ByteLocale {
    fn mb_cur_max(&self) -> usize {
        MB_CUR_MAX_BYTE
    }

    fn mbrtowc(&self, s: &[u8], _st: &mut MbState) -> MbResult {
        match s.first() {
            Some(&byte) => MbResult::Ok(u32::from(byte), 1),
            None => MbResult::Incomplete,
        }
    }

    fn wctype(&self, name: &[u8]) -> Option<Wctype> {
        Wctype::from_code(wctype(name))
    }

    fn iswctype(&self, c: u32, t: Wctype) -> bool {
        iswctype(c, t.code())
    }

    fn iswalpha(&self, c: u32) -> bool {
        iswalpha(c)
    }

    fn iswupper(&self, c: u32) -> bool {
        iswupper(c)
    }

    fn iswlower(&self, c: u32) -> bool {
        iswlower(c)
    }

    fn iswalnum(&self, c: u32) -> bool {
        iswalnum(c)
    }

    fn towlower(&self, c: u32) -> u32 {
        towlower(c)
    }

    fn towupper(&self, c: u32) -> u32 {
        towupper(c)
    }
}

/// Every UTF-8 locale: `MB_CUR_MAX == 4`, validated UTF-8 decoding.
#[derive(Clone, Copy, Debug, Default)]
pub struct Utf8Locale;

impl RegexLocale for Utf8Locale {
    fn mb_cur_max(&self) -> usize {
        MB_CUR_MAX_UTF8
    }

    fn mbrtowc(&self, s: &[u8], st: &mut MbState) -> MbResult {
        match mbrtowc(st, MbMode::Utf8, s) {
            Mbr::Char { wc, used } => MbResult::Ok(wc, used),
            Mbr::Incomplete => MbResult::Incomplete,
            Mbr::Ilseq => MbResult::Invalid,
        }
    }

    fn wctype(&self, name: &[u8]) -> Option<Wctype> {
        Wctype::from_code(wctype(name))
    }

    fn iswctype(&self, c: u32, t: Wctype) -> bool {
        iswctype(c, t.code())
    }

    fn iswalpha(&self, c: u32) -> bool {
        iswalpha(c)
    }

    fn iswupper(&self, c: u32) -> bool {
        iswupper(c)
    }

    fn iswlower(&self, c: u32) -> bool {
        iswlower(c)
    }

    fn iswalnum(&self, c: u32) -> bool {
        iswalnum(c)
    }

    fn towlower(&self, c: u32) -> u32 {
        towlower(c)
    }

    fn towupper(&self, c: u32) -> u32 {
        towupper(c)
    }
}

#[cfg(test)]
#[path = "locale_ut.rs"]
mod tests;
