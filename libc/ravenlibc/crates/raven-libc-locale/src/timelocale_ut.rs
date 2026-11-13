// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! `TimeLocale` tests.

use super::*;

#[test]
fn the_c_time_table_is_posix() {
    let locale = &C_TIME;
    assert_eq!(locale.day(0), "Sunday");
    assert_eq!(locale.abday(6), "Sat");
    assert_eq!(locale.mon(0), "January");
    assert_eq!(locale.abmon(11), "Dec");
    assert_eq!(locale.am_pm(0), "AM");
    assert_eq!(locale.d_t_fmt(), "%a %b %e %T %Y");
    assert_eq!(locale.d_fmt(), "%m/%d/%y");
    assert_eq!(locale.t_fmt(), "%H:%M:%S");
    assert_eq!(locale.t_fmt_ampm(), "%I:%M:%S %p");
    assert_eq!(locale.era_d_t_fmt(), "%a %b %e %T %Y");
}

#[test]
fn c_and_c_utf8_share_the_table() {
    let table = NameTable::new();
    let c = time_locale(&Locale::C, &table) as *const dyn TimeLocale;
    let utf8 = time_locale(&Locale::CUTF8, &table) as *const dyn TimeLocale;
    assert!(core::ptr::eq(c, utf8));
    assert_eq!(time_locale(&Locale::C, &table).day(1), "Monday");
}
