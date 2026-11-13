// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: locale model and language services. COPY sources are named per file.
// The musl files have no per-file header; musl/COPYRIGHT covers them.
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! Locale model, `langinfo`, `localeconv`, collation, wide strings, gettext and catalogs.
//!
//! The crate is `no_std` plus `alloc`, class P (`#![forbid(unsafe_code)]`). Locale state is
//! explicit (`LocaleState`); the shell owns the global lock, the per-thread `locale_t` slot and
//! every C symbol.

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
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: bounds-checked donor arithmetic.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

extern crate alloc;

pub mod casecmp;
pub mod collate;
pub mod gettext;
pub mod langinfo;
pub mod locale;
pub mod localeconv;
pub mod nls;
pub mod timelocale;
pub mod wstring;

pub use langinfo::{CODESET, nl_langinfo, nl_langinfo_name};
pub use locale::{
    EnvSource, LC_ALL, LC_ALL_MASK, LC_COLLATE, LC_CTYPE, LC_MESSAGES, LC_MONETARY, LC_NUMERIC,
    LC_TIME, LOCALE_NAME_MAX, Locale, LocaleArg, LocaleError, LocaleState, MAX_INTERNED, NameId,
    NameTable, uselocale,
};
pub use localeconv::{LConvData, localeconv};
pub use timelocale::{C_TIME, CTime, TimeLocale, time_locale};
