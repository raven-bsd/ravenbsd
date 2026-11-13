// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C locale time-string tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

#[test]
fn the_c_locale_has_the_posix_strings() {
    let locale = CLocale;
    assert_eq!(locale.day(0), "Sunday");
    assert_eq!(locale.day(6), "Saturday");
    assert_eq!(locale.abday(0), "Sun");
    assert_eq!(locale.mon(0), "January");
    assert_eq!(locale.mon(11), "December");
    assert_eq!(locale.abmon(0), "Jan");
    assert_eq!(locale.am_pm(0), "AM");
    assert_eq!(locale.am_pm(1), "PM");
    assert_eq!(locale.d_t_fmt(), "%a %b %e %H:%M:%S %Y");
    assert_eq!(locale.d_fmt(), "%m/%d/%y");
    assert_eq!(locale.t_fmt(), "%H:%M:%S");
    assert_eq!(locale.t_fmt_ampm(), "%I:%M:%S %p");
}

#[test]
fn out_of_range_indexes_are_empty() {
    let locale = CLocale;
    assert_eq!(locale.day(7), "");
    assert_eq!(locale.mon(12), "");
    assert_eq!(locale.am_pm(2), "");
}
