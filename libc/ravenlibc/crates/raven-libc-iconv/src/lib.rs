// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: iconv ported from musl:src/locale/{iconv,iconv_close}.c @
// 7813e9119eb5fe681b6750b8a033a0490bca0941 (no per-file header; covered by
// musl/COPYRIGHT). Patches P1/P2/P3 are documented at their sites.

//! `iconv_open`/`iconv`/`iconv_close` logic over static tables.

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

mod charset;
mod convert;
mod tables_8bit;

pub use charset::Charset;
pub use convert::Iconv;

/// An `iconv` failure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconvError {
    /// The charset pair is not supported (`EINVAL` at open).
    Unsupported,
    /// An invalid input sequence (`EILSEQ`).
    IllegalSequence,
    /// A truncated input sequence (`EINVAL`).
    Incomplete,
    /// The output buffer is too small (`E2BIG`).
    TooBig,
}

/// A conversion mode selected by `//TRANSLIT` and `//IGNORE`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    /// Reject an unmappable character.
    #[default]
    Strict,
    /// Substitute `?` and count the conversion.
    Translit,
    /// Skip an unmappable character.
    Ignore,
}

/// `iconv_open`.
pub fn iconv_open(to: &[u8], from: &[u8]) -> Result<Iconv, IconvError> {
    Iconv::open(to, from)
}

/// `iconv_close` is a no-op in this pure layer.
pub fn iconv_close(_cd: Iconv) {}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
