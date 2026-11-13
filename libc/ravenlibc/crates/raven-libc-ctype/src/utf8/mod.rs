// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The UTF-8 and byte-transparent multibyte codec (doc 02 3.1).
//!
//! Every C function of `musl:src/multibyte/` has one Rust file here, and the two NetBSD C23
//! `char8_t` functions have one file each. State is always an explicit `&mut MbState`; the
//! thread-local null-`ps` objects and `errno` belong to the shell (A-ctype-9, guide 3.12).

pub mod bittab;
pub mod internal;

mod btowc;
mod c16rtomb;
mod c32rtomb;
mod c8rtomb;
mod mblen;
mod mbrlen;
mod mbrtoc16;
mod mbrtoc32;
mod mbrtoc8;
mod mbrtowc;
mod mbsinit;
mod mbsnrtowcs;
mod mbsrtowcs;
mod mbstowcs;
mod mbtowc;
mod wcrtomb;
mod wcsnrtombs;
mod wcsrtombs;
mod wcstombs;
mod wctob;
mod wctomb;

pub use bittab::BITTAB;
pub use btowc::btowc;
pub use c8rtomb::c8rtomb;
pub use c16rtomb::c16rtomb;
pub use c32rtomb::c32rtomb;
pub use internal::{SA, SB, codeunit, is_codeunit, oob};
pub use mblen::mblen;
pub use mbrlen::mbrlen;
pub use mbrtoc8::mbrtoc8;
pub use mbrtoc16::mbrtoc16;
pub use mbrtoc32::mbrtoc32;
pub use mbrtowc::{mbrtowc, mbrtowc_reset};
pub use mbsinit::mbsinit;
pub use mbsnrtowcs::mbsnrtowcs;
pub use mbsrtowcs::mbsrtowcs;
pub use mbstowcs::mbstowcs;
pub use mbtowc::mbtowc;
pub use wcrtomb::wcrtomb;
pub use wcsnrtombs::wcsnrtombs;
pub use wcsrtombs::wcsrtombs;
pub use wcstombs::wcstombs;
pub use wctob::wctob;
pub use wctomb::wctomb;

use crate::errors::MbError;
use crate::state::MbState;

/// The outcome of `mbrtowc`/`mbrtoc32`/`mbrlen`: a decoded character, incomplete input or an error.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Mbr {
    /// A complete character and the number of input bytes consumed.
    Char {
        /// Decoded code unit.
        wc: u32,
        /// Bytes consumed from the input slice.
        used: usize,
    },
    /// The input ends in the middle of a character; the state holds the partial character.
    Incomplete,
    /// An invalid sequence; the state has been reset.
    Ilseq,
}

impl Mbr {
    /// The C return value: `0` for a NUL, `used`, `(size_t)-2` or `(size_t)-1`.
    #[must_use]
    pub const fn c_return(self) -> isize {
        match self {
            Self::Char { wc, used } => {
                if wc == 0 {
                    0
                } else {
                    used as isize
                }
            }
            Self::Incomplete => -2,
            Self::Ilseq => -1,
        }
    }
}

/// The outcome of `mbrtoc16`: a UTF-16 code unit, a pending low surrogate or an error.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Mbr16 {
    /// A code unit and the number of input bytes consumed.
    Char {
        /// UTF-16 code unit.
        unit: u16,
        /// Bytes consumed from the input slice.
        used: usize,
    },
    /// A buffered low surrogate was returned without consuming input (C return `-3`).
    PendingLow {
        /// The pending low surrogate.
        unit: u16,
    },
    /// The input ends in the middle of a character.
    Incomplete,
    /// An invalid sequence; the state has been reset.
    Ilseq,
}

impl Mbr16 {
    /// The C return value; a NUL unit returns `0`.
    #[must_use]
    pub const fn c_return(self) -> isize {
        match self {
            Self::Char { unit, used } => {
                if unit == 0 {
                    0
                } else {
                    used as isize
                }
            }
            Self::PendingLow { .. } => -3,
            Self::Incomplete => -2,
            Self::Ilseq => -1,
        }
    }
}

/// The outcome of `mbrtoc8`: a UTF-8 code unit, a buffered continuation byte or an error.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Mbr8 {
    /// A code unit and the number of input bytes consumed.
    Char {
        /// UTF-8 code unit.
        unit: u8,
        /// Bytes consumed from the input slice.
        used: usize,
    },
    /// A buffered continuation byte was returned without consuming input (C return `-3`).
    Continuation {
        /// The pending byte.
        unit: u8,
    },
    /// The input ends in the middle of a character.
    Incomplete,
    /// An invalid sequence; the state has been reset.
    Ilseq,
}

impl Mbr8 {
    /// The C return value; a NUL byte returns `0`.
    #[must_use]
    pub const fn c_return(self) -> isize {
        match self {
            Self::Char { unit, used } => {
                if unit == 0 {
                    0
                } else {
                    used as isize
                }
            }
            Self::Continuation { .. } => -3,
            Self::Incomplete => -2,
            Self::Ilseq => -1,
        }
    }
}

/// Where the source pointer of a string conversion ends.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum SrcPos {
    /// The conversion consumed the terminating NUL; the C stores a null pointer.
    Nul,
    /// The conversion stopped at this byte/character offset.
    At(usize),
}

/// One string conversion result: return value, final source position and final state.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct StrConv {
    /// Number of converted units, or the error.
    pub ret: Result<usize, MbError>,
    /// Where the source pointer ends.
    pub src: SrcPos,
    /// The conversion state after the call.
    pub state: MbState,
}

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
