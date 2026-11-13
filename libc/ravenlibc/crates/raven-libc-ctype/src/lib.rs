// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Character classification, case mapping and the UTF-8 multibyte codec.
//!
//! This crate is the pure-logic leaf of `architecture/raven-libc/donor-map/02-ctype-locale-i18n.md`
//! sections 3.1-3.3: the UTF-8 codec behind `mbrtowc`, the `<ctype.h>`/`<wctype.h>` families,
//! `wcwidth`/`wcswidth` and the generated Unicode 15.1.0 property and case tables. Locale state,
//! `errno`, thread-local null-`ps` states and every `extern "C"` symbol belong to the shell crate.
//!
//! The crate is `no_std`, allocation-free and `forbid(unsafe_code)` (class P).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

pub mod ctype;
pub mod errors;
pub mod mode;
pub mod state;
pub mod unicode;
pub mod utf8;
pub mod wcswidth;
pub mod wctype;
pub mod wcwidth;

pub use ctype::{
    isalnum, isalpha, isascii, isblank, iscntrl, isdigit, isgraph, islower, isprint, ispunct,
    isspace, isupper, isxdigit, toascii, tolower, toupper,
};
pub use errors::MbError;
pub use mode::{EOF, MB_CUR_MAX_BYTE, MB_CUR_MAX_UTF8, MbMode, WEOF};
pub use state::MbState;
pub use unicode::{TABLE_BYTES, UNICODE_VERSION};
pub use utf8::{
    BITTAB, Mbr, Mbr8, Mbr16, SA, SB, SrcPos, StrConv, btowc, c8rtomb, c16rtomb, c32rtomb,
    is_codeunit, mblen, mbrlen, mbrtoc8, mbrtoc16, mbrtoc32, mbrtowc, mbrtowc_reset, mbsinit,
    mbsnrtowcs, mbsrtowcs, mbstowcs, mbtowc, wcrtomb, wcsnrtombs, wcsrtombs, wcstombs, wctob,
    wctomb,
};
pub use wcswidth::wcswidth;
pub use wctype::{
    iswalnum, iswalpha, iswblank, iswcntrl, iswctype, iswdigit, iswgraph, iswlower, iswprint,
    iswpunct, iswspace, iswupper, iswxdigit, towctrans, towlower, towupper, wctrans, wctype,
};
pub use wcwidth::wcwidth;
