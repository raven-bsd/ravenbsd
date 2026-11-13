// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! `langinfo` tests.

use super::*;
use crate::locale::{EnvSource, LocaleState};

struct Empty;
impl EnvSource for Empty {
    fn get(&self, _name: &[u8]) -> Option<&[u8]> {
        None
    }
}

#[test]
fn codeset_is_c_or_utf8() {
    let table = NameTable::new();
    assert_eq!(
        nl_langinfo(CODESET, &Locale::C, &table),
        b"ANSI_X3.4-1968\0"
    );
    assert_eq!(nl_langinfo(CODESET, &Locale::CUTF8, &table), b"UTF-8\0");
}

#[test]
fn the_time_table_has_the_musl_entries() {
    let table = NameTable::new();
    let expected: [&[u8]; 50] = [
        b"Sun\0",
        b"Mon\0",
        b"Tue\0",
        b"Wed\0",
        b"Thu\0",
        b"Fri\0",
        b"Sat\0",
        b"Sunday\0",
        b"Monday\0",
        b"Tuesday\0",
        b"Wednesday\0",
        b"Thursday\0",
        b"Friday\0",
        b"Saturday\0",
        b"Jan\0",
        b"Feb\0",
        b"Mar\0",
        b"Apr\0",
        b"May\0",
        b"Jun\0",
        b"Jul\0",
        b"Aug\0",
        b"Sep\0",
        b"Oct\0",
        b"Nov\0",
        b"Dec\0",
        b"January\0",
        b"February\0",
        b"March\0",
        b"April\0",
        b"May\0",
        b"June\0",
        b"July\0",
        b"August\0",
        b"September\0",
        b"October\0",
        b"November\0",
        b"December\0",
        b"AM\0",
        b"PM\0",
        b"%a %b %e %T %Y\0",
        b"%m/%d/%y\0",
        b"%H:%M:%S\0",
        b"%I:%M:%S %p\0",
        b"\0",
        b"\0",
        b"%m/%d/%y\0",
        b"0123456789\0",
        b"%a %b %e %T %Y\0",
        b"%H:%M:%S\0",
    ];
    for (index, value) in expected.iter().enumerate() {
        let item = (LC_TIME as u32) << 16 | index as u32;
        assert_eq!(
            nl_langinfo(item, &Locale::C, &table),
            *value,
            "item {index}"
        );
    }
}

#[test]
fn numeric_and_messages_items() {
    let table = NameTable::new();
    assert_eq!(nl_langinfo(RADIXCHAR, &Locale::C, &table), b".\0");
    assert_eq!(nl_langinfo(THOUSEP, &Locale::C, &table), b"\0");
    assert_eq!(nl_langinfo(YESEXPR, &Locale::C, &table), b"^[yY]\0");
    assert_eq!(nl_langinfo(NOEXPR, &Locale::C, &table), b"^[nN]\0");
    assert_eq!(nl_langinfo(YESSTR, &Locale::C, &table), b"yes\0");
    assert_eq!(nl_langinfo(NOSTR, &Locale::C, &table), b"no\0");
}

#[test]
fn unknown_items_are_empty() {
    let table = NameTable::new();
    assert_eq!(nl_langinfo(0xffff, &Locale::C, &table), b"\0");
    assert_eq!(
        nl_langinfo((LC_MONETARY as u32) << 16 | 3, &Locale::C, &table),
        b"\0"
    );
}

#[test]
fn locale_name_is_separate() {
    let mut state = LocaleState::new();
    let _ = state.setlocale(LC_TIME, Some(b"en_US.UTF-8"), &Empty);
    let item = (LC_TIME as u32) << 16 | 0xffff;
    assert_eq!(
        nl_langinfo_name(item, &state.global(), state.table()),
        Some(b"en_US.UTF-8".as_slice())
    );
    assert_eq!(
        nl_langinfo_name(CODESET, &state.global(), state.table()),
        None
    );
}
