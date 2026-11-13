// SPDX-License-Identifier: MIT
// RavenBSD: transpiled from the pinned donor; see PROVENANCE.toml.
// Copyright (c) 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: the POSIX `lconv` static from musl:src/locale/localeconv.c @
// 7813e9119eb5fe681b6750b8a033a0490bca0941.

//! `localeconv`.

/// The `struct lconv` fields as Rust slices and signed bytes.
pub struct LConvData {
    /// Decimal point.
    pub decimal_point: &'static [u8],
    /// Thousands separator.
    pub thousands_sep: &'static [u8],
    /// Grouping.
    pub grouping: &'static [u8],
    /// International currency symbol.
    pub int_curr_symbol: &'static [u8],
    /// Currency symbol.
    pub currency_symbol: &'static [u8],
    /// Monetary decimal point.
    pub mon_decimal_point: &'static [u8],
    /// Monetary thousands separator.
    pub mon_thousands_sep: &'static [u8],
    /// Monetary grouping.
    pub mon_grouping: &'static [u8],
    /// Positive sign.
    pub positive_sign: &'static [u8],
    /// Negative sign.
    pub negative_sign: &'static [u8],
    /// International fractional digits.
    pub int_frac_digits: i8,
    /// Fractional digits.
    pub frac_digits: i8,
    /// Positive currency precedes.
    pub p_cs_precedes: i8,
    /// Positive separated by space.
    pub p_sep_by_space: i8,
    /// Negative currency precedes.
    pub n_cs_precedes: i8,
    /// Negative separated by space.
    pub n_sep_by_space: i8,
    /// Positive sign position.
    pub p_sign_posn: i8,
    /// Negative sign position.
    pub n_sign_posn: i8,
    /// International positive currency precedes.
    pub int_p_cs_precedes: i8,
    /// International positive separated by space.
    pub int_p_sep_by_space: i8,
    /// International negative currency precedes.
    pub int_n_cs_precedes: i8,
    /// International negative separated by space.
    pub int_n_sep_by_space: i8,
    /// International positive sign position.
    pub int_p_sign_posn: i8,
    /// International negative sign position.
    pub int_n_sign_posn: i8,
}

const MAX: i8 = i8::MAX;

static LCONV: LConvData = LConvData {
    decimal_point: b".\0",
    thousands_sep: b"\0",
    grouping: b"\0",
    int_curr_symbol: b"\0",
    currency_symbol: b"\0",
    mon_decimal_point: b"\0",
    mon_thousands_sep: b"\0",
    mon_grouping: b"\0",
    positive_sign: b"\0",
    negative_sign: b"\0",
    int_frac_digits: MAX,
    frac_digits: MAX,
    p_cs_precedes: MAX,
    p_sep_by_space: MAX,
    n_cs_precedes: MAX,
    n_sep_by_space: MAX,
    p_sign_posn: MAX,
    n_sign_posn: MAX,
    int_p_cs_precedes: MAX,
    int_p_sep_by_space: MAX,
    int_n_cs_precedes: MAX,
    int_n_sep_by_space: MAX,
    int_p_sign_posn: MAX,
    int_n_sign_posn: MAX,
};

/// The shared POSIX `lconv`.
#[must_use]
pub fn localeconv() -> &'static LConvData {
    &LCONV
}

#[cfg(test)]
#[path = "localeconv_ut.rs"]
mod tests;
